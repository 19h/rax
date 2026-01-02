//! A64 integer mul_div tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_integer_arithmetic_mul_widening_32_64 Tests
// ============================================================================

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field U 23 +: 1`
/// Requirement: FieldBoundary { field: "U", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_u_0_min_0_9b200000() {
    // Encoding: 0x9B200000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field U = 0 (Min)
    // Fields: Rm=0, Rn=0, o0=0, Rd=0, Ra=0, U=0
    let encoding: u32 = 0x9B200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field U 23 +: 1`
/// Requirement: FieldBoundary { field: "U", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_u_1_max_0_9ba00000() {
    // Encoding: 0x9BA00000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field U = 1 (Max)
    // Fields: o0=0, Ra=0, Rn=0, Rd=0, U=1, Rm=0
    let encoding: u32 = 0x9BA00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rm_0_min_0_9b200000() {
    // Encoding: 0x9B200000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rm = 0 (Min)
    // Fields: Rn=0, Rd=0, o0=0, U=0, Ra=0, Rm=0
    let encoding: u32 = 0x9B200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rm_1_poweroftwo_0_9b210000() {
    // Encoding: 0x9B210000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rm = 1 (PowerOfTwo)
    // Fields: Rd=0, U=0, Rm=1, Ra=0, o0=0, Rn=0
    let encoding: u32 = 0x9B210000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rm_30_poweroftwominusone_0_9b3e0000() {
    // Encoding: 0x9B3E0000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Ra=0, Rd=0, o0=0, Rn=0, U=0
    let encoding: u32 = 0x9B3E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rm_31_max_0_9b3f0000() {
    // Encoding: 0x9B3F0000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rm = 31 (Max)
    // Fields: Ra=0, Rn=0, o0=0, U=0, Rm=31, Rd=0
    let encoding: u32 = 0x9B3F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field o0 15 +: 1`
/// Requirement: FieldBoundary { field: "o0", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_o0_0_min_0_9b200000() {
    // Encoding: 0x9B200000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field o0 = 0 (Min)
    // Fields: Ra=0, o0=0, Rn=0, U=0, Rd=0, Rm=0
    let encoding: u32 = 0x9B200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field o0 15 +: 1`
/// Requirement: FieldBoundary { field: "o0", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_o0_1_max_0_9b208000() {
    // Encoding: 0x9B208000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field o0 = 1 (Max)
    // Fields: Ra=0, Rd=0, Rn=0, U=0, Rm=0, o0=1
    let encoding: u32 = 0x9B208000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_ra_0_min_0_9b200000() {
    // Encoding: 0x9B200000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Ra = 0 (Min)
    // Fields: U=0, Ra=0, Rn=0, Rd=0, Rm=0, o0=0
    let encoding: u32 = 0x9B200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_ra_1_poweroftwo_0_9b200400() {
    // Encoding: 0x9B200400
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Ra = 1 (PowerOfTwo)
    // Fields: Rd=0, Ra=1, U=0, Rn=0, o0=0, Rm=0
    let encoding: u32 = 0x9B200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_ra_30_poweroftwominusone_0_9b207800() {
    // Encoding: 0x9B207800
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Ra = 30 (PowerOfTwoMinusOne)
    // Fields: Ra=30, Rm=0, Rd=0, U=0, o0=0, Rn=0
    let encoding: u32 = 0x9B207800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_ra_31_max_0_9b207c00() {
    // Encoding: 0x9B207C00
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Ra = 31 (Max)
    // Fields: o0=0, Rm=0, Ra=31, Rd=0, Rn=0, U=0
    let encoding: u32 = 0x9B207C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rn_0_min_0_9b200000() {
    // Encoding: 0x9B200000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rn = 0 (Min)
    // Fields: Rd=0, U=0, Rm=0, Ra=0, Rn=0, o0=0
    let encoding: u32 = 0x9B200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rn_1_poweroftwo_0_9b200020() {
    // Encoding: 0x9B200020
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rn = 1 (PowerOfTwo)
    // Fields: Ra=0, Rd=0, Rm=0, o0=0, Rn=1, U=0
    let encoding: u32 = 0x9B200020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rn_30_poweroftwominusone_0_9b2003c0() {
    // Encoding: 0x9B2003C0
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, U=0, Rn=30, o0=0, Rd=0, Ra=0
    let encoding: u32 = 0x9B2003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rn_31_max_0_9b2003e0() {
    // Encoding: 0x9B2003E0
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rn = 31 (Max)
    // Fields: U=0, o0=0, Rm=0, Ra=0, Rn=31, Rd=0
    let encoding: u32 = 0x9B2003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rd_0_min_0_9b200000() {
    // Encoding: 0x9B200000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rd = 0 (Min)
    // Fields: Rn=0, U=0, Rm=0, o0=0, Rd=0, Ra=0
    let encoding: u32 = 0x9B200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rd_1_poweroftwo_0_9b200001() {
    // Encoding: 0x9B200001
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, o0=0, Ra=0, U=0, Rn=0, Rd=1
    let encoding: u32 = 0x9B200001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rd_30_poweroftwominusone_0_9b20001e() {
    // Encoding: 0x9B20001E
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: o0=0, Rn=0, U=0, Rm=0, Ra=0, Rd=30
    let encoding: u32 = 0x9B20001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_field_rd_31_max_0_9b20001f() {
    // Encoding: 0x9B20001F
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field Rd = 31 (Max)
    // Fields: U=0, Rm=0, Ra=0, Rn=0, Rd=31, o0=0
    let encoding: u32 = 0x9B20001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// U=0 (minimum value)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_combo_0_0_9b200000() {
    // Encoding: 0x9B200000
    // Test aarch64_integer_arithmetic_mul_widening_32_64 field combination: U=0, Rm=0, o0=0, Ra=0, Rn=0, Rd=0
    // Fields: U=0, o0=0, Rn=0, Rd=0, Rm=0, Ra=0
    let encoding: u32 = 0x9B200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_special_rn_31_stack_pointer_sp_may_require_alignment_0_9b2003e0(
) {
    // Encoding: 0x9B2003E0
    // Test aarch64_integer_arithmetic_mul_widening_32_64 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: o0=0, Ra=0, Rn=31, U=0, Rd=0, Rm=0
    let encoding: u32 = 0x9B2003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_9b20001f(
) {
    // Encoding: 0x9B20001F
    // Test aarch64_integer_arithmetic_mul_widening_32_64 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: U=0, o0=0, Ra=0, Rn=0, Rm=0, Rd=31
    let encoding: u32 = 0x9B20001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `SMULL X0, W1, W2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// simple multiply
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_smull_oracle_0_9b227c20() {
    // Test SMULL: simple multiply (oracle)
    // Encoding: 0x9B227C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x2);
    set_x(&mut cpu, 2, 0x3);
    let encoding: u32 = 0x9B227C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x6, "X0 should be 0x0000000000000006");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `SMULL X0, W1, W2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max 32-bit * 2
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_smull_oracle_1_9b227c20() {
    // Test SMULL: max 32-bit * 2 (oracle)
    // Encoding: 0x9B227C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x9B227C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFE,
        "X0 should be 0xFFFFFFFFFFFFFFFE"
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `SMULL X0, W1, W2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// large positive * large positive
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_smull_oracle_2_9b227c20() {
    // Test SMULL: large positive * large positive (oracle)
    // Encoding: 0x9B227C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFF);
    set_x(&mut cpu, 2, 0x7FFFFFFF);
    let encoding: u32 = 0x9B227C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x3FFFFFFF00000001,
        "X0 should be 0x3FFFFFFF00000001"
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `SMULL X0, W1, W2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max unsigned * max unsigned
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_smull_oracle_3_9b227c20() {
    // Test SMULL: max unsigned * max unsigned (oracle)
    // Encoding: 0x9B227C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFF);
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x9B227C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `SMULL X0, W1, W2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// medium values
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_smull_oracle_4_9b227c20() {
    // Test SMULL: medium values (oracle)
    // Encoding: 0x9B227C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xC8);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x9B227C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x4E20, "X0 should be 0x0000000000004E20");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `SMULL X0, W1, W2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// 16-bit values
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_smull_oracle_5_9b227c20() {
    // Test SMULL: 16-bit values (oracle)
    // Encoding: 0x9B227C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1234);
    set_x(&mut cpu, 2, 0x5678);
    let encoding: u32 = 0x9B227C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x6260060, "X0 should be 0x0000000006260060");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_reg_write_0_9b200000() {
    // Test aarch64_integer_arithmetic_mul_widening_32_64 register write: GpFromField("d")
    // Encoding: 0x9B200000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x9B200000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_sp_rn_9b2003e0() {
    // Test aarch64_integer_arithmetic_mul_widening_32_64 with Rn = SP (31)
    // Encoding: 0x9B2003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x9B2003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_32_64
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_32_64_zr_rd_9b20001f() {
    // Test aarch64_integer_arithmetic_mul_widening_32_64 with Rd = ZR (31)
    // Encoding: 0x9B20001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x9B20001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_integer_arithmetic_mul_widening_64_128hi Tests
// ============================================================================

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field U 23 +: 1`
/// Requirement: FieldBoundary { field: "U", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_u_0_min_0_9b400000() {
    // Encoding: 0x9B400000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field U = 0 (Min)
    // Fields: Ra=0, Rm=0, Rd=0, U=0, Rn=0
    let encoding: u32 = 0x9B400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field U 23 +: 1`
/// Requirement: FieldBoundary { field: "U", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_u_1_max_0_9bc00000() {
    // Encoding: 0x9BC00000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field U = 1 (Max)
    // Fields: U=1, Rm=0, Rd=0, Rn=0, Ra=0
    let encoding: u32 = 0x9BC00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rm_0_min_0_9b400000() {
    // Encoding: 0x9B400000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rm = 0 (Min)
    // Fields: Rd=0, Ra=0, U=0, Rm=0, Rn=0
    let encoding: u32 = 0x9B400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rm_1_poweroftwo_0_9b410000() {
    // Encoding: 0x9B410000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, U=0, Ra=0, Rn=0, Rd=0
    let encoding: u32 = 0x9B410000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rm_30_poweroftwominusone_0_9b5e0000()
{
    // Encoding: 0x9B5E0000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rm=30, Rd=0, Ra=0, U=0
    let encoding: u32 = 0x9B5E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rm_31_max_0_9b5f0000() {
    // Encoding: 0x9B5F0000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rm = 31 (Max)
    // Fields: Rm=31, Rd=0, Rn=0, Ra=0, U=0
    let encoding: u32 = 0x9B5F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_ra_0_min_0_9b400000() {
    // Encoding: 0x9B400000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Ra = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0, Ra=0, U=0
    let encoding: u32 = 0x9B400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_ra_1_poweroftwo_0_9b400400() {
    // Encoding: 0x9B400400
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Ra = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, Ra=1, U=0, Rd=0
    let encoding: u32 = 0x9B400400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_ra_30_poweroftwominusone_0_9b407800()
{
    // Encoding: 0x9B407800
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Ra = 30 (PowerOfTwoMinusOne)
    // Fields: Ra=30, U=0, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x9B407800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_ra_31_max_0_9b407c00() {
    // Encoding: 0x9B407C00
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Ra = 31 (Max)
    // Fields: Rn=0, Rm=0, U=0, Rd=0, Ra=31
    let encoding: u32 = 0x9B407C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rn_0_min_0_9b400000() {
    // Encoding: 0x9B400000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rn = 0 (Min)
    // Fields: U=0, Rm=0, Rn=0, Rd=0, Ra=0
    let encoding: u32 = 0x9B400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rn_1_poweroftwo_0_9b400020() {
    // Encoding: 0x9B400020
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, U=0, Rm=0, Ra=0, Rn=1
    let encoding: u32 = 0x9B400020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rn_30_poweroftwominusone_0_9b4003c0()
{
    // Encoding: 0x9B4003C0
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, U=0, Rd=0, Ra=0, Rm=0
    let encoding: u32 = 0x9B4003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rn_31_max_0_9b4003e0() {
    // Encoding: 0x9B4003E0
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rn = 31 (Max)
    // Fields: Rn=31, U=0, Rd=0, Rm=0, Ra=0
    let encoding: u32 = 0x9B4003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rd_0_min_0_9b400000() {
    // Encoding: 0x9B400000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rd = 0 (Min)
    // Fields: Ra=0, Rd=0, U=0, Rn=0, Rm=0
    let encoding: u32 = 0x9B400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rd_1_poweroftwo_0_9b400001() {
    // Encoding: 0x9B400001
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Ra=0, Rn=0, Rd=1, U=0
    let encoding: u32 = 0x9B400001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rd_30_poweroftwominusone_0_9b40001e()
{
    // Encoding: 0x9B40001E
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rm=0, U=0, Ra=0, Rn=0
    let encoding: u32 = 0x9B40001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_field_rd_31_max_0_9b40001f() {
    // Encoding: 0x9B40001F
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field Rd = 31 (Max)
    // Fields: U=0, Rm=0, Rn=0, Rd=31, Ra=0
    let encoding: u32 = 0x9B40001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// U=0 (minimum value)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_combo_0_0_9b400000() {
    // Encoding: 0x9B400000
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi field combination: U=0, Rm=0, Ra=0, Rn=0, Rd=0
    // Fields: Rn=0, Rd=0, U=0, Rm=0, Ra=0
    let encoding: u32 = 0x9B400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_special_rn_31_stack_pointer_sp_may_require_alignment_0_9b4003e0(
) {
    // Encoding: 0x9B4003E0
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rd=0, Rn=31, U=0, Ra=0
    let encoding: u32 = 0x9B4003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_9b40001f(
) {
    // Encoding: 0x9B40001F
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rn=0, U=0, Ra=0, Rm=0
    let encoding: u32 = 0x9B40001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `SMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// small values - high bits zero
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_smulh_oracle_0_9b427c20() {
    // Test SMULH: small values - high bits zero (oracle)
    // Encoding: 0x9B427C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x3);
    set_x(&mut cpu, 1, 0x2);
    let encoding: u32 = 0x9B427C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `SMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// large value * 2 - produces high bits
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_smulh_oracle_1_9b427c20() {
    // Test SMULH: large value * 2 - produces high bits (oracle)
    // Encoding: 0x9B427C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x9B427C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFF,
        "X0 should be 0xFFFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `SMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max * max unsigned
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_smulh_oracle_2_9b427c20() {
    // Test SMULH: max * max unsigned (oracle)
    // Encoding: 0x9B427C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x9B427C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `SMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max positive * max positive
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_smulh_oracle_3_9b427c20() {
    // Test SMULH: max positive * max positive (oracle)
    // Encoding: 0x9B427C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x9B427C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x3FFFFFFFFFFFFFFF,
        "X0 should be 0x3FFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `SMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// 2^32 * 2^32
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_smulh_oracle_4_9b427c20() {
    // Test SMULH: 2^32 * 2^32 (oracle)
    // Encoding: 0x9B427C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x100000000);
    set_x(&mut cpu, 2, 0x100000000);
    let encoding: u32 = 0x9B427C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_reg_write_0_9b400000() {
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi register write: GpFromField("d")
    // Encoding: 0x9B400000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x9B400000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_sp_rn_9b4003e0() {
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi with Rn = SP (31)
    // Encoding: 0x9B4003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x9B4003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_arithmetic_mul_widening_64_128hi
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_integer_arithmetic_mul_widening_64_128hi_zr_rd_9b40001f() {
    // Test aarch64_integer_arithmetic_mul_widening_64_128hi with Rd = ZR (31)
    // Encoding: 0x9B40001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x9B40001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_integer_arithmetic_div Tests
// ============================================================================

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_integer_arithmetic_div_field_sf_0_min_800_1ac00800() {
    // Encoding: 0x1AC00800
    // Test aarch64_integer_arithmetic_div field sf = 0 (Min)
    // Fields: Rm=0, sf=0, o1=0, Rd=0, Rn=0
    let encoding: u32 = 0x1AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_integer_arithmetic_div_field_sf_1_max_800_9ac00800() {
    // Encoding: 0x9AC00800
    // Test aarch64_integer_arithmetic_div field sf = 1 (Max)
    // Fields: Rm=0, Rn=0, sf=1, Rd=0, o1=0
    let encoding: u32 = 0x9AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rm_0_min_800_1ac00800() {
    // Encoding: 0x1AC00800
    // Test aarch64_integer_arithmetic_div field Rm = 0 (Min)
    // Fields: o1=0, sf=0, Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0x1AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rm_1_poweroftwo_800_1ac10800() {
    // Encoding: 0x1AC10800
    // Test aarch64_integer_arithmetic_div field Rm = 1 (PowerOfTwo)
    // Fields: sf=0, Rm=1, o1=0, Rd=0, Rn=0
    let encoding: u32 = 0x1AC10800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rm_30_poweroftwominusone_800_1ade0800() {
    // Encoding: 0x1ADE0800
    // Test aarch64_integer_arithmetic_div field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rn=0, Rd=0, sf=0, o1=0
    let encoding: u32 = 0x1ADE0800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rm_31_max_800_1adf0800() {
    // Encoding: 0x1ADF0800
    // Test aarch64_integer_arithmetic_div field Rm = 31 (Max)
    // Fields: o1=0, Rd=0, sf=0, Rn=0, Rm=31
    let encoding: u32 = 0x1ADF0800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field o1 10 +: 1`
/// Requirement: FieldBoundary { field: "o1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_integer_arithmetic_div_field_o1_0_min_800_1ac00800() {
    // Encoding: 0x1AC00800
    // Test aarch64_integer_arithmetic_div field o1 = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0, sf=0, o1=0
    let encoding: u32 = 0x1AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field o1 10 +: 1`
/// Requirement: FieldBoundary { field: "o1", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_integer_arithmetic_div_field_o1_1_max_800_1ac00c00() {
    // Encoding: 0x1AC00C00
    // Test aarch64_integer_arithmetic_div field o1 = 1 (Max)
    // Fields: Rm=0, sf=0, Rd=0, Rn=0, o1=1
    let encoding: u32 = 0x1AC00C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rn_0_min_800_1ac00800() {
    // Encoding: 0x1AC00800
    // Test aarch64_integer_arithmetic_div field Rn = 0 (Min)
    // Fields: Rn=0, Rm=0, sf=0, Rd=0, o1=0
    let encoding: u32 = 0x1AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rn_1_poweroftwo_800_1ac00820() {
    // Encoding: 0x1AC00820
    // Test aarch64_integer_arithmetic_div field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, o1=0, sf=0, Rm=0, Rd=0
    let encoding: u32 = 0x1AC00820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rn_30_poweroftwominusone_800_1ac00bc0() {
    // Encoding: 0x1AC00BC0
    // Test aarch64_integer_arithmetic_div field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: sf=0, Rm=0, Rd=0, o1=0, Rn=30
    let encoding: u32 = 0x1AC00BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rn_31_max_800_1ac00be0() {
    // Encoding: 0x1AC00BE0
    // Test aarch64_integer_arithmetic_div field Rn = 31 (Max)
    // Fields: o1=0, Rn=31, sf=0, Rm=0, Rd=0
    let encoding: u32 = 0x1AC00BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rd_0_min_800_1ac00800() {
    // Encoding: 0x1AC00800
    // Test aarch64_integer_arithmetic_div field Rd = 0 (Min)
    // Fields: Rn=0, Rd=0, Rm=0, sf=0, o1=0
    let encoding: u32 = 0x1AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rd_1_poweroftwo_800_1ac00801() {
    // Encoding: 0x1AC00801
    // Test aarch64_integer_arithmetic_div field Rd = 1 (PowerOfTwo)
    // Fields: o1=0, Rd=1, sf=0, Rn=0, Rm=0
    let encoding: u32 = 0x1AC00801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rd_30_poweroftwominusone_800_1ac0081e() {
    // Encoding: 0x1AC0081E
    // Test aarch64_integer_arithmetic_div field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: o1=0, Rm=0, Rd=30, sf=0, Rn=0
    let encoding: u32 = 0x1AC0081E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_integer_arithmetic_div_field_rd_31_max_800_1ac0081f() {
    // Encoding: 0x1AC0081F
    // Test aarch64_integer_arithmetic_div field Rd = 31 (Max)
    // Fields: Rd=31, Rm=0, sf=0, o1=0, Rn=0
    let encoding: u32 = 0x1AC0081F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// sf=0 (8-bit / byte size)
#[test]
fn test_aarch64_integer_arithmetic_div_combo_0_800_1ac00800() {
    // Encoding: 0x1AC00800
    // Test aarch64_integer_arithmetic_div field combination: sf=0, Rm=0, o1=0, Rn=0, Rd=0
    // Fields: Rm=0, Rn=0, Rd=0, sf=0, o1=0
    let encoding: u32 = 0x1AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_integer_arithmetic_div_special_sf_0_size_variant_0_2048_1ac00800() {
    // Encoding: 0x1AC00800
    // Test aarch64_integer_arithmetic_div special value sf = 0 (Size variant 0)
    // Fields: sf=0, Rn=0, o1=0, Rm=0, Rd=0
    let encoding: u32 = 0x1AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_integer_arithmetic_div_special_sf_1_size_variant_1_2048_9ac00800() {
    // Encoding: 0x9AC00800
    // Test aarch64_integer_arithmetic_div special value sf = 1 (Size variant 1)
    // Fields: Rm=0, Rn=0, Rd=0, sf=1, o1=0
    let encoding: u32 = 0x9AC00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_integer_arithmetic_div_special_rn_31_stack_pointer_sp_may_require_alignment_2048_1ac00be0(
) {
    // Encoding: 0x1AC00BE0
    // Test aarch64_integer_arithmetic_div special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, sf=0, Rn=31, Rd=0, o1=0
    let encoding: u32 = 0x1AC00BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_integer_arithmetic_div_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_2048_1ac0081f(
) {
    // Encoding: 0x1AC0081F
    // Test aarch64_integer_arithmetic_div special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rm=0, o1=0, sf=0, Rn=0
    let encoding: u32 = 0x1AC0081F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// small values - high bits zero
#[test]
fn test_aarch64_integer_arithmetic_div_umulh_oracle_0_9bc27c20() {
    // Test UMULH: small values - high bits zero (oracle)
    // Encoding: 0x9BC27C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x2);
    set_x(&mut cpu, 2, 0x3);
    let encoding: u32 = 0x9BC27C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// large value * 2 - produces high bits
#[test]
fn test_aarch64_integer_arithmetic_div_umulh_oracle_1_9bc27c20() {
    // Test UMULH: large value * 2 - produces high bits (oracle)
    // Encoding: 0x9BC27C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x9BC27C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max * max unsigned
#[test]
fn test_aarch64_integer_arithmetic_div_umulh_oracle_2_9bc27c20() {
    // Test UMULH: max * max unsigned (oracle)
    // Encoding: 0x9BC27C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x9BC27C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFE,
        "X0 should be 0xFFFFFFFFFFFFFFFE"
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max positive * max positive
#[test]
fn test_aarch64_integer_arithmetic_div_umulh_oracle_3_9bc27c20() {
    // Test UMULH: max positive * max positive (oracle)
    // Encoding: 0x9BC27C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x9BC27C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x3FFFFFFFFFFFFFFF,
        "X0 should be 0x3FFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// 2^32 * 2^32
#[test]
fn test_aarch64_integer_arithmetic_div_umulh_oracle_4_9bc27c20() {
    // Test UMULH: 2^32 * 2^32 (oracle)
    // Encoding: 0x9BC27C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x100000000);
    set_x(&mut cpu, 1, 0x100000000);
    let encoding: u32 = 0x9BC27C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_integer_arithmetic_div_reg_write_0_1ac00800() {
    // Test aarch64_integer_arithmetic_div register write: GpFromField("d")
    // Encoding: 0x1AC00800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1AC00800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_integer_arithmetic_div_sp_rn_1ac00be0() {
    // Test aarch64_integer_arithmetic_div with Rn = SP (31)
    // Encoding: 0x1AC00BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1AC00BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_arithmetic_div
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_integer_arithmetic_div_zr_rd_1ac0081f() {
    // Test aarch64_integer_arithmetic_div with Rd = ZR (31)
    // Encoding: 0x1AC0081F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1AC0081F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}
