//! Direct and native-frontier coverage for profile-disabled PTWRITE.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const PROBE_ADDRESS: u64 = 0x4000;
const PROBE_LEN: usize = 576;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.cr4 = 1 << 18; // OSXSAVE: exposes accidental PTWRITE -> XSAVE dispatch.
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    {
        vcpu.set_jit_mem(false);
        vcpu.set_jit_call(false);
    }
    vcpu
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn assert_register_state_unchanged(actual: &Registers, expected: &Registers) {
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

fn assert_step_ud(vcpu: &mut X86_64Vcpu) {
    let before = vcpu.regs.clone();
    let error = vcpu
        .step()
        .expect_err("profile-disabled PTWRITE must inject #UD");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "expected #UD delivery failure, got {error}"
    );
    assert_register_state_unchanged(&vcpu.regs, &before);
}

#[test]
fn deterministic_cpuid_profile_does_not_advertise_ptwrite() {
    let mut vcpu = test_vcpu(memory_with_code(&[0x0F, 0xA2]));
    vcpu.regs.rax = 0x14;
    vcpu.regs.rcx = 0;

    assert!(vcpu.step().expect("CPUID.14H").is_none());
    assert_eq!(vcpu.regs.rax, 0);
    assert_eq!(vcpu.regs.rbx & (1 << 4), 0, "EBX.PTWRITE must remain clear");
    assert_eq!(vcpu.regs.rcx, 0);
    assert_eq!(vcpu.regs.rdx, 0);
}

#[test]
fn direct_ptwrite_faults_before_register_or_memory_observation() {
    for (code, apx) in [
        (&[0xF3, 0x0F, 0xAE, 0xE3][..], false),
        (&[0xF3, 0x48, 0x0F, 0xAE, 0xE3], false),
        (&[0xF3, 0x41, 0x0F, 0xAE, 0xE3], false),
        (&[0xF3, 0xD5, 0x90, 0xAE, 0xE3], true),
        (&[0xF0, 0xF3, 0x0F, 0xAE, 0xE3], false),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(code));
        vcpu.set_apx_enabled(apx);
        vcpu.regs.rbx = 0xBBBB_BBBB_BBBB_BBBB;
        vcpu.regs.r11 = 0x1111_1111_1111_1111;
        vcpu.regs.r19 = 0x1919_1919_1919_1919;
        assert_step_ud(&mut vcpu);
    }

    let memory = memory_with_code(&[0xF3, 0x0F, 0xAE, 0x20]);
    let sentinel = [0xA5; PROBE_LEN];
    memory
        .write_slice(&sentinel, GuestAddress(PROBE_ADDRESS))
        .unwrap();
    let mut vcpu = test_vcpu(memory.clone());
    vcpu.regs.rax = PROBE_ADDRESS;
    assert_step_ud(&mut vcpu);

    let mut observed = [0; PROBE_LEN];
    memory
        .read_slice(&mut observed, GuestAddress(PROBE_ADDRESS))
        .unwrap();
    assert_eq!(observed, sentinel, "PTWRITE must not access its source");
}

#[test]
fn direct_ptwrite_feature_fault_precedes_address_faults() {
    for (code, address) in [
        (&[0xF3, 0x0F, 0xAE, 0x20][..], 0x20_000),
        (&[0x36, 0xF3, 0x0F, 0xAE, 0x20], 0x0000_8000_0000_0000),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(code));
        vcpu.regs.rax = address;
        assert_step_ud(&mut vcpu);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_profile_disabled_ptwrite_exits_at_the_exact_faulting_frontier() {
    for (name, instruction, apx) in [
        ("register", &[0xF3, 0x0F, 0xAE, 0xE3][..], false),
        ("memory", &[0xF3, 0x0F, 0xAE, 0x23], false),
        ("REX2 register", &[0xF3, 0xD5, 0x90, 0xAE, 0xE3], true),
    ] {
        let mut code = vec![
            0xB9, 0x78, 0x56, 0x34, 0x12, // mov ecx,12345678h
            0xEB, 0x02, // jmp PTWRITE
            0x90, 0x90, // unreachable padding
        ];
        code.extend_from_slice(instruction);
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.set_apx_enabled(apx);
        vcpu.regs.rbx = 0x20_000;
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at profile-disabled PTWRITE")
            .expect("supported prefix must remain native before PTWRITE frontier");

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rcx, 0x1234_5678, "{name}");
        assert_eq!(vcpu.regs.rip, 9, "{name}");
        assert_step_ud(&mut vcpu);
    }
}
