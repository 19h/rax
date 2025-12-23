//! Comprehensive tests for DAA and DAS BCD adjustment instructions

use crate::common::*;

// ============================================================================
// DAA - Decimal Adjust AL after Addition
// ============================================================================

#[test]
fn test_daa_no_adjustment() {
    // DAA when no adjustment needed
    let code = &[
        0xB0, 0x25,       // MOV AL, 0x25
        0x04, 0x13,       // ADD AL, 0x13 (AL = 0x38)
        0x27,             // DAA (no adjustment)
        0xF4,             // HLT
    ];
    let mut cpu = create_test_cpu_compat(code);

    run_test(&mut cpu);

    assert_eq!(cpu.get_rax() & 0xFF, 0x38, "DAA: 0x25 + 0x13 = 0x38 (no adjustment)");
}

#[test]
fn test_daa_low_nibble_adjustment() {
    // DAA when low nibble > 9
    let code = &[
        0xB0, 0x29,       // MOV AL, 0x29
        0x04, 0x08,       // ADD AL, 0x08 (AL = 0x31, but BCD should be 0x37)
        0x27,             // DAA
        0xF4,             // HLT
    ];
    let mut cpu = create_test_cpu_compat(code);

    run_test(&mut cpu);

    assert_eq!(cpu.get_rax() & 0xFF, 0x37, "DAA: 0x29 + 0x08 = 0x37 (low nibble adjusted)");
}

// ============================================================================
// DAS - Decimal Adjust AL after Subtraction
// ============================================================================

#[test]
fn test_das_no_adjustment() {
    // DAS when no adjustment needed
    let code = &[
        0xB0, 0x35,       // MOV AL, 0x35
        0x2C, 0x12,       // SUB AL, 0x12 (AL = 0x23)
        0x2F,             // DAS
        0xF4,             // HLT
    ];
    let mut cpu = create_test_cpu_compat(code);

    run_test(&mut cpu);

    assert_eq!(cpu.get_rax() & 0xFF, 0x23, "DAS: 0x35 - 0x12 = 0x23 (no adjustment)");
}

