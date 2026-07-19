//! Native x86-64 JIT differentials for PKRU state and fault handoffs.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr4 = 1 << 22;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct PKRU instruction").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

#[test]
fn jit_pkru_matches_direct_and_commits_through_the_vcpu_abi() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // WRPKRU; RDPKRU; JMP HLT; HLT.
    memory
        .write_slice(
            &[0x0F, 0x01, 0xEF, 0x0F, 0x01, 0xEE, 0xEB, 0x00, 0xF4],
            GuestAddress(0),
        )
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = 0xFFFF_FFFF_89AB_CDEF;
        vcpu.regs.rcx = 0x1357_9BDF_0000_0000;
        vcpu.regs.rdx = 0x2468_ACE0_0000_0000;
        vcpu.pkru = 0x1234_5678;
    }

    run_direct_to(&mut direct, 8);
    let region = native
        .jit_compile_region()
        .expect("compile PKRU region")
        .expect("PKRU must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.pkru, direct.pkru);
    assert_eq!(native.pkru, 0x89AB_CDEF);
    assert_eq!(native.regs.rax, direct.regs.rax);
    assert_eq!(native.regs.rcx, direct.regs.rcx);
    assert_eq!(native.regs.rdx, direct.regs.rdx);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, direct.regs.rip);
    assert_eq!(native.regs.rip, 8);
}

#[test]
fn jit_verify_path_snapshots_compares_and_adopts_pkru_state() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(
            &[0x0F, 0x01, 0xEF, 0x0F, 0x01, 0xEE, 0xEB, 0x00, 0xF4],
            GuestAddress(0),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 0x89AB_CDEF;
    vcpu.regs.rcx = 0;
    vcpu.regs.rdx = 0;
    vcpu.pkru = 0x1234_5678;
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified PKRU region")
        .expect("verified PKRU region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.pkru, 0x89AB_CDEF);
    assert_eq!(vcpu.regs.rax, 0x89AB_CDEF);
    assert_eq!(vcpu.regs.rdx, 0);
    assert_eq!(vcpu.regs.rip, 8);
}

#[test]
fn jit_pkru_dynamic_faults_handoff_without_partial_commit() {
    for (code, cr4, ecx, edx) in [
        (&[0x0F, 0x01, 0xEE, 0xEB, 0x00, 0xF4][..], 0, 0, 0),
        (&[0x0F, 0x01, 0xEE, 0xEB, 0x00, 0xF4], 1 << 22, 1, 0),
        (&[0x0F, 0x01, 0xEF, 0xEB, 0x00, 0xF4], 1 << 22, 1, 0),
        (&[0x0F, 0x01, 0xEF, 0xEB, 0x00, 0xF4], 1 << 22, 0, 1),
    ] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory.write_slice(code, GuestAddress(0)).unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cr4 = cr4;
        vcpu.regs.rax = 0xA5A5_A5A5_89AB_CDEF;
        vcpu.regs.rcx = ecx;
        vcpu.regs.rdx = edx;
        vcpu.pkru = 0x1234_5678;
        let before = (vcpu.regs.rax, vcpu.regs.rcx, vcpu.regs.rdx, vcpu.pkru);

        let region = vcpu
            .jit_compile_region()
            .expect("compile guarded PKRU region")
            .expect("dynamic PKRU faults must not prevent compilation");
        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0, "fault must hand off at instruction PC");
        assert_eq!(
            (vcpu.regs.rax, vcpu.regs.rcx, vcpu.regs.rdx, vcpu.pkru),
            before
        );
        assert!(
            vcpu.step().is_err(),
            "direct re-execution must deliver the architectural exception"
        );
    }
}

#[test]
fn jit_pkru_state_is_coherent_across_interpreter_callouts() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // WRPKRU; CALL 100h; RDPKRU; JMP HLT; HLT.
    memory
        .write_slice(
            &[
                0x0F, 0x01, 0xEF, 0xE8, 0xF8, 0x00, 0x00, 0x00, 0x0F, 0x01, 0xEE, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    // RDPKRU; MOV EBX,EAX; MOV EAX,0x2468ACE0; WRPKRU; RET.
    memory
        .write_slice(
            &[
                0x0F, 0x01, 0xEE, 0x89, 0xC3, 0xB8, 0xE0, 0xAC, 0x68, 0x24, 0x0F, 0x01, 0xEF, 0xC3,
            ],
            GuestAddress(0x100),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.set_jit_call(true);
    vcpu.regs.rax = 0x1357_9BDF;
    vcpu.regs.rcx = 0;
    vcpu.regs.rdx = 0;
    vcpu.pkru = 0;

    let region = vcpu
        .jit_compile_region()
        .expect("compile callout PKRU region")
        .expect("PKRU callout sequence must be native eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rbx, 0x1357_9BDF, "callee did not see native PKRU");
    assert_eq!(vcpu.pkru, 0x2468_ACE0);
    assert_eq!(
        vcpu.regs.rax, vcpu.pkru as u64,
        "native continuation lost callee PKRU"
    );
    assert_eq!(vcpu.regs.rdx, 0);
    assert_eq!(vcpu.regs.rsp, 0x8000);
    assert_eq!(vcpu.regs.rip, 13);
}
