//! Direct and native x86-64 WAITPKG semantic and fault-handoff coverage.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const STATUS_FLAGS: u64 = 0x08D5;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | STATUS_FLAGS | (1 << 10);
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    {
        vcpu.set_jit_mem(false);
        vcpu.set_jit_call(false);
    }
    vcpu
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct WAITPKG instruction").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct WAITPKG execution did not reach {target:#x}");
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
fn direct_waitpkg_probes_umonitor_memory_and_clears_only_wait_status_flags() {
    let memory = memory_with_code(&[
        0xF3, 0x0F, 0xAE, 0xF3, // UMONITOR RBX
        0xF2, 0x0F, 0xAE, 0xF1, // UMWAIT ECX
        0x66, 0x0F, 0xAE, 0xF6, // TPAUSE ESI
    ]);
    memory.write_slice(&[0xA5], GuestAddress(0x3000)).unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rbx = 0x3000;
    vcpu.regs.rcx = 1;
    vcpu.regs.rsi = 0;
    vcpu.regs.rax = 0x1122_3344_5566_7788;
    vcpu.regs.rdx = 0x8877_6655_4433_2211;
    let before = vcpu.regs.clone();

    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.regs.rip, 4);
    assert_eq!(vcpu.regs.rflags, before.rflags);
    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.regs.rip, 8);
    assert_eq!(vcpu.regs.rflags & STATUS_FLAGS, 0);
    assert_eq!(vcpu.regs.rflags & (1 << 10), 1 << 10);
    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.regs.rip, 12);
    assert_eq!(gprs(&vcpu.regs), gprs(&before));
    assert_eq!(vcpu.regs.rflags & STATUS_FLAGS, 0);
    assert_eq!(vcpu.regs.rflags & (1 << 10), 1 << 10);
}

#[test]
fn direct_waitpkg_faults_are_precise_and_noncommitting() {
    for code in [&[0x66, 0x0F, 0xAE, 0xF1][..], &[0xF2, 0x0F, 0xAE, 0xF1]] {
        for (control, cr4, cpl) in [(2_u64, 0_u64, 0_u16), (0, 1 << 2, 3)] {
            let mut vcpu = test_vcpu(memory_with_code(code));
            vcpu.regs.rcx = control;
            vcpu.sregs.cr4 = cr4;
            vcpu.sregs.cs.selector = cpl;
            let before = vcpu.regs.clone();
            let error = exception_without_idt(&mut vcpu);
            assert!(error.contains("IDT entry 13 not present"), "{error}");
            assert_regs_equal(&vcpu.regs, &before);
        }
    }

    let mut memory_fault = test_vcpu(memory_with_code(&[0xF3, 0x0F, 0xAE, 0xF0]));
    memory_fault.regs.rax = 0x20_000;
    let before = memory_fault.regs.clone();
    assert!(memory_fault.step().is_err());
    assert_regs_equal(&memory_fault.regs, &before);

    for (code, vector) in [
        (&[0xF3, 0x0F, 0xAE, 0xF0][..], 13),
        (&[0x36, 0xF3, 0x0F, 0xAE, 0xF0], 12),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(code));
        vcpu.regs.rax = 0x0000_8000_0000_0000;
        let before = vcpu.regs.clone();
        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{error}"
        );
        assert_regs_equal(&vcpu.regs, &before);
    }
}

#[test]
fn direct_waitpkg_tsd_bypasses_are_exact() {
    for (cr0, cr4, cpl) in [(0_u64, 1_u64 << 2, 3_u16), (1, 0, 3), (1, 1 << 2, 0)] {
        let mut vcpu = test_vcpu(memory_with_code(&[0xF2, 0x0F, 0xAE, 0xF1]));
        vcpu.regs.rcx = 1;
        vcpu.sregs.cr0 = cr0;
        vcpu.sregs.cr4 = cr4;
        vcpu.sregs.cs.selector = cpl;
        assert!(vcpu.step().unwrap().is_none());
        assert_eq!(vcpu.regs.rip, 4);
        assert_eq!(vcpu.regs.rflags & STATUS_FLAGS, 0);
    }
}

#[test]
fn direct_waitpkg_non_long_address_sizes_and_virtual_8086_tsd_are_exact() {
    for (code, rbx, fs_base, expected_rip) in [
        (&[0xF3, 0x0F, 0xAE, 0xF3][..], 0xFFFF_FFFF_0000_3000, 0, 4),
        (&[0x67, 0xF3, 0x0F, 0xAE, 0xF3], 0xFFFF_FFFF_0001_3000, 0, 5),
        (
            &[0x64, 0xF3, 0x0F, 0xAE, 0xF3],
            0xFFFF_FFFF_0000_1000,
            0x2000,
            5,
        ),
    ] {
        let memory = Arc::new(
            GuestMemoryMmap::<()>::from_ranges(&[
                (GuestAddress(0), 0x1000),
                (GuestAddress(0x3000), 0x1000),
            ])
            .unwrap(),
        );
        memory.write_slice(code, GuestAddress(0)).unwrap();
        memory.write_slice(&[0xA5], GuestAddress(0x3000)).unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cs.l = false;
        vcpu.sregs.cs.db = true;
        vcpu.sregs.fs.base = fs_base;
        vcpu.regs.rbx = rbx;
        assert!(vcpu.step().unwrap().is_none());
        assert_eq!(vcpu.regs.rip, expected_rip);
    }

    let mut real = test_vcpu(memory_with_code(&[0xF2, 0x0F, 0xAE, 0xF1]));
    real.sregs.cr0 = 0;
    real.sregs.cr4 = 1 << 2;
    real.sregs.cs.l = false;
    real.sregs.cs.db = false;
    real.sregs.cs.selector = 3;
    real.regs.rcx = 0xFFFF_FFFF_0000_0001;
    assert!(real.step().unwrap().is_none());
    assert_eq!(real.regs.rip, 4);
    assert_eq!(real.regs.rflags & STATUS_FLAGS, 0);

    let mut virtual_8086 = test_vcpu(memory_with_code(&[0xF2, 0x0F, 0xAE, 0xF1]));
    virtual_8086.sregs.cr4 = 1 << 2;
    virtual_8086.sregs.cs.l = false;
    virtual_8086.sregs.cs.db = true;
    virtual_8086.sregs.cs.selector = 0;
    virtual_8086.regs.rflags |= flags::bits::VM;
    virtual_8086.regs.rcx = 1;
    let before = virtual_8086.regs.clone();
    let error = exception_without_idt(&mut virtual_8086);
    assert!(error.contains("IDT entry 13 not present"), "{error}");
    assert_regs_equal(&virtual_8086.regs, &before);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_waitpkg_matches_direct_for_monitor_wait_pause_and_apx_egpr() {
    let memory = memory_with_code(&[
        0xF3, 0x0F, 0xAE, 0xF3, // UMONITOR RBX
        0xF2, 0x0F, 0xAE, 0xF1, // UMWAIT ECX
        0x66, 0xD5, 0x90, 0xAE, 0xF0, // TPAUSE R16D
        0xEB, 0x00, // JMP HLT
        0xF4,
    ]);
    memory.write_slice(&[0x5A], GuestAddress(0x3000)).unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rbx = 0x3000;
        vcpu.regs.rcx = 1;
        vcpu.regs.r16 = 0;
        vcpu.regs.rax = 0x1122_3344_5566_7788;
        vcpu.regs.rdx = 0x8877_6655_4433_2211;
        vcpu.set_apx_enabled(true);
        vcpu.set_jit_mem(true);
    }

    run_direct_to(&mut direct, 15);
    let region = native
        .jit_compile_region()
        .expect("compile WAITPKG region")
        .expect("helper-backed WAITPKG region must be native eligible");
    native.jit_run_region_native(&region);

    assert_regs_equal(&native.regs, &direct.regs);
    assert_eq!(native.regs.rip, 15);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_umonitor_matches_direct_for_addr32_segments_rex_and_rex2() {
    for (code, target) in [
        (&[0xF3, 0x67, 0x0F, 0xAE, 0xF3, 0xEB, 0x00, 0xF4][..], 7), // UMONITOR EBX
        (&[0x64, 0xF3, 0x41, 0x0F, 0xAE, 0xF7, 0xEB, 0x00, 0xF4], 8), // UMONITOR FS:R15
        (&[0xF3, 0xD5, 0x91, 0xAE, 0xF7, 0xEB, 0x00, 0xF4], 7),     // UMONITOR R31
        (&[0x36, 0xF3, 0x0F, 0xAE, 0xF6, 0xEB, 0x00, 0xF4], 7),     // UMONITOR SS:RSI
    ] {
        let memory = memory_with_code(code);
        memory.write_slice(&[0xA5], GuestAddress(0x3000)).unwrap();
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.regs.rbx = 0xFFFF_FFFF_0000_3000;
            vcpu.regs.r15 = 0x100;
            vcpu.regs.r31 = 0x3000;
            vcpu.regs.rsi = 0x3000;
            // Avoid the Apple linux/amd64 bridge's imported-AF discrepancy;
            // direct UMONITOR flag preservation is covered above.
            vcpu.regs.rflags &= !(1 << 4);
            vcpu.sregs.fs.base = 0x2F00;
            vcpu.set_apx_enabled(true);
            vcpu.set_jit_mem(true);
        }

        run_direct_to(&mut direct, target);
        let region = native
            .jit_compile_region()
            .expect("compile addressed UMONITOR")
            .expect("all UMONITOR address/register forms must be native eligible");
        native.jit_run_region_native(&region);
        assert_regs_equal(&native.regs, &direct.regs);
        assert_eq!(native.regs.rip, target);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_rex2_waitpkg_apx_guard_precedes_control_validation() {
    for (apx_enabled, control, vector) in [(false, 0_u64, 6), (false, 2, 6), (true, 2, 13)] {
        let mut vcpu = test_vcpu(memory_with_code(&[
            0xF2, 0xD5, 0x90, 0xAE, 0xF0, // UMWAIT R16D
            0xEB, 0x00, 0xF4,
        ]));
        vcpu.set_apx_enabled(apx_enabled);
        vcpu.regs.r16 = control;
        vcpu.regs.rflags &= !(1 << 4);
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile guarded REX2 UMWAIT")
            .expect("dynamic APX/control faults must not block native admission");
        vcpu.jit_run_region_native(&region);
        assert_regs_equal(&vcpu.regs, &before);

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{error}"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_waitpkg_dynamic_fault_guards_handoff_before_flag_commit() {
    for (control, cr4, cpl) in [(2_u64, 0_u64, 0_u16), (0, 1 << 2, 3), (2, 1 << 2, 3)] {
        let mut vcpu = test_vcpu(memory_with_code(&[
            0xF2, 0x0F, 0xAE, 0xF1, // UMWAIT ECX
            0xEB, 0x00, 0xF4,
        ]));
        vcpu.regs.rcx = control;
        vcpu.sregs.cr4 = cr4;
        vcpu.sregs.cs.selector = cpl;
        // Avoid the Apple linux/amd64 bridge's imported-AF discrepancy; every
        // remaining status/control flag is still a noncommitting input.
        vcpu.regs.rflags &= !(1 << 4);
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile guarded UMWAIT")
            .expect("dynamic WAITPKG faults must not block native admission");
        vcpu.jit_run_region_native(&region);
        assert_regs_equal(&vcpu.regs, &before);

        let error = exception_without_idt(&mut vcpu);
        assert!(error.contains("IDT entry 13 not present"), "{error}");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_umonitor_memory_and_noncanonical_fault_handoffs_are_precise() {
    for (code, rax, vector) in [
        (
            &[0xF3, 0x0F, 0xAE, 0xF0, 0xEB, 0x00, 0xF4][..],
            0x20_000,
            None,
        ),
        (
            &[0x36, 0xF3, 0x0F, 0xAE, 0xF0, 0xEB, 0x00, 0xF4],
            0x0000_8000_0000_0000,
            Some(12),
        ),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(code));
        vcpu.regs.rax = rax;
        vcpu.regs.rflags &= !(1 << 4);
        vcpu.set_jit_mem(true);
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile faulting UMONITOR")
            .expect("dynamic UMONITOR faults must not block native admission");
        vcpu.jit_run_region_native(&region);
        assert_regs_equal(&vcpu.regs, &before);

        if let Some(vector) = vector {
            let error = exception_without_idt(&mut vcpu);
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "{error}"
            );
        } else {
            assert!(vcpu.step().is_err());
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_waitpkg_memory_mode_and_long_mode_admission_are_fail_closed() {
    let mut monitor = test_vcpu(memory_with_code(&[
        0xF3, 0x0F, 0xAE, 0xF0, 0xEB, 0x00, 0xF4,
    ]));
    monitor.regs.rax = 0x2000;
    assert!(monitor.jit_compile_region().unwrap().is_none());

    let mut wait = test_vcpu(memory_with_code(&[
        0xF2, 0x0F, 0xAE, 0xF1, 0xEB, 0x00, 0xF4,
    ]));
    wait.regs.rcx = 0;
    assert!(wait.jit_compile_region().unwrap().is_some());

    for vcpu in [&mut monitor, &mut wait] {
        vcpu.sregs.cs.l = false;
        vcpu.sregs.cs.db = true;
        vcpu.set_jit_mem(true);
        assert!(
            vcpu.jit_compile_region().unwrap().is_none(),
            "compatibility-mode WAITPKG must remain an interpreter frontier"
        );
    }
}
