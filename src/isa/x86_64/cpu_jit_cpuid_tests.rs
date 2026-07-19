//! Native x86-64 JIT differentials for deterministic guest CPUID.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr4 = (1 << 18) | (1 << 22);
    vcpu.xcr0 = 0x7 | (1 << 19);
    vcpu.set_xeon_phi_avx512_enabled(true);
    vcpu.set_vp2intersect_enabled(true);
    vcpu.set_sse4a_enabled(true);
    vcpu.set_apx_enabled(true);
    vcpu.regs.rip = 0;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.r8 = 0x8888_8888_8888_8888;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.r16 = 0x1616_1616_1616_1616;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn outputs(vcpu: &X86_64Vcpu) -> [u64; 4] {
    [vcpu.regs.rax, vcpu.regs.rbx, vcpu.regs.rcx, vcpu.regs.rdx]
}

fn assert_direct_native_cpuid_match(leaf: u32, subleaf: u32) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // CPUID; JMP next; HLT. HLT is an explicit interpreter frontier, leaving
    // CPUID and the branch in the executable native entry block.
    memory
        .write_slice(&[0x0F, 0xA2, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();

    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = 0xFFFF_FFFF_0000_0000 | u64::from(leaf);
        vcpu.regs.rcx = 0xEEEE_EEEE_0000_0000 | u64::from(subleaf);
        vcpu.regs.rbx = u64::MAX;
        vcpu.regs.rdx = u64::MAX;
    }
    let preserved = [
        native.regs.rflags,
        native.regs.rsp,
        native.regs.rbp,
        native.regs.r8,
        native.regs.r15,
        native.regs.r16,
        native.regs.r31,
    ];

    assert!(direct.step().expect("execute direct CPUID").is_none());
    assert_eq!(direct.regs.rip, 2);

    let region = native
        .jit_compile_region()
        .expect("compile CPUID region")
        .expect("CPUID must be native eligible");
    assert!(!region.uses_vector);
    assert!(!region.uses_mmx);
    native.jit_run_region_native(&region);

    assert_eq!(
        outputs(&native),
        outputs(&direct),
        "leaf={leaf:#x} subleaf={subleaf:#x}"
    );
    assert_eq!(native.regs.rip, 4, "native handoff must target HLT");
    assert_eq!(
        [
            native.regs.rflags,
            native.regs.rsp,
            native.regs.rbp,
            native.regs.r8,
            native.regs.r15,
            native.regs.r16,
            native.regs.r31,
        ],
        preserved,
        "CPUID helper boundary corrupted non-output state"
    );
}

#[test]
fn jit_cpuid_matches_direct_profile_for_static_dynamic_and_extended_leaves() {
    for (leaf, subleaf) in [
        (0, 0xFFFF_FFFF),
        (1, 0),
        (2, 0),
        (7, 0),
        (7, 1),
        (7, 2),
        (0xD, 0),
        (0xD, 1),
        (0xD, 2),
        (0xD, 5),
        (0xD, 6),
        (0xD, 7),
        (0xD, 19),
        (0xD, 20),
        (0x15, 0),
        (0x16, 0),
        (0x29, 0),
        (0x29, 1),
        (0x8000_0000, 0),
        (0x8000_0001, 0),
        (0x8000_0002, 0),
        (0x8000_0003, 0),
        (0x8000_0004, 0),
        (0x8000_0007, 0),
        (0x8000_0008, 0),
        (0xFFFF_FFFF, 0xFFFF_FFFF),
    ] {
        assert_direct_native_cpuid_match(leaf, subleaf);
    }
}

#[test]
fn jit_serialize_matches_direct_and_preserves_state_at_handoff() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // SERIALIZE; JMP HLT; HLT. The explicit branch gives the native region a
    // deterministic handoff frontier after the serializing instruction.
    memory
        .write_slice(&[0x0F, 0x01, 0xE8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();

    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
        vcpu.regs.rbx = 0x1122_3344_5566_7788;
        vcpu.regs.rcx = 0x8877_6655_4433_2211;
        vcpu.regs.rdx = 0xFEDC_BA98_7654_3210;
    }

    assert!(direct.step().expect("direct SERIALIZE").is_none());
    assert!(direct.step().expect("direct handoff branch").is_none());
    assert_eq!(direct.regs.rip, 5);

    let region = native
        .jit_compile_region()
        .expect("compile SERIALIZE region")
        .expect("SERIALIZE must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(outputs(&native), outputs(&direct));
    assert_eq!(native.regs.r8, direct.regs.r8);
    assert_eq!(native.regs.r15, direct.regs.r15);
    assert_eq!(native.regs.r16, direct.regs.r16);
    assert_eq!(native.regs.r31, direct.regs.r31);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rbp, direct.regs.rbp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, direct.regs.rip);
}

#[test]
fn jit_verify_accepts_deterministic_serialize_regions() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x01, 0xE8, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 0xA5A5_5A5A_F0F0_0F0F;
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified SERIALIZE region")
        .expect("verified SERIALIZE region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rax, 0xA5A5_5A5A_F0F0_0F0F);
    assert_eq!(vcpu.regs.rip, 5);
}
