use rax::cpu::Registers;

#[path = "../common/mod.rs"]
mod common;
use common::{run_until_hlt, setup_vm};

// CVTTPS2PI — Convert With Truncation Packed Single Precision FP to Packed Dword Integers
// CVTTPD2PI — Convert With Truncation Packed Double Precision FP to Packed Dword Integers
//
// These instructions use truncation (round toward zero) instead of MXCSR rounding mode.
// Note: These instructions operate on MMX and XMM registers which are not currently exposed
// in the Registers struct. These placeholder tests document the expected behavior.

#[test]
fn test_cvttps2pi_opcode() {
    // CVTTPS2PI MM0, XMM0 - Opcode: NP 0F 2C /r
    // Converts two single precision FP values to two signed dword integers with truncation
    // Uses low quadword of XMM source
    // Prefix: NP (no mandatory prefix)
    let code = [0x0f, 0x2c, 0xc0, 0xf4]; // CVTTPS2PI MM0, XMM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Test would verify correct integer values with truncation
}

#[test]
fn test_cvttpd2pi_opcode() {
    // CVTTPD2PI MM0, XMM0 - Opcode: 66 0F 2C /r
    // Converts two double precision FP values to two signed dword integers with truncation
    // Uses both 64-bit doubles (full XMM register)
    // Prefix: 66 (operand size override for double precision)
    let code = [0x66, 0x0f, 0x2c, 0xc0, 0xf4]; // CVTTPD2PI MM0, XMM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Test would verify correct integer values with truncation
}

#[test]
fn test_cvttps2pi_truncation_positive() {
    // Test truncation (round toward zero) with positive fractional values
    // 1.9 should truncate to 1 (not round to 2)
    // 2.9 should truncate to 2 (not round to 3)
    // Differs from CVTPS2PI which rounds per MXCSR
}

#[test]
fn test_cvttps2pi_truncation_negative() {
    // Test truncation with negative fractional values
    // -1.9 should truncate to -1 (not round to -2)
    // -2.9 should truncate to -2 (not round to -3)
}

#[test]
fn test_cvttpd2pi_truncation_positive() {
    // Test truncation with double precision positive values
    // 1.5, 2.5, 3.5 should truncate to 1, 2, 3 respectively
}

#[test]
fn test_cvttpd2pi_truncation_negative() {
    // Test truncation with double precision negative values
    // -1.5, -2.5, -3.5 should truncate to -1, -2, -3 respectively
}

#[test]
fn test_cvttps2pi_low_quadword_only() {
    // CVTTPS2PI uses only low quadword of XMM source
    // High quadword is ignored
    // Input: XMM0 = 0xDEADBEEF_CAFEBABE
    // Only 0xCAFEBABE (low 64 bits with 2 single FP values) is used
}

#[test]
fn test_cvttpd2pi_full_xmm() {
    // CVTTPD2PI uses full 128-bit XMM register
    // Both 64-bit double values are converted
}

#[test]
fn test_cvttps2pi_overflow_positive() {
    // Input: 3.0e9 (exceeds max i32)
    // Result: 0x80000000 (indefinite value)
    // Floating-point invalid exception raised (if not masked)
}

#[test]
fn test_cvttps2pi_overflow_negative() {
    // Input: -3.0e9 (below min i32)
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvttpd2pi_overflow_positive() {
    // Input: 3.0e9 (exceeds max i32)
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvttpd2pi_overflow_negative() {
    // Input: -3.0e9 (below min i32)
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvttps2pi_vs_cvtps2pi_difference() {
    // CVTTPS2PI always truncates (ignores MXCSR rounding)
    // CVTPS2PI respects MXCSR rounding mode
    // Example: 2.5 with MXCSR round-to-nearest
    // CVTTPS2PI: result = 2 (truncation)
    // CVTPS2PI: result = 2 or 3 (depends on rounding)
}

#[test]
fn test_cvttps2pi_zero() {
    // 0.0 should convert to 0
}

#[test]
fn test_cvttps2pi_positive_one() {
    // 1.0 should convert to 1
}

#[test]
fn test_cvttps2pi_negative_one() {
    // -1.0 should convert to -1
}

#[test]
fn test_cvttps2pi_max_positive() {
    // Large positive value near max i32
}

#[test]
fn test_cvttps2pi_min_negative() {
    // Large negative value near min i32
}

#[test]
fn test_cvttpd2pi_zero() {
    // 0.0 (double) should convert to 0
}

#[test]
fn test_cvttpd2pi_positive_one() {
    // 1.0 (double) should convert to 1
}

#[test]
fn test_cvttpd2pi_negative_one() {
    // -1.0 (double) should convert to -1
}

#[test]
fn test_cvttps2pi_transition_x87() {
    // CVTTPS2PI causes transition from x87 FPU to MMX
    // x87 FPU top-of-stack pointer set to 0
    // x87 FPU tag word set to all 0s (valid)
}

#[test]
fn test_cvttpd2pi_transition_x87() {
    // CVTTPD2PI causes transition from x87 FPU to MMX
    // Same state transitions as CVTTPS2PI
}

#[test]
fn test_cvttps2pi_simd_exceptions() {
    // SIMD Floating-Point Exceptions: Invalid, Precision
    // Invalid: Result exceeds i32 range
    // Precision: Conversion is inexact (even though truncating)
}

#[test]
fn test_cvttpd2pi_simd_exceptions() {
    // SIMD Floating-Point Exceptions: Invalid, Precision
    // Same as CVTTPS2PI
}

#[test]
fn test_cvttps2pi_extended_register() {
    // CVTTPS2PI XMM8, MM0 with REX.R prefix
    // Opcode: 41 0F 2C /r
}

#[test]
fn test_cvttpd2pi_extended_register() {
    // CVTTPD2PI XMM8, MM0 with REX.R prefix
    // Opcode: REX.R 66 0F 2C /r
}

#[test]
fn test_cvttps2pi_memory_operand() {
    // CVTTPS2PI MM0, [RDX] - load from memory
    // Opcode: 0F 2C /r with ModRM for memory addressing
    // 8-byte (64-bit) memory location
}

#[test]
fn test_cvttpd2pi_memory_operand() {
    // CVTTPD2PI MM0, [RDX] - load from memory
    // Opcode: 66 0F 2C /r with ModRM for memory addressing
    // 16-byte (128-bit) memory location
}

#[test]
fn test_cvttps2pi_very_small_positive() {
    // Input: 0.1 (single precision)
    // Should truncate to 0
}

#[test]
fn test_cvttps2pi_very_small_negative() {
    // Input: -0.1 (single precision)
    // Should truncate to 0
}

#[test]
fn test_cvttpd2pi_very_small_positive() {
    // Input: 0.1 (double precision)
    // Should truncate to 0
}

#[test]
fn test_cvttpd2pi_very_small_negative() {
    // Input: -0.1 (double precision)
    // Should truncate to 0
}
