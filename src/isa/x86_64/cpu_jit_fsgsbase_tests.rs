//! Native x86-64 JIT differentials for FSGSBASE state and fault handoffs.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr4 = 1 << 16;
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
            vcpu.step().expect("direct FSGSBASE instruction").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

#[test]
fn jit_fsgsbase_matches_direct_and_commits_segment_bases_through_the_vcpu_abi() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // WRFSBASE RAX; RDFSBASE RBX; WRGSBASE RCX; RDGSBASE RDX; JMP HLT; HLT.
    memory
        .write_slice(
            &[
                0xF3, 0x48, 0x0F, 0xAE, 0xD0, 0xF3, 0x48, 0x0F, 0xAE, 0xC3, 0xF3, 0x48, 0x0F, 0xAE,
                0xD9, 0xF3, 0x48, 0x0F, 0xAE, 0xCA, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = 0xFFFF_8000_89AB_CDEF;
        vcpu.regs.rcx = 0x0000_7FFF_7654_3210;
        vcpu.regs.rbx = 0x1111;
        vcpu.regs.rdx = 0x2222;
    }

    run_direct_to(&mut direct, 22);
    let region = native
        .jit_compile_region()
        .expect("compile FSGSBASE region")
        .expect("FSGSBASE must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(
        [
            native.regs.rax,
            native.regs.rbx,
            native.regs.rcx,
            native.regs.rdx,
            native.regs.rsp,
            native.regs.rbp,
            native.regs.rflags,
            native.regs.rip,
        ],
        [
            direct.regs.rax,
            direct.regs.rbx,
            direct.regs.rcx,
            direct.regs.rdx,
            direct.regs.rsp,
            direct.regs.rbp,
            direct.regs.rflags,
            direct.regs.rip,
        ]
    );
    assert_eq!(native.sregs.fs.base, direct.sregs.fs.base);
    assert_eq!(native.sregs.gs.base, direct.sregs.gs.base);
    assert_eq!(native.regs.rip, 22);
}

#[test]
fn jit_verify_path_snapshots_compares_and_adopts_fsgsbase_state() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(
            &[
                0xF3, 0x48, 0x0F, 0xAE, 0xD0, // WRFSBASE RAX
                0xF3, 0x48, 0x0F, 0xAE, 0xC3, // RDFSBASE RBX
                0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 0xFFFF_8000_89AB_CDEF;
    vcpu.regs.rbx = 0;
    let region = vcpu
        .jit_compile_region()
        .expect("compile verify FSGSBASE region")
        .expect("verify FSGSBASE region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.sregs.fs.base, 0xFFFF_8000_89AB_CDEF);
    assert_eq!(vcpu.regs.rbx, vcpu.sregs.fs.base);
    assert_eq!(vcpu.regs.rip, 12);
}

#[test]
fn jit_rex2_fsgsbase_matches_direct_for_egpr_operands() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // WRFSBASE R16; RDFSBASE R31; JMP HLT; HLT.
    memory
        .write_slice(
            &[
                0xF3, 0xD5, 0x98, 0xAE, 0xD0, 0xF3, 0xD5, 0x99, 0xAE, 0xC7, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.regs.r16 = 0xFFFF_8000_7654_3210;
        vcpu.regs.r31 = 0x3131;
    }

    run_direct_to(&mut direct, 12);
    let region = native
        .jit_compile_region()
        .expect("compile REX2 FSGSBASE region")
        .expect("REX2 FSGSBASE must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.r16, direct.regs.r16);
    assert_eq!(native.regs.r31, direct.regs.r31);
    assert_eq!(native.sregs.fs.base, direct.sregs.fs.base);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 12);
}

#[test]
fn jit_wrfsbase_updates_same_region_segment_relative_memory_addressing() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // WRFSBASE RAX; MOV RBX,qword ptr FS:[0]; JMP HLT; HLT.
    memory
        .write_slice(
            &[
                0xF3, 0x48, 0x0F, 0xAE, 0xD0, 0x64, 0x48, 0x8B, 0x1C, 0x25, 0, 0, 0, 0, 0xEB, 0x00,
                0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    memory
        .write_slice(
            &0x0123_4567_89AB_CDEF_u64.to_le_bytes(),
            GuestAddress(0x3000),
        )
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = 0x3000;
        vcpu.regs.rbx = 0;
        vcpu.set_jit_mem(true);
    }

    run_direct_to(&mut direct, 16);
    let region = native
        .jit_compile_region()
        .expect("compile segmented FSGSBASE region")
        .expect("FSGSBASE plus SegmentRel load must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.fs.base, 0x3000);
    assert_eq!(native.regs.rbx, 0x0123_4567_89AB_CDEF);
    assert_eq!(native.regs.rbx, direct.regs.rbx);
    assert_eq!(native.regs.rip, 16);
}

#[test]
fn jit_fsgsbase_dynamic_faults_handoff_without_partial_commit() {
    for (code, configure, expected_base) in [
        (
            &[0xF3, 0x48, 0x0F, 0xAE, 0xC0, 0xEB, 0x00, 0xF4][..],
            0_u8,
            0x1234_u64,
        ), // CR4-clear RDFSBASE RAX
        (&[0xF3, 0x48, 0x0F, 0xAE, 0xD8, 0xEB, 0x00, 0xF4], 1, 0x2468), // noncanonical WRGSBASE RAX
        (&[0xF3, 0xD5, 0x98, 0xAE, 0xC0, 0xEB, 0x00, 0xF4], 2, 0x1234), // APX-disabled RDFSBASE R16
    ] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory.write_slice(code, GuestAddress(0)).unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.fs.base = 0x1234;
        vcpu.sregs.gs.base = 0x2468;
        vcpu.regs.rax = 0x0000_8000_0000_0000;
        vcpu.regs.r16 = 0x1616;
        match configure {
            0 => vcpu.sregs.cr4 = 0,
            1 => {}
            2 => vcpu.set_apx_enabled(false),
            _ => unreachable!(),
        }
        let before_rax = vcpu.regs.rax;
        let before_r16 = vcpu.regs.r16;

        let region = vcpu
            .jit_compile_region()
            .expect("compile guarded FSGSBASE region")
            .expect("dynamic fault conditions must not prevent compilation");
        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0, "fault must hand off at instruction PC");
        assert_eq!(vcpu.regs.rax, before_rax);
        assert_eq!(vcpu.regs.r16, before_r16);
        assert_eq!(vcpu.sregs.fs.base, 0x1234);
        assert_eq!(vcpu.sregs.gs.base, 0x2468);
        assert_eq!(
            if configure == 1 {
                vcpu.sregs.gs.base
            } else {
                vcpu.sregs.fs.base
            },
            expected_base
        );
        assert!(
            vcpu.step().is_err(),
            "direct re-execution must deliver the architectural exception"
        );
    }
}

#[test]
fn jit_rejects_fsgsbase_regions_outside_cs_l() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0xF3, 0x0F, 0xAE, 0xC0, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut long_mode = test_vcpu(memory.clone());
    assert!(
        long_mode.jit_compile_region().unwrap().is_some(),
        "long-mode FSGSBASE baseline must compile"
    );

    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode FSGSBASE must remain an interpreter frontier"
    );
    assert!(compatibility.step().is_err(), "direct path must inject #UD");
}

#[test]
fn jit_fsgsbase_state_is_coherent_across_interpreter_callouts() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // WRFSBASE RAX; CALL 100h; RDGSBASE RBX; JMP HLT; HLT.
    memory
        .write_slice(
            &[
                0xF3, 0x48, 0x0F, 0xAE, 0xD0, 0xE8, 0xF6, 0x00, 0x00, 0x00, 0xF3, 0x48, 0x0F, 0xAE,
                0xCB, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    // RDFSBASE RDX; WRGSBASE RCX; RET.
    memory
        .write_slice(
            &[
                0xF3, 0x48, 0x0F, 0xAE, 0xC2, 0xF3, 0x48, 0x0F, 0xAE, 0xD9, 0xC3,
            ],
            GuestAddress(0x100),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.set_jit_call(true);
    vcpu.regs.rax = 0xFFFF_8000_89AB_CDEF;
    vcpu.regs.rcx = 0x0000_7FFF_7654_3210;

    let region = vcpu
        .jit_compile_region()
        .expect("compile callout FSGSBASE region")
        .expect("FSGSBASE callout sequence must be native eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.sregs.fs.base, 0xFFFF_8000_89AB_CDEF);
    assert_eq!(
        vcpu.regs.rdx, vcpu.sregs.fs.base,
        "callee did not see native FS.base"
    );
    assert_eq!(vcpu.sregs.gs.base, 0x0000_7FFF_7654_3210);
    assert_eq!(
        vcpu.regs.rbx, vcpu.sregs.gs.base,
        "native continuation lost callee GS.base"
    );
    assert_eq!(vcpu.regs.rsp, 0x8000);
    assert_eq!(vcpu.regs.rip, 17);
}
