//! Native x86-64 JIT differentials for deterministic guest-PMC reads.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const PMC_MASK: u64 = (1_u64 << 40) - 1;

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rbx = 0xBBBB_BBBB_BBBB_BBBB;
    vcpu.regs.r8 = 0x8888_8888_8888_8888;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.r16 = 0x1616_1616_1616_1616;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn pmc(regs: &Registers) -> u64 {
    (regs.rdx << 32) | regs.rax
}

fn wrapping_interval_contains(start: u64, end: u64, value: u64, mask: u64) -> bool {
    let start = start & mask;
    let end = end & mask;
    let value = value & mask;
    if start <= end {
        (start..=end).contains(&value)
    } else {
        value >= start || value <= end
    }
}

#[test]
fn jit_rdpmc_uses_unadjusted_guest_clock_and_preserves_handoff_state() {
    for selector in [7_u64, 0xFFFF_FFFF_0000_0007, 0x8000_0000] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x33, 0xEB, 0x00, 0xF4], GuestAddress(0))
            .unwrap();
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.regs.rax = u64::MAX;
            vcpu.regs.rcx = selector;
            vcpu.regs.rdx = u64::MAX;
            vcpu.tsc_adjust = 0x1234_5678_9ABC_DEF0;
        }
        let preserved = [
            native.regs.rflags,
            native.regs.rcx,
            native.regs.rbx,
            native.regs.rsp,
            native.regs.rbp,
            native.regs.r8,
            native.regs.r15,
            native.regs.r16,
            native.regs.r31,
        ];

        let direct_before = direct.tsc().wrapping_sub(direct.tsc_adjust);
        assert!(direct.step().expect("direct RDPMC").is_none());
        let direct_value = pmc(&direct.regs);
        let direct_after = direct.tsc().wrapping_sub(direct.tsc_adjust);
        let width_mask = if selector as u32 & (1 << 31) != 0 {
            u64::from(u32::MAX)
        } else {
            PMC_MASK
        };
        assert!(wrapping_interval_contains(
            direct_before,
            direct_after,
            direct_value,
            width_mask
        ));
        assert!(direct.step().expect("direct handoff branch").is_none());

        let region = native
            .jit_compile_region()
            .expect("compile RDPMC region")
            .expect("RDPMC must be native eligible");
        let native_before = native.tsc().wrapping_sub(native.tsc_adjust);
        native.jit_run_region_native(&region);
        let native_value = pmc(&native.regs);
        let native_after = native.tsc().wrapping_sub(native.tsc_adjust);
        assert!(wrapping_interval_contains(
            native_before,
            native_after,
            native_value,
            width_mask
        ));
        assert_eq!(native.regs.rax >> 32, 0);
        assert_eq!(native.regs.rdx >> 32, 0);
        assert_eq!(
            [
                native.regs.rflags,
                native.regs.rcx,
                native.regs.rbx,
                native.regs.rsp,
                native.regs.rbp,
                native.regs.r8,
                native.regs.r15,
                native.regs.r16,
                native.regs.r31,
            ],
            preserved
        );
        assert_eq!(native.regs.rip, 4);
    }
}

#[test]
fn jit_rdpmc_dynamic_fault_handoff_is_precise_and_noncommitting() {
    for (selector, cr0, cr4, cpl, virtual_8086) in [
        (8_u64, 1_u64, 1_u64 << 8, 3_u16, false),
        (0, 1, 0, 3, false),
        (0, 1, 0, 0, true),
        (0x4000_0000, 1, 0, 0, false),
    ] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x33, 0xEB, 0x00, 0xF4], GuestAddress(0))
            .unwrap();
        let mut native = test_vcpu(memory);
        native.sregs.cr0 = cr0;
        native.sregs.cr4 = cr4;
        native.sregs.cs.selector = cpl;
        native.sregs.cs.dpl = cpl as u8;
        if virtual_8086 {
            native.regs.rflags |= 1 << 17;
        }
        native.regs.rax = 0x1111;
        native.regs.rcx = selector;
        native.regs.rdx = 0x3333;

        let region = native
            .jit_compile_region()
            .expect("compile guarded RDPMC region")
            .expect("dynamic RDPMC faults must not prevent native admission");
        native.jit_run_region_native(&region);

        assert_eq!(native.regs.rip, 0);
        assert_eq!(native.regs.rax, 0x1111);
        assert_eq!(native.regs.rcx, selector);
        assert_eq!(native.regs.rdx, 0x3333);
    }
}

#[test]
fn jit_rdpmc_pce_and_real_mode_bypasses_execute_natively() {
    for (cr0, cr4, cpl) in [(1_u64, 1_u64 << 8, 3_u16), (0, 0, 3), (1, 0, 0)] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x33, 0xEB, 0x00, 0xF4], GuestAddress(0))
            .unwrap();
        let mut native = test_vcpu(memory);
        native.sregs.cr0 = cr0;
        native.sregs.cr4 = cr4;
        native.sregs.cs.selector = cpl;
        native.sregs.cs.dpl = cpl as u8;
        native.regs.rcx = 0;

        let region = native
            .jit_compile_region()
            .expect("compile permitted RDPMC region")
            .expect("permitted RDPMC must be native eligible");
        native.jit_run_region_native(&region);
        assert_eq!(native.regs.rip, 4);
    }
}

#[test]
fn jit_verify_skips_impossible_rdpmc_clock_replay() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x33, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rcx = 0;
    let region = vcpu
        .jit_compile_region()
        .expect("compile RDPMC verify region")
        .expect("RDPMC verify region must be native eligible");
    assert!(region.uses_timestamp);

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 4);
    assert_eq!(vcpu.regs.rax >> 32, 0);
    assert_eq!(vcpu.regs.rdx >> 32, 0);
}
