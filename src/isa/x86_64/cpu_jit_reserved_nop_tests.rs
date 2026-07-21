//! Direct/native differentials for REX2 reserved-NOP APX guards.

use super::*;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const HLT_PC: u64 = 15;
const CODE: &[u8] = &[
    0xD5, 0xFF, 0x19, 0x84, 0x7F, 0x78, 0x56, 0x34, 0x12, // REX2 0F 19 + SIB/disp32
    0x48, 0x83, 0xC3, 0x01, // add rbx,1
    0xEB, 0x00, // jmp hlt
    0xF4,
];

const NO_EFFECT_HLT_PC: u64 = 51;
const NO_EFFECT_CODE: &[u8] = &[
    0xD5, 0x00, 0x90, // REX2 NOP
    0xF3, 0xD5, 0x00, 0x90, // REX2 PAUSE
    0xD5, 0x00, 0x9B, // REX2 FWAIT
    0xD5, 0x80, 0x08, // REX2 INVD
    0xD5, 0x80, 0x09, // REX2 WBINVD
    0xD5, 0x80, 0x1C, 0xC0, // REX2 CLDEMOTE register hint
    0xD5, 0x00, 0xC6, 0xF8, 0x42, // REX2 XABORT outside RTM
    0xD5, 0x80, 0x01, 0xC1, // REX2 VMCALL hint
    0xD5, 0x00, 0xD9, 0xD0, // REX2 FNOP
    0xD5, 0x00, 0xDB, 0xE0, // REX2 FENI8087_NOP
    0xD5, 0x00, 0xDB, 0xE1, // REX2 FDISI8087_NOP
    0xD5, 0x00, 0xDB, 0xE4, // REX2 FSETPM287_NOP
    0x48, 0x83, 0xC3, 0x01, // add rbx,1
    0xEB, 0x00, // jmp hlt
    0xF4,
];

fn memory_with_bytes(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn memory_with_code() -> Arc<GuestMemoryMmap> {
    memory_with_bytes(CODE)
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rax = 0x0000_8000_0000_0000;
    vcpu.regs.rbx = 0x0123_4567_89AB_CDEF;
    vcpu.regs.r15 = 0xF0E1_D2C3_B4A5_9687;
    vcpu.regs.r16 = 0x1122_3344_5566_7788;
    vcpu.regs.r31 = 0x8877_6655_4433_2211;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to_hlt(vcpu: &mut X86_64Vcpu) {
    for _ in 0..8 {
        if vcpu.regs.rip == HLT_PC {
            return;
        }
        assert!(vcpu.step().expect("direct reserved-NOP sequence").is_none());
    }
    panic!("direct execution did not reach HLT at {HLT_PC:#x}");
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

#[test]
fn jit_rex2_reserved_nop_matches_direct_and_continues_in_the_region() {
    let memory = memory_with_code();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    direct.set_apx_enabled(true);
    native.set_apx_enabled(true);

    run_direct_to_hlt(&mut direct);
    let region = native
        .jit_compile_region()
        .expect("compile REX2 reserved-NOP region")
        .expect("dynamically guarded reserved NOP must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(register_image(&native), register_image(&direct));
    assert_eq!(native.regs.rip, HLT_PC);
    assert_eq!(native.regs.rbx, 0x0123_4567_89AB_CDF0);
}

#[test]
fn jit_rex2_reserved_nop_rechecks_apx_and_deoptimizes_without_commit() {
    let mut vcpu = test_vcpu(memory_with_code());
    vcpu.set_apx_enabled(true);
    let region = vcpu
        .jit_compile_region()
        .expect("compile REX2 reserved-NOP region")
        .expect("dynamic APX guard must not block admission");

    vcpu.set_apx_enabled(false);
    let before = register_image(&vcpu);
    vcpu.jit_run_region_native(&region);
    assert_eq!(register_image(&vcpu), before);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rbx, 0x0123_4567_89AB_CDEF);

    let error = format!(
        "{:#}",
        vcpu.step()
            .expect_err("direct replay with APX disabled must inject #UD")
    );
    assert!(error.contains("IDT entry 6 not present"), "{error}");
}

#[test]
fn jit_verify_accepts_rex2_reserved_nop_with_apx_enabled() {
    let mut vcpu = test_vcpu(memory_with_code());
    vcpu.set_apx_enabled(true);
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified REX2 reserved-NOP region")
        .expect("verified reserved NOP must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, HLT_PC);
    assert_eq!(vcpu.regs.rbx, 0x0123_4567_89AB_CDF0);
}

#[test]
fn jit_rex2_no_effect_families_match_direct_and_continue_in_one_region() {
    let memory = memory_with_bytes(NO_EFFECT_CODE);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    direct.set_apx_enabled(true);
    native.set_apx_enabled(true);

    for _ in 0..20 {
        if direct.regs.rip == NO_EFFECT_HLT_PC {
            break;
        }
        assert!(direct.step().expect("direct no-effect sequence").is_none());
    }
    assert_eq!(direct.regs.rip, NO_EFFECT_HLT_PC);

    let region = native
        .jit_compile_region()
        .expect("compile every REX2 no-effect family")
        .expect("all dynamically guarded no-effect families must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(register_image(&native), register_image(&direct));
    assert_eq!(native.regs.rip, NO_EFFECT_HLT_PC);
    assert_eq!(native.regs.rbx, 0x0123_4567_89AB_CDF0);
}

#[test]
fn jit_rex2_no_effect_families_recheck_apx_before_first_commit() {
    let mut vcpu = test_vcpu(memory_with_bytes(NO_EFFECT_CODE));
    vcpu.set_apx_enabled(true);
    let region = vcpu
        .jit_compile_region()
        .expect("compile every REX2 no-effect family")
        .expect("dynamic APX guards must preserve native admission");

    vcpu.set_apx_enabled(false);
    let before = register_image(&vcpu);
    vcpu.jit_run_region_native(&region);

    assert_eq!(register_image(&vcpu), before);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rbx, 0x0123_4567_89AB_CDEF);
}
