//! Direct and native x86-64 differentials for MONITOR/MWAIT fault handoff.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 1;
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
            vcpu.step()
                .expect("direct MONITOR/MWAIT instruction")
                .is_none(),
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

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn assert_regs_equal(actual: &Registers, expected: &Registers) {
    assert_eq!(gprs(actual), gprs(expected));
    assert_eq!(actual.rip, expected.rip);
    assert_eq!(actual.rflags, expected.rflags);
    assert_eq!(actual.xmm, expected.xmm);
    assert_eq!(actual.ymm_high, expected.ymm_high);
    assert_eq!(actual.zmm_high, expected.zmm_high);
    assert_eq!(actual.zmm_ext, expected.zmm_ext);
    assert_eq!(actual.k, expected.k);
    assert_eq!(actual.mm, expected.mm);
}

#[test]
fn jit_monitor_mwait_matches_direct_and_preserves_architectural_state() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // MONITOR; MWAIT; JMP HLT; HLT.
    memory
        .write_slice(
            &[0x0F, 0x01, 0xC8, 0x0F, 0x01, 0xC9, 0xEB, 0x00, 0xF4],
            GuestAddress(0),
        )
        .unwrap();
    memory.write_slice(&[0xA5], GuestAddress(0x3000)).unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = 0x3000;
        vcpu.regs.rbx = 0x1122_3344_5566_7788;
        vcpu.regs.rcx = 0;
        vcpu.regs.rdx = 0xA5A5_5A5A_1234_5678;
        vcpu.set_jit_mem(true);
    }

    run_direct_to(&mut direct, 8);
    let region = native
        .jit_compile_region()
        .expect("compile MONITOR/MWAIT region")
        .expect("helper-backed MONITOR/MWAIT must be native eligible");
    native.jit_run_region_native(&region);

    assert_regs_equal(&native.regs, &direct.regs);
    assert_eq!(native.regs.rip, 8);
}

#[test]
fn jit_monitor_requires_memory_helpers_but_mwait_does_not() {
    let monitor_memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    monitor_memory
        .write_slice(&[0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut monitor = test_vcpu(monitor_memory);
    monitor.regs.rax = 0x2000;
    assert!(
        monitor.jit_compile_region().unwrap().is_none(),
        "MONITOR must not bypass the guest MMU"
    );

    let mwait_memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    mwait_memory
        .write_slice(&[0x0F, 0x01, 0xC9, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut mwait = test_vcpu(mwait_memory);
    assert!(
        mwait.jit_compile_region().unwrap().is_some(),
        "MWAIT has no memory access and must compile without JIT memory mode"
    );
}

#[test]
fn jit_monitor_mwait_fault_guards_handoff_before_direct_exception_delivery() {
    for (cpl, rcx, vector) in [(3_u16, 0_u64, 6), (0, 1, 13), (3, 1, 6)] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4], GuestAddress(0))
            .unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cs.selector = cpl;
        vcpu.regs.rax = 0x2000;
        vcpu.regs.rcx = rcx;
        vcpu.set_jit_mem(true);
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile guarded MONITOR region")
            .expect("dynamic architectural faults must not block compilation");
        vcpu.jit_run_region_native(&region);

        assert_regs_equal(&vcpu.regs, &before);
        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "unexpected exception priority: {error}"
        );
    }
}

#[test]
fn jit_monitor_memory_fault_handoff_is_precise_and_noncommitting() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 0x20_000;
    vcpu.regs.rcx = 0;
    vcpu.set_jit_mem(true);
    let before = vcpu.regs.clone();

    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting MONITOR region")
        .expect("dynamic memory faults must not block compilation");
    vcpu.jit_run_region_native(&region);

    assert_regs_equal(&vcpu.regs, &before);
    assert!(
        vcpu.step().is_err(),
        "direct restart must report the same invalid guest read"
    );
}

#[test]
fn jit_monitor_honors_addr32_and_fs_relative_effective_addresses() {
    for (code, rax, fs_base, target) in [
        (
            &[0x67, 0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4][..],
            0xFFFF_FFFF_0000_3000,
            0,
            6,
        ),
        (
            &[0x64, 0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4][..],
            0x100,
            0x2F00,
            6,
        ),
        (
            &[0x64, 0x67, 0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4][..],
            0xFFFF_FFFF_0000_0100,
            0x2F00,
            7,
        ),
        (
            &[0x36, 0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4][..],
            0x3000,
            0,
            6,
        ),
    ] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory.write_slice(code, GuestAddress(0)).unwrap();
        memory.write_slice(&[0x5A], GuestAddress(0x3000)).unwrap();
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.regs.rax = rax;
            vcpu.regs.rcx = 0;
            vcpu.sregs.fs.base = fs_base;
            vcpu.set_jit_mem(true);
        }

        run_direct_to(&mut direct, target);
        let region = native
            .jit_compile_region()
            .expect("compile addressed MONITOR")
            .expect("addr32/FS MONITOR must be native eligible");
        native.jit_run_region_native(&region);
        assert_regs_equal(&native.regs, &direct.regs);
        assert_eq!(native.regs.rip, target);
    }
}

#[test]
fn jit_monitor_noncanonical_ss_fault_handoff_preserves_exception_class() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x36, 0x0F, 0x01, 0xC8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 0x0000_8000_0000_0000;
    vcpu.regs.rcx = 0;
    vcpu.set_jit_mem(true);
    let before = vcpu.regs.clone();

    let region = vcpu
        .jit_compile_region()
        .expect("compile SS-prefixed MONITOR")
        .expect("dynamic canonicality faults must not block compilation");
    vcpu.jit_run_region_native(&region);
    assert_regs_equal(&vcpu.regs, &before);

    let error = exception_without_idt(&mut vcpu);
    assert!(
        error.contains("IDT entry 12 not present"),
        "native handoff changed #SS(0) to another fault: {error}"
    );
}

#[test]
fn jit_rejects_monitor_mwait_outside_cs_l() {
    for opcode in [0xC8, 0xC9] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x01, opcode, 0xEB, 0x00, 0xF4], GuestAddress(0))
            .unwrap();
        let mut compatibility = test_vcpu(memory);
        compatibility.sregs.cs.l = false;
        compatibility.sregs.cs.db = true;
        compatibility.regs.rax = 0x2000;
        compatibility.set_jit_mem(true);
        assert!(
            compatibility.jit_compile_region().unwrap().is_none(),
            "compatibility-mode MONITOR/MWAIT must remain an interpreter frontier"
        );
    }
}
