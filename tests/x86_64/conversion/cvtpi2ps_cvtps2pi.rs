use rax::cpu::Registers;

#[path = "../common/mod.rs"]
mod common;
use common::{run_until_hlt, setup_vm};

// CVTPI2PS — Convert Packed Dword Integers to Packed Single Precision Floating-Point Values
// CVTPS2PI — Convert Packed Single Precision Floating-Point Values to Packed Dword Integers
//
// Note: These instructions operate on MMX and XMM registers which are not currently exposed
// in the Registers struct. These placeholder tests document the expected behavior.

#[test]
fn test_cvtpi2ps_opcode() {
    // CVTPI2PS XMM0, MM0 - Opcode: NP 0F 2A /r
    // Converts two signed dword integers from MMX to two single precision FP values in XMM
    // Result is stored in low quadword of destination, high quadword unchanged
    // Uses rounding control from MXCSR
    let code = [0x0f, 0x2a, 0xc0, 0xf4]; // CVTPI2PS XMM0, MM0 + HLT
    // When implemented, this should:
    // 1. Read MM0 (low 32 bits and bits 32-63 as signed dwords)
    // 2. Convert each to single precision FP
    // 3. Store in low quadword of XMM0
    // 4. Leave high quadword of XMM0 unchanged

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Test would verify correct FP values if registers were exposed
}

#[test]
fn test_cvtps2pi_opcode() {
    // CVTPS2PI MM0, XMM0 - Opcode: NP 0F 2D /r
    // Converts two single precision FP values from XMM to two signed dword integers in MMX
    // Uses low quadword of XMM source
    // When conversion is inexact, rounds per MXCSR
    // If result exceeds i32 range, returns indefinite value (0x80000000)
    let code = [0x0f, 0x2d, 0xc0, 0xf4]; // CVTPS2PI MM0, XMM0 + HLT
    // When implemented, this should:
    // 1. Read low 64 bits of XMM0 as two single precision FP values
    // 2. Convert each to signed dword integer
    // 3. Store in MM0
    // 4. Handle overflow to 0x80000000

    let mut regs = Registers::default();
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Test would verify correct integer values if registers were exposed
}

#[test]
fn test_cvtpi2ps_documentation() {
    // CVTPI2PS documentation tests - verify instruction encoding
    // Format: CVTPI2PS r128, r64/m64
    // Operand 1: XMM register (destination)
    // Operand 2: MMX register or 64-bit memory
    // Prefix: NP (no mandatory prefix)
    // Opcode: 0F 2A
    // ModRM: /r

    // Tests would cover:
    // - Zero input
    // - Positive integer values
    // - Negative integer values
    // - Maximum positive int32 (2147483647)
    // - Minimum negative int32 (-2147483648)
    // - Rounding modes (default round to nearest)
    // - Precision (FP values might be inexact for large integers)

    // Example test case (when implemented):
    // Input: MM0 = [1, 2] as dwords
    // Expected output: XMM0[63:0] = [1.0f, 2.0f] in single precision
    // 1.0f = 0x3F800000, 2.0f = 0x40000000
    // XMM0[63:0] should be: 0x40000000_3F800000
}

#[test]
fn test_cvtps2pi_documentation() {
    // CVTPS2PI documentation tests - verify instruction encoding
    // Format: CVTPS2PI r64, r128/m64
    // Operand 1: MMX register (destination)
    // Operand 2: XMM register or 128-bit memory
    // When XMM register, only low 64 bits (2 single precision values) are used
    // Prefix: NP (no mandatory prefix)
    // Opcode: 0F 2D
    // ModRM: /r

    // Tests would cover:
    // - Zero input (FP)
    // - Positive FP values
    // - Negative FP values
    // - Fractional values (rounding per MXCSR)
    // - Overflow handling (returns 0x80000000)
    // - Using only low quadword of XMM (high quadword ignored)

    // Example test case (when implemented):
    // Input: XMM0[63:0] = [5.0f, 10.0f]
    // Expected output: MM0 = [5, 10] as signed dwords
    // MM0 should be: 0x000A_0005
}

// Tests for extended registers (R8-R15) would be added when supported
#[test]
fn test_cvtpi2ps_r8_xmm_placeholder() {
    // CVTPI2PS XMM8, MM0 with REX.R prefix
    // Would test conversion with extended XMM register
    // Opcode with REX: 44 0F 2A
}

#[test]
fn test_cvtps2pi_r8_xmm_placeholder() {
    // CVTPS2PI MM7, XMM8 with REX.R prefix
    // Would test conversion with extended XMM register
    // Opcode with REX: 49 0F 2D
}

// Special case tests for when registers are available
#[test]
fn test_cvtpi2ps_high_quadword_unchanged() {
    // Verify that CVTPI2PS doesn't modify high quadword of destination
    // Input: XMM0 = 0xDEADBEEF_CAFEBABE (garbage in high quadword)
    // After CVTPI2PS: High quadword should still be 0xDEADBEEF
    // Only low quadword is modified with converted values
}

#[test]
fn test_cvtps2pi_low_quadword_only() {
    // Verify that CVTPS2PI uses only low quadword of XMM source
    // Input: XMM0 = 0xCAFEBABE_DEADBEEF
    // Should convert 0xDEADBEEF (low 64 bits) only
    // High 64 bits (0xCAFEBABE) are ignored
}

// Rounding mode tests
#[test]
fn test_cvtpi2ps_rounding_default() {
    // Test default MXCSR rounding mode (round to nearest)
    // CVTPI2PS should use MXCSR bits 13:12 for rounding
    // Default is round to nearest even
}

#[test]
fn test_cvtps2pi_rounding_truncate() {
    // Test with rounding mode set to truncation (round toward zero)
    // When MXCSR bits 13:12 = 11b, CVTPS2PI truncates
}

// Overflow handling tests
#[test]
fn test_cvtps2pi_overflow_positive() {
    // Test conversion of FP value exceeding max i32 (> 2147483647)
    // Result should be 0x80000000 (indefinite integer value)
    // Floating-point invalid exception raised (if not masked)
}

#[test]
fn test_cvtps2pi_overflow_negative() {
    // Test conversion of FP value below min i32 (< -2147483648)
    // Result should be 0x80000000 (indefinite integer value)
}

// State transition tests
#[test]
fn test_cvtpi2ps_x87_transition() {
    // CVTPI2PS causes transition from x87 FPU to MMX
    // x87 FPU top-of-stack pointer set to 0
    // x87 FPU tag word set to all 0s (valid)
    // If x87 exception pending, it's handled first
}

#[test]
fn test_cvtps2pi_x87_transition() {
    // CVTPS2PI causes transition from x87 FPU to MMX
    // Same state transitions as CVTPI2PS
}

#[test]
fn test_cvtpi2ps_simd_floating_point_precision() {
    // Test SIMD Floating-Point Exceptions: Precision
    // When conversion is inexact, precision exception raised
}

#[test]
fn test_cvtps2pi_simd_floating_point_invalid_precision() {
    // Test SIMD Floating-Point Exceptions: Invalid, Precision
    // Invalid raised on overflow
    // Precision raised on inexact conversion
}
