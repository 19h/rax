//! Native x86-64 JIT differentials for RDTSC/RDTSCP guest-clock reads.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

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
    vcpu.tsc_aux = 0x89AB_CDEF;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn timestamp(regs: &Registers) -> u64 {
    (regs.rdx << 32) | regs.rax
}

fn assert_native_timestamp_shape(opcode: &[u8], reads_aux: bool) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    let mut code = opcode.to_vec();
    code.extend_from_slice(&[0xEB, 0x00, 0xF4]); // JMP next; HLT frontier
    memory.write_slice(&code, GuestAddress(0)).unwrap();

    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = u64::MAX;
        vcpu.regs.rdx = u64::MAX;
        vcpu.regs.rcx = 0xCAFE_BABE_DEAD_BEEF;
    }
    let preserved = [
        native.regs.rflags,
        native.regs.rbx,
        native.regs.rsp,
        native.regs.rbp,
        native.regs.r8,
        native.regs.r15,
        native.regs.r16,
        native.regs.r31,
    ];

    let direct_before = direct.tsc();
    assert!(direct.step().expect("direct timestamp read").is_none());
    let direct_tsc = timestamp(&direct.regs);
    let direct_after = direct.tsc();
    assert!(
        (direct_before..=direct_after).contains(&direct_tsc),
        "direct timestamp {direct_tsc:#x} escaped guest-clock bracket \
         {direct_before:#x}..={direct_after:#x}"
    );
    assert!(direct.step().expect("direct handoff branch").is_none());

    let region = native
        .jit_compile_region()
        .expect("compile timestamp region")
        .expect("timestamp read must be native eligible");
    let native_before = native.tsc();
    native.jit_run_region_native(&region);
    let native_tsc = timestamp(&native.regs);
    let native_after = native.tsc();

    assert!(
        (native_before..=native_after).contains(&native_tsc),
        "native timestamp {native_tsc:#x} escaped guest-clock bracket \
         {native_before:#x}..={native_after:#x}"
    );
    assert_eq!(native.regs.rax >> 32, 0);
    assert_eq!(native.regs.rdx >> 32, 0);
    if reads_aux {
        assert_eq!(native.regs.rcx, u64::from(native.tsc_aux));
    } else {
        assert_eq!(native.regs.rcx, 0xCAFE_BABE_DEAD_BEEF);
    }
    assert_eq!(
        [
            native.regs.rflags,
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
    assert_eq!(native.regs.rip, opcode.len() as u64 + 2);
}

#[test]
fn jit_rdtsc_and_rdtscp_use_guest_clock_state_and_preserve_handoff_state() {
    assert_native_timestamp_shape(&[0x0F, 0x31], false);
    assert_native_timestamp_shape(&[0x0F, 0x01, 0xF9], true);
}

#[test]
fn jit_timestamp_tsd_fault_handoff_is_precise_and_noncommitting() {
    for opcode in [&[0x0F, 0x31][..], &[0x0F, 0x01, 0xF9]] {
        for virtual_8086 in [false, true] {
            let memory = Arc::new(
                GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap(),
            );
            let mut code = opcode.to_vec();
            code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
            memory.write_slice(&code, GuestAddress(0)).unwrap();
            let mut native = test_vcpu(memory);
            native.sregs.cr4 |= 1 << 2;
            if virtual_8086 {
                native.sregs.cs.selector = 0;
                native.sregs.cs.dpl = 0;
                native.regs.rflags |= 1 << 17;
            } else {
                native.sregs.cs.selector = 3;
                native.sregs.cs.dpl = 3;
            }
            native.regs.rax = 0x1111;
            native.regs.rcx = 0x2222;
            native.regs.rdx = 0x3333;

            let region = native
                .jit_compile_region()
                .expect("compile guarded timestamp region")
                .expect("dynamic TSD condition must not prevent native admission");
            native.jit_run_region_native(&region);

            assert_eq!(native.regs.rip, 0);
            assert_eq!(native.regs.rax, 0x1111);
            assert_eq!(native.regs.rcx, 0x2222);
            assert_eq!(native.regs.rdx, 0x3333);
        }
    }
}

#[test]
fn jit_verify_executes_timestamp_regions_without_impossible_clock_replay() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x01, 0xF9, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile timestamp verify region")
        .expect("timestamp verify region must be native eligible");
    assert!(region.uses_timestamp);

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 5);
    assert_eq!(vcpu.regs.rax >> 32, 0);
    assert_eq!(vcpu.regs.rdx >> 32, 0);
    assert_eq!(vcpu.regs.rcx, u64::from(vcpu.tsc_aux));
}
