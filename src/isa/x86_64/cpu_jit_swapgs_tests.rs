//! Native x86-64 JIT differentials for SWAPGS state and fault handoffs.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
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
            vcpu.step().expect("direct SWAPGS instruction").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

#[test]
fn jit_swapgs_matches_direct_and_commits_both_bases_through_the_vcpu_abi() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // SWAPGS; JMP HLT; HLT.
    memory
        .write_slice(&[0x0F, 0x01, 0xF8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.sregs.gs.base = 0x0000_7FFF_1234_5000;
        vcpu.kernel_gs_base = 0xFFFF_8000_ABCD_E000;
        vcpu.regs.rax = 0x1122_3344_5566_7788;
    }

    run_direct_to(&mut direct, 5);
    let region = native
        .jit_compile_region()
        .expect("compile SWAPGS region")
        .expect("SWAPGS must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.gs.base, direct.sregs.gs.base);
    assert_eq!(native.kernel_gs_base, direct.kernel_gs_base);
    assert_eq!(native.regs.rax, direct.regs.rax);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rbp, direct.regs.rbp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 5);
}

#[test]
fn jit_verify_path_snapshots_compares_and_adopts_swapgs_state() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x01, 0xF8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    let old_gs = 0x0000_7FFF_1234_5000;
    let old_kernel = 0xFFFF_8000_ABCD_E000;
    vcpu.sregs.gs.base = old_gs;
    vcpu.kernel_gs_base = old_kernel;
    let region = vcpu
        .jit_compile_region()
        .expect("compile verify SWAPGS region")
        .expect("verify SWAPGS region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.sregs.gs.base, old_kernel);
    assert_eq!(vcpu.kernel_gs_base, old_gs);
    assert_eq!(vcpu.regs.rip, 5);
}

#[test]
fn jit_swapgs_updates_same_region_gs_relative_memory_addressing() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // SWAPGS; MOV RBX,qword ptr GS:[0]; JMP HLT; HLT.
    memory
        .write_slice(
            &[
                0x0F, 0x01, 0xF8, 0x65, 0x48, 0x8B, 0x1C, 0x25, 0, 0, 0, 0, 0xEB, 0x00, 0xF4,
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
        vcpu.sregs.gs.base = 0x2000;
        vcpu.kernel_gs_base = 0x3000;
        vcpu.regs.rbx = 0;
        vcpu.set_jit_mem(true);
    }

    run_direct_to(&mut direct, 14);
    let region = native
        .jit_compile_region()
        .expect("compile GS-relative SWAPGS region")
        .expect("SWAPGS plus SegmentRel load must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.gs.base, 0x3000);
    assert_eq!(native.kernel_gs_base, 0x2000);
    assert_eq!(native.regs.rbx, 0x0123_4567_89AB_CDEF);
    assert_eq!(native.regs.rbx, direct.regs.rbx);
    assert_eq!(native.regs.rip, 14);
}

#[test]
fn jit_swapgs_cpl_fault_handoff_is_precise_noncommitting_and_becomes_gp() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x01, 0xF8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.sregs.cs.selector = 3;
    vcpu.sregs.gs.base = 0x1234;
    vcpu.kernel_gs_base = 0xFFFF_8000_0000_5678;
    let before_flags = vcpu.regs.rflags;

    let region = vcpu
        .jit_compile_region()
        .expect("compile guarded SWAPGS region")
        .expect("dynamic CPL must not prevent compilation");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.sregs.gs.base, 0x1234);
    assert_eq!(vcpu.kernel_gs_base, 0xFFFF_8000_0000_5678);
    assert_eq!(vcpu.regs.rflags, before_flags);
    assert!(
        exception_without_idt(&mut vcpu).contains("IDT entry 13 not present"),
        "long-mode CPL != 0 must deliver #GP(0)"
    );
}

#[test]
fn jit_rejects_swapgs_outside_cs_l_and_direct_path_prioritizes_ud() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x01, 0xF8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut long_mode = test_vcpu(memory.clone());
    assert!(
        long_mode.jit_compile_region().unwrap().is_some(),
        "long-mode SWAPGS baseline must compile"
    );

    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.sregs.cs.selector = 3;
    compatibility.sregs.gs.base = 0x1234;
    compatibility.kernel_gs_base = 0x5678;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode SWAPGS must remain an interpreter frontier"
    );
    assert!(
        exception_without_idt(&mut compatibility).contains("IDT entry 6 not present"),
        "CS.L=0 must deliver #UD before the simultaneous CPL violation"
    );
    assert_eq!(compatibility.sregs.gs.base, 0x1234);
    assert_eq!(compatibility.kernel_gs_base, 0x5678);
}

#[test]
fn jit_swapgs_state_is_coherent_across_interpreter_callouts() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // SWAPGS; CALL 100h; SWAPGS; JMP HLT; HLT.
    memory
        .write_slice(
            &[
                0x0F, 0x01, 0xF8, 0xE8, 0xF8, 0x00, 0x00, 0x00, 0x0F, 0x01, 0xF8, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    // SWAPGS; RET.
    memory
        .write_slice(&[0x0F, 0x01, 0xF8, 0xC3], GuestAddress(0x100))
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_jit_call(true);
        vcpu.sregs.gs.base = 0x0000_7FFF_1234_5000;
        vcpu.kernel_gs_base = 0xFFFF_8000_ABCD_E000;
    }

    run_direct_to(&mut direct, 13);
    let region = native
        .jit_compile_region()
        .expect("compile callout SWAPGS region")
        .expect("SWAPGS callout sequence must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.gs.base, direct.sregs.gs.base);
    assert_eq!(native.kernel_gs_base, direct.kernel_gs_base);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 13);
}
