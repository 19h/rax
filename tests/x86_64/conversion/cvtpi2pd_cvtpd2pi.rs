use rax::cpu::Registers;

#[path = "../common/mod.rs"]
mod common;
use common::{run_until_hlt, setup_vm};

// CVTPI2PD — Convert Packed Dword Integers to Packed Double Precision Floating-Point Values
// CVTPD2PI — Convert Packed Double Precision Floating-Point Values to Packed Dword Integers
//
// Note: These instructions operate on MMX and XMM registers which are not currently exposed
// in the Registers struct. These placeholder tests document the expected behavior.

#[test]
fn test_cvtpi2pd_opcode() {
    // CVTPI2PD XMM0, MM0 - Opcode: 66 0F 2A /r
    // Converts two signed dword integers from MMX to two double precision FP values in XMM
    // Result fills the entire XMM register (two 64-bit double values)
    // Prefix: 66 (operand size override for double precision)
    let code = [0x66, 0x0f, 0x2a, 0xc0, 0xf4]; // CVTPI2PD XMM0, MM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Test would verify correct double precision FP values if registers were exposed
}

#[test]
fn test_cvtpd2pi_opcode() {
    // CVTPD2PI MM0, XMM0 - Opcode: 66 0F 2D /r
    // Converts two double precision FP values from XMM to two signed dword integers in MMX
    // Uses both 64-bit doubles (full XMM register)
    // Prefix: 66 (operand size override for double precision)
    let code = [0x66, 0x0f, 0x2d, 0xc0, 0xf4]; // CVTPD2PI MM0, XMM0 + HLT

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Test would verify correct integer values if registers were exposed
}

#[test]
fn test_cvtpi2pd_documentation() {
    // CVTPI2PD documentation tests - verify instruction encoding
    // Format: CVTPI2PD r128, r64/m64
    // Operand 1: XMM register (destination)
    // Operand 2: MMX register or 64-bit memory
    // Prefix: 66 0F 2A /r

    // When from operand xmm, mm:
    // - Causes transition from x87 FPU to MMX
    // - x87 FPU top-of-stack pointer set to 0
    // - x87 FPU tag word set to all 0s (valid)
    // - If x87 exception pending, handled before instruction

    // When from operand xmm, m64:
    // - Does NOT cause transition to MMX
    // - Does NOT take x87 FPU exceptions

    // Tests would cover:
    // - Zero input
    // - Positive integer values
    // - Negative integer values
    // - Maximum positive int32 (2147483647)
    // - Minimum negative int32 (-2147483648)
    // - Double precision can exactly represent all 32-bit integers (no rounding)

    // Example: MM0 = [1, 10] as dwords
    // Expected: XMM0 = [1.0, 10.0] in double precision
    // 1.0 = 0x3FF0000000000000, 10.0 = 0x4024000000000000
    // XMM0 = [0x4024000000000000, 0x3FF0000000000000] (high then low)
}

#[test]
fn test_cvtpd2pi_documentation() {
    // CVTPD2PI documentation tests - verify instruction encoding
    // Format: CVTPD2PI r64, r128/m128
    // Operand 1: MMX register (destination)
    // Operand 2: XMM register or 128-bit memory
    // Prefix: 66 0F 2D /r

    // Uses both 64-bit double precision values from XMM source
    // Causes transition from x87 FPU to MMX

    // Tests would cover:
    // - Zero input (FP)
    // - Positive FP values
    // - Negative FP values
    // - Fractional values (rounding per MXCSR)
    // - Overflow handling (returns 0x80000000 if result > max i32 or < min i32)
    // - Use of both quadwords (128-bit XMM)

    // Example: XMM0 = [5.0, 10.0] in double precision
    // Expected: MM0 = [5, 10] as signed dwords
    // MM0 = 0x000A_0005
}

#[test]
fn test_cvtpi2pd_memory_operand() {
    // CVTPI2PD XMM0, [RDX] - load from memory
    // Opcode: 66 0F 2A /r with ModRM for memory addressing
    // 8-byte (64-bit) memory location
    // Can be aligned or unaligned
}

#[test]
fn test_cvtpd2pi_memory_operand() {
    // CVTPD2PI MM0, [RDX] - load from memory
    // Opcode: 66 0F 2D /r with ModRM for memory addressing
    // 16-byte (128-bit) memory location
    // Intel specifies 16-byte alignment for optimal performance
}

// Extended register tests
#[test]
fn test_cvtpi2pd_xmm8() {
    // CVTPI2PD XMM8, MM0 with REX.R prefix
    // Opcode: REX.R 66 0F 2A /r
}

#[test]
fn test_cvtpd2pi_xmm8() {
    // CVTPD2PI MM0, XMM8 with REX.R prefix
    // Opcode: REX.R 66 0F 2D /r
}

// Boundary tests
#[test]
fn test_cvtpi2pd_max_positive_i32() {
    // Input: 2147483647 (0x7FFFFFFF)
    // Expected: 2.147483647e9 in double precision
    // No rounding needed (double has exact representation)
}

#[test]
fn test_cvtpi2pd_min_negative_i32() {
    // Input: -2147483648 (0x80000000)
    // Expected: -2.147483648e9 in double precision
    // No rounding needed (double has exact representation)
}

#[test]
fn test_cvtpd2pi_overflow_positive() {
    // Input: 3.0e9 (exceeds max i32)
    // Result: 0x80000000 (indefinite value)
    // Floating-point invalid exception raised (if not masked)
}

#[test]
fn test_cvtpd2pi_overflow_negative() {
    // Input: -3.0e9 (below min i32)
    // Result: 0x80000000 (indefinite value)
}

#[test]
fn test_cvtpd2pi_rounding_truncate() {
    // Input: 3.7 (double precision)
    // With MXCSR rounding = truncate: Result = 3
    // With MXCSR rounding = round to nearest: Result = 4
}

#[test]
fn test_cvtpi2pd_no_exception() {
    // CVTPI2PD from m64 (memory) has no FP exceptions
    // Only version: xmm, m64 raises no SIMD FP exceptions
    // SIMD Floating-Point Exceptions: None
}

#[test]
fn test_cvtpd2pi_fp_exception_handling() {
    // CVTPD2PI raises SIMD floating-point exceptions
    // SIMD Floating-Point Exceptions: Invalid, Precision
    // Invalid: Result exceeds i32 range
    // Precision: Conversion is inexact (rounding needed)
}

#[test]
fn test_cvtpi2pd_precision_no_loss() {
    // Double precision can exactly represent all 32-bit signed integers
    // No rounding or precision loss should occur
    // SIMD Floating-Point Exceptions: None
}

#[test]
fn test_cvtpd2pi_all_register_combinations() {
    // Test all MMX source/destination registers (MM0-MM7)
    // Test all XMM source/destination registers (XMM0-XMM7)
    // With REX: XMM8-XMM15
    // With REX: No MMX8-MMX15 (MMX registers not extended in 64-bit mode)
}

#[test]
fn test_cvtpi2pd_various_values() {
    // Test collection of values:
    // 0, 1, -1, 10, -10, 100, -100, 1000, -1000
    // Powers of 2: 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024
    // Powers of 2 - 1: 1, 3, 7, 15, 31, 63, 127, 255, 511, 1023
    // Non-powers of 2: 3, 5, 7, 13, 17, 19, 23, 29, 31, 37
}

#[test]
fn test_cvtpd2pi_fraction_handling() {
    // Test fractional double values and their rounding:
    // 0.1, 0.5, 0.9, 1.5, 2.5, 3.5, -0.5, -1.5, -2.5, -3.5
    // All should round per MXCSR setting
}
