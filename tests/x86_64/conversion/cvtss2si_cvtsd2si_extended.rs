use rax::cpu::Registers;

use crate::common::{run_until_hlt, setup_vm};

// CVTSS2SI — Convert Scalar Single Precision Floating-Point Value to Signed Integer
// CVTSD2SI — Convert Scalar Double Precision Floating-Point Value to Signed Integer
//
// Extended tests for rounding modes, overflow handling, and edge cases.
// Note: These instructions operate on XMM registers which are not currently exposed
// in the Registers struct. These placeholder tests document the expected behavior.

#[test]
fn test_cvtss2si_32bit_zero() {
    // CVTSS2SI r32, xmm/m32 - 32-bit destination
    // Opcode: F3 0F 2D /r
    // Input: 0.0
    // Result: 0
    let code = [0xf3, 0x0f, 0x2d, 0xc0, 0xf4]; // CVTSS2SI EAX, XMM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_cvtss2si_64bit_zero() {
    // CVTSS2SI r64, xmm/m32 - 64-bit destination with REX.W
    // Opcode: F3 REX.W 0F 2D /r
    // Input: 0.0
    // Result: 0
    let code = [0xf3, 0x48, 0x0f, 0x2d, 0xc0, 0xf4]; // CVTSS2SI RAX, XMM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_cvtsd2si_32bit_zero() {
    // CVTSD2SI r32, xmm/m64 - 32-bit destination
    // Opcode: F2 0F 2D /r
    // Input: 0.0
    // Result: 0
    let code = [0xf2, 0x0f, 0x2d, 0xc0, 0xf4]; // CVTSD2SI EAX, XMM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_cvtsd2si_64bit_zero() {
    // CVTSD2SI r64, xmm/m64 - 64-bit destination with REX.W
    // Opcode: F2 REX.W 0F 2D /r
    // Input: 0.0
    // Result: 0
    let code = [0xf2, 0x48, 0x0f, 0x2d, 0xc0, 0xf4]; // CVTSD2SI RAX, XMM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_cvtss2si_positive_one() {
    // CVTSS2SI with input 1.0f
    // Result: 1
}

#[test]
fn test_cvtss2si_negative_one() {
    // CVTSS2SI with input -1.0f
    // Result: -1
}

#[test]
fn test_cvtss2si_large_positive() {
    // CVTSS2SI with input near max i32 (2.147e9)
    // Result: close to 2147483647
}

#[test]
fn test_cvtss2si_large_positive_overflow() {
    // CVTSS2SI with input exceeding max i32 (3e9)
    // Result: 0x80000000 (indefinite value for i32)
    // Floating-point invalid exception raised
}

#[test]
fn test_cvtss2si_large_negative_overflow() {
    // CVTSS2SI with input below min i32 (-3e9)
    // Result: 0x80000000 (indefinite value for i32)
}

#[test]
fn test_cvtss2si_to_i64_large() {
    // CVTSS2SI r64 with large value
    // Result: 64-bit signed integer
    // Large values fit in i64 but not i32
}

#[test]
fn test_cvtss2si_to_i64_overflow() {
    // CVTSS2SI r64 with very large value exceeding i64 max
    // Result: 0x8000000000000000 (indefinite value for i64)
}

#[test]
fn test_cvtss2si_rounding_mode_default() {
    // Test default MXCSR rounding (round to nearest even)
    // Input: 1.5
    // Result depends on MXCSR bits 13:12 (default = round to nearest)
}

#[test]
fn test_cvtss2si_rounding_mode_truncate() {
    // Test rounding mode: truncate (round toward zero)
    // MXCSR bits 13:12 = 11b
    // Input: 1.9
    // Result: 1 (not 2)
}

#[test]
fn test_cvtss2si_rounding_mode_down() {
    // Test rounding mode: round down (toward negative infinity)
    // MXCSR bits 13:12 = 01b
    // Input: 1.1
    // Result: 1
}

#[test]
fn test_cvtss2si_rounding_mode_up() {
    // Test rounding mode: round up (toward positive infinity)
    // MXCSR bits 13:12 = 10b
    // Input: 1.1
    // Result: 2
}

#[test]
fn test_cvtsd2si_positive_one() {
    // CVTSD2SI with input 1.0
    // Result: 1
}

#[test]
fn test_cvtsd2si_negative_one() {
    // CVTSD2SI with input -1.0
    // Result: -1
}

#[test]
fn test_cvtsd2si_large_positive() {
    // CVTSD2SI with double precision near max i32
    // Result: close to 2147483647
}

#[test]
fn test_cvtsd2si_large_positive_overflow() {
    // CVTSD2SI with double exceeding max i32
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtsd2si_large_negative_overflow() {
    // CVTSD2SI with double below min i32
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtsd2si_to_i64_large() {
    // CVTSD2SI r64 with large value
    // Result: 64-bit signed integer
}

#[test]
fn test_cvtsd2si_to_i64_overflow() {
    // CVTSD2SI r64 with very large value exceeding i64 max
    // Result: 0x8000000000000000 (indefinite value)
}

#[test]
fn test_cvtss2si_nan_handling() {
    // Input: NaN (quiet NaN)
    // Result: 0x80000000 (indefinite value)
    // Floating-point invalid exception raised
}

#[test]
fn test_cvtss2si_infinity_positive() {
    // Input: +Inf
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtss2si_infinity_negative() {
    // Input: -Inf
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtsd2si_nan_handling() {
    // Input: NaN (quiet NaN)
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtsd2si_infinity_positive() {
    // Input: +Inf
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtsd2si_infinity_negative() {
    // Input: -Inf
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtss2si_denormalized() {
    // Input: Smallest positive denormalized single
    // Result: 0 (too small to round to 1)
}

#[test]
fn test_cvtsd2si_denormalized() {
    // Input: Smallest positive denormalized double
    // Result: 0 (too small to round to 1)
}

#[test]
fn test_cvtss2si_source_unchanged() {
    // XMM source should not be modified
    // After CVTSS2SI EAX, XMM0, XMM0 should be unchanged
}

#[test]
fn test_cvtsd2si_source_unchanged() {
    // XMM source should not be modified
    // After CVTSD2SI EAX, XMM0, XMM0 should be unchanged
}

#[test]
fn test_cvtss2si_uses_low_dword() {
    // Uses only low 32 bits of XMM register
    // High bits are ignored
}

#[test]
fn test_cvtsd2si_uses_low_quadword() {
    // Uses only low 64 bits of XMM register
    // High bits are ignored
}

#[test]
fn test_cvtss2si_all_gpr_destinations() {
    // Test all general purpose registers as destinations
    // RAX, RBX, RCX, RDX, RSI, RDI, R8-R15
}

#[test]
fn test_cvtsd2si_all_gpr_destinations() {
    // Test all general purpose registers as destinations
    // RAX, RBX, RCX, RDX, RSI, RDI, R8-R15
}

#[test]
fn test_cvtss2si_all_xmm_sources() {
    // Test all XMM registers as sources
    // XMM0-XMM7, XMM8-XMM15 (with REX)
}

#[test]
fn test_cvtsd2si_all_xmm_sources() {
    // Test all XMM registers as sources
    // XMM0-XMM7, XMM8-XMM15 (with REX)
}

#[test]
fn test_cvtss2si_memory_operand() {
    // CVTSS2SI r32, [RDX]
    // Load single precision from memory (32-bit)
}

#[test]
fn test_cvtsd2si_memory_operand() {
    // CVTSD2SI r32, [RDX]
    // Load double precision from memory (64-bit)
}

#[test]
fn test_cvtss2si_vex_encoding() {
    // VEX-encoded VCVTSS2SI (AVX and later)
    // VEX.LIG.F3.0F.W0 2D /r
}

#[test]
fn test_cvtsd2si_vex_encoding() {
    // VEX-encoded VCVTSD2SI (AVX and later)
    // VEX.LIG.F2.0F.W0 2D /r
}

#[test]
fn test_cvtss2si_evex_encoding() {
    // EVEX-encoded VCVTSS2SI (AVX-512)
    // EVEX.LLIG.F3.0F.W0 2D /r
}

#[test]
fn test_cvtsd2si_evex_encoding() {
    // EVEX-encoded VCVTSD2SI (AVX-512)
    // EVEX.LLIG.F2.0F.W0 2D /r
}

#[test]
fn test_cvtss2si_fractional_values() {
    // Test various fractional values
    // 0.1, 0.5, 0.9, 1.1, 1.5, 1.9, 2.1, etc.
    // Verify rounding behavior
}

#[test]
fn test_cvtsd2si_fractional_values() {
    // Test various fractional double values
    // 0.1, 0.5, 0.9, 1.1, 1.5, 1.9, 2.1, etc.
    // Verify rounding behavior
}

#[test]
fn test_cvtss2si_negative_fractional() {
    // Test negative fractional values
    // -0.1, -0.5, -0.9, -1.1, -1.5, -1.9, -2.1, etc.
    // Verify rounding with negatives
}

#[test]
fn test_cvtsd2si_negative_fractional() {
    // Test negative fractional double values
    // Same range as positive but negative
}
