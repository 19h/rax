//! CPU-level native-JIT coverage for helper-backed VEX binary memory sources.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DATA_BASE: u64 = 0x3000;
const DISP: u64 = 0x20;

fn long_mode_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rflags = 0x8D7;
    vcpu.mxcsr = 0xDFC5;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn seed_architectural_state(vcpu: &mut X86_64Vcpu) {
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rcx = 0x1111_2222_3333_4444;
    vcpu.regs.rdx = 0x5555_6666_7777_8888;
    vcpu.regs.rbx = DATA_BASE;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rbp = 0xA000;
    vcpu.regs.rsi = 0x9999_AAAA_BBBB_CCCC;
    vcpu.regs.rdi = 0xDDDD_EEEE_FFFF_0000;
    vcpu.regs.r8 = 0x0808_0808_0808_0808;
    vcpu.regs.r9 = 0x0909_0909_0909_0909;
    vcpu.regs.r10 = 0x1010_1010_1010_1010;
    vcpu.regs.r11 = DATA_BASE;
    vcpu.regs.r12 = 0x1212_1212_1212_1212;
    vcpu.regs.r13 = 0x1313_1313_1313_1313;
    vcpu.regs.r14 = 0x1414_1414_1414_1414;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.xmm = std::array::from_fn(|register| {
        [
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32),
            0xFEDC_BA98_7654_3210u64.rotate_right((register * 11) as u32),
        ]
    });
    vcpu.regs.ymm_high = std::array::from_fn(|register| {
        [
            0x1111_2222_3333_4444u64.rotate_left((register * 5) as u32),
            0xAAAA_BBBB_CCCC_DDDDu64.rotate_right((register * 3) as u32),
        ]
    });
    vcpu.regs.zmm_high = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 13 + word * 17) as u32)
        })
    });
    vcpu.regs.zmm_ext = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x6996_F00F_3CC3_A55Au64.rotate_right((register * 19 + word * 23) as u32)
        })
    });
    vcpu.regs.k = [
        0x6996_F00F_3CC3_A55A,
        0,
        1,
        0x0123_4567_89AB_CDEF,
        0x5555_AAAA_3333_CCCC,
        0x8000_0000_0000_0000,
        0xF0F0_0F0F_A5A5_5A5A,
        u64::MAX,
    ];
}

fn gprs(regs: &crate::vm::vcpu::Registers) -> [u64; 16] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15,
    ]
}

fn assert_architectural_state_equal(
    actual: &X86_64Vcpu,
    expected: &crate::vm::vcpu::Registers,
    expected_mxcsr: u32,
    context: &str,
) {
    assert_eq!(gprs(&actual.regs), gprs(expected), "{context}: GPRs");
    assert_eq!(actual.regs.xmm, expected.xmm, "{context}: XMM");
    assert_eq!(
        actual.regs.ymm_high, expected.ymm_high,
        "{context}: YMM high"
    );
    assert_eq!(
        actual.regs.zmm_high, expected.zmm_high,
        "{context}: ZMM high"
    );
    assert_eq!(
        actual.regs.zmm_ext, expected.zmm_ext,
        "{context}: ZMM16-ZMM31"
    );
    assert_eq!(actual.regs.k, expected.k, "{context}: opmasks");
    assert_eq!(actual.regs.rflags, expected.rflags, "{context}: RFLAGS");
    assert_eq!(actual.mxcsr, expected_mxcsr, "{context}: MXCSR");
    assert_eq!(actual.regs.rip, expected.rip, "{context}: RIP");
}

#[test]
fn jit_verify_executes_chained_c4_c5_wig_logic_memory_sources_with_avx_only_state() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping CPU JIT VEX memory-logic verification: host lacks AVX");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vandps xmm0,xmm1,[rbx+0x20]       (C5, scratch=2)
    // vorpd ymm15,ymm0,[r11+0x20]       (C4.W0, high destination, scratch=1)
    // vxorps ymm9,ymm9,[r11+0x20]       (C4.W1 ignored, alias, scratch=0)
    // jmp next; hlt
    let code = [
        0xC5, 0xF0, 0x54, 0x43, 0x20, 0xC4, 0x41, 0x7D, 0x56, 0x7B, 0x20, 0xC4, 0x41, 0xB4, 0x57,
        0x4B, 0x20, 0xEB, 0x00, 0xF4,
    ];
    memory.write_slice(&code, GuestAddress(0)).unwrap();
    let source: [u8; 32] = std::array::from_fn(|index| (index as u8).wrapping_mul(0x3D) ^ 0xA5);
    memory
        .write_slice(&source, GuestAddress(DATA_BASE + DISP))
        .unwrap();

    let mut direct = long_mode_vcpu(memory.clone());
    let mut verified = long_mode_vcpu(memory);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut verified);

    let frontier = code.len() as u64 - 1;
    let mut direct_steps = 0usize;
    while direct.regs.rip != frontier {
        assert!(direct.step().unwrap().is_none());
        direct_steps += 1;
        assert!(direct_steps <= 4, "direct execution missed HLT frontier");
    }
    assert_eq!(direct_steps, 4);

    let region = verified
        .jit_compile_region()
        .expect("compile VEX memory-logic region")
        .expect("helper-backed VEX memory logic must be native eligible");
    assert!(region.uses_vector);
    assert!(region.avx_ymm16_vector_state);
    assert!(!region.narrow_vector_opmasks);

    verified.jit_run_region_verified(&region);
    assert_architectural_state_equal(
        &verified,
        &direct.regs,
        direct.mxcsr,
        "verified chained logic",
    );
    assert_eq!(verified.regs.rip, frontier);
}

#[test]
fn jit_verify_executes_packed_integer_logic_memory_sources_with_avx2_gate() {
    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping CPU JIT VEX integer memory-logic verification: host lacks AVX2");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vpand xmm2,xmm3,[r11+0x20]       (VEX.128 requires AVX)
    // vpxor ymm14,ymm2,[r11+0x20]      (VEX.256 requires AVX2)
    // jmp next; hlt
    let code = [
        0xC4, 0xC1, 0x61, 0xDB, 0x53, 0x20, 0xC4, 0x41, 0x6D, 0xEF, 0x73, 0x20, 0xEB, 0x00, 0xF4,
    ];
    memory.write_slice(&code, GuestAddress(0)).unwrap();
    let source: [u8; 32] = std::array::from_fn(|index| (index as u8).wrapping_mul(0xA7) ^ 0x5C);
    memory
        .write_slice(&source, GuestAddress(DATA_BASE + DISP))
        .unwrap();

    let mut direct = long_mode_vcpu(memory.clone());
    let mut verified = long_mode_vcpu(memory);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut verified);

    let frontier = code.len() as u64 - 1;
    let mut direct_steps = 0usize;
    while direct.regs.rip != frontier {
        assert!(direct.step().unwrap().is_none());
        direct_steps += 1;
        assert!(direct_steps <= 3, "direct execution missed HLT frontier");
    }
    assert_eq!(direct_steps, 3);

    let region = verified
        .jit_compile_region()
        .expect("compile VEX integer memory-logic region")
        .expect("helper-backed VEX integer memory logic must be native eligible");
    assert!(region.uses_vector);
    assert!(region.avx_ymm16_vector_state);
    assert!(!region.narrow_vector_opmasks);

    verified.jit_run_region_verified(&region);
    assert_architectural_state_equal(
        &verified,
        &direct.regs,
        direct.mxcsr,
        "verified packed-integer logic",
    );
    assert_eq!(verified.regs.rip, frontier);
}

#[test]
fn jit_verify_executes_wrapping_and_saturating_integer_arithmetic_memory_sources() {
    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping CPU JIT VEX integer memory-arithmetic verification: host lacks AVX2");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vpaddsb xmm2,xmm3,[rbx+0x20]     (C5, signed saturation, scratch=0)
    // vpsubusb xmm15,xmm0,[r11+0x20]  (C4.W0, unsigned saturation, scratch=1)
    // vpaddq ymm9,ymm9,[r11+0x20]      (C4.W1 ignored, wrapping, scratch=0)
    // vpsubsw ymm14,ymm2,[r11+0x20]    (C4.W0, signed saturation, scratch=0)
    // jmp next; hlt
    let code = [
        0xC5, 0xE1, 0xEC, 0x53, 0x20, 0xC4, 0x41, 0x79, 0xD8, 0x7B, 0x20, 0xC4, 0x41, 0xB5, 0xD4,
        0x4B, 0x20, 0xC4, 0x41, 0x6D, 0xE9, 0x73, 0x20, 0xEB, 0x00, 0xF4,
    ];
    memory.write_slice(&code, GuestAddress(0)).unwrap();
    let source: [u8; 32] = std::array::from_fn(|index| match index % 8 {
        0 => 0x01,
        1 => 0x7F,
        2 => 0x80,
        3 => 0xFF,
        4 => 0x55,
        5 => 0xAA,
        6 => 0x00,
        _ => index as u8,
    });
    memory
        .write_slice(&source, GuestAddress(DATA_BASE + DISP))
        .unwrap();

    let mut direct = long_mode_vcpu(memory.clone());
    let mut verified = long_mode_vcpu(memory);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut verified);

    let frontier = code.len() as u64 - 1;
    let mut direct_steps = 0usize;
    while direct.regs.rip != frontier {
        assert!(direct.step().unwrap().is_none());
        direct_steps += 1;
        assert!(direct_steps <= 5, "direct execution missed HLT frontier");
    }
    assert_eq!(direct_steps, 5);

    let region = verified
        .jit_compile_region()
        .expect("compile VEX integer memory-arithmetic region")
        .expect("helper-backed VEX integer arithmetic must be native eligible");
    assert!(region.uses_vector);
    assert!(region.avx_ymm16_vector_state);
    assert!(!region.narrow_vector_opmasks);

    verified.jit_run_region_verified(&region);
    assert_architectural_state_equal(
        &verified,
        &direct.regs,
        direct.mxcsr,
        "verified packed-integer arithmetic",
    );
    assert_eq!(verified.regs.rip, frontier);
}

#[test]
fn jit_integer_arithmetic_memory_fault_exits_without_architectural_commit() {
    if !std::is_x86_feature_detected!("avx2") {
        eprintln!("skipping CPU JIT VEX integer memory-arithmetic fault test: host lacks AVX2");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vpsubusb ymm14,ymm2,[r11+0x20]; jmp next; hlt
    let code = [0xC4, 0x41, 0x6D, 0xD8, 0x73, 0x20, 0xEB, 0x00, 0xF4];
    memory.write_slice(&code, GuestAddress(0)).unwrap();

    let mut vcpu = long_mode_vcpu(memory);
    seed_architectural_state(&mut vcpu);
    vcpu.regs.r11 = 0x2_0000;
    let before = vcpu.regs.clone();
    let before_mxcsr = vcpu.mxcsr;

    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting VEX integer memory-arithmetic region")
        .expect("dynamic faulting address must not prevent native admission");
    assert!(region.uses_vector);
    assert!(region.avx_ymm16_vector_state);
    vcpu.jit_run_region_native(&region);

    assert_architectural_state_equal(
        &vcpu,
        &before,
        before_mxcsr,
        "integer-arithmetic fault deoptimization",
    );
    assert_eq!(vcpu.regs.rip, 0);
}

#[test]
fn jit_memory_logic_fault_exits_at_instruction_without_architectural_commit() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping CPU JIT VEX memory-logic fault test: host lacks AVX");
        return;
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vandps xmm0,xmm1,[rbx+0x20]; jmp next; hlt
    let code = [0xC5, 0xF0, 0x54, 0x43, 0x20, 0xEB, 0x00, 0xF4];
    memory.write_slice(&code, GuestAddress(0)).unwrap();

    let mut vcpu = long_mode_vcpu(memory);
    seed_architectural_state(&mut vcpu);
    vcpu.regs.rbx = 0x2_0000;
    let before = vcpu.regs.clone();
    let before_mxcsr = vcpu.mxcsr;

    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting VEX memory-logic region")
        .expect("faulting address is dynamic and must not prevent native admission");
    assert!(region.uses_vector);
    assert!(region.avx_ymm16_vector_state);
    vcpu.jit_run_region_native(&region);

    assert_architectural_state_equal(&vcpu, &before, before_mxcsr, "fault deoptimization");
    assert_eq!(vcpu.regs.rip, 0);
}
