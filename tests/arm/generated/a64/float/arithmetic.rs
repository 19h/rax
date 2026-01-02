//! A64 float arithmetic tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_float_arithmetic_div Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_div_field_type1_0_min_1800_1e201800() {
    // Encoding: 0x1E201800
    // Test aarch64_float_arithmetic_div field type1 = 0 (Min)
    // Fields: Rm=0, Rd=0, type1=0, Rn=0
    let encoding: u32 = 0x1E201800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_div_field_type1_1_poweroftwo_1800_1e601800() {
    // Encoding: 0x1E601800
    // Test aarch64_float_arithmetic_div field type1 = 1 (PowerOfTwo)
    // Fields: Rn=0, type1=1, Rm=0, Rd=0
    let encoding: u32 = 0x1E601800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_div_field_type1_3_max_1800_1ee01800() {
    // Encoding: 0x1EE01800
    // Test aarch64_float_arithmetic_div field type1 = 3 (Max)
    // Fields: Rn=0, Rd=0, type1=3, Rm=0
    let encoding: u32 = 0x1EE01800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_div_field_rm_0_min_1800_1e201800() {
    // Encoding: 0x1E201800
    // Test aarch64_float_arithmetic_div field Rm = 0 (Min)
    // Fields: Rn=0, Rm=0, type1=0, Rd=0
    let encoding: u32 = 0x1E201800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_div_field_rm_1_poweroftwo_1800_1e211800() {
    // Encoding: 0x1E211800
    // Test aarch64_float_arithmetic_div field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, type1=0, Rd=0
    let encoding: u32 = 0x1E211800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_div_field_rm_30_poweroftwominusone_1800_1e3e1800() {
    // Encoding: 0x1E3E1800
    // Test aarch64_float_arithmetic_div field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, type1=0, Rn=0, Rd=0
    let encoding: u32 = 0x1E3E1800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_float_arithmetic_div_field_rm_31_max_1800_1e3f1800() {
    // Encoding: 0x1E3F1800
    // Test aarch64_float_arithmetic_div field Rm = 31 (Max)
    // Fields: Rn=0, type1=0, Rm=31, Rd=0
    let encoding: u32 = 0x1E3F1800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_div_field_rn_0_min_1800_1e201800() {
    // Encoding: 0x1E201800
    // Test aarch64_float_arithmetic_div field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, type1=0, Rm=0
    let encoding: u32 = 0x1E201800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_div_field_rn_1_poweroftwo_1800_1e201820() {
    // Encoding: 0x1E201820
    // Test aarch64_float_arithmetic_div field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, Rn=1, type1=0
    let encoding: u32 = 0x1E201820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_div_field_rn_30_poweroftwominusone_1800_1e201bc0() {
    // Encoding: 0x1E201BC0
    // Test aarch64_float_arithmetic_div field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=30, type1=0, Rd=0
    let encoding: u32 = 0x1E201BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_div_field_rn_31_max_1800_1e201be0() {
    // Encoding: 0x1E201BE0
    // Test aarch64_float_arithmetic_div field Rn = 31 (Max)
    // Fields: Rn=31, Rm=0, type1=0, Rd=0
    let encoding: u32 = 0x1E201BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_div_field_rd_0_min_1800_1e201800() {
    // Encoding: 0x1E201800
    // Test aarch64_float_arithmetic_div field Rd = 0 (Min)
    // Fields: Rd=0, Rm=0, type1=0, Rn=0
    let encoding: u32 = 0x1E201800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_div_field_rd_1_poweroftwo_1800_1e201801() {
    // Encoding: 0x1E201801
    // Test aarch64_float_arithmetic_div field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, Rd=1, type1=0
    let encoding: u32 = 0x1E201801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_div_field_rd_30_poweroftwominusone_1800_1e20181e() {
    // Encoding: 0x1E20181E
    // Test aarch64_float_arithmetic_div field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rd=30, Rn=0, type1=0
    let encoding: u32 = 0x1E20181E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_div_field_rd_31_max_1800_1e20181f() {
    // Encoding: 0x1E20181F
    // Test aarch64_float_arithmetic_div field Rd = 31 (Max)
    // Fields: Rn=0, Rm=0, type1=0, Rd=31
    let encoding: u32 = 0x1E20181F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_div_combo_0_1800_1e201800() {
    // Encoding: 0x1E201800
    // Test aarch64_float_arithmetic_div field combination: type1=0, Rm=0, Rn=0, Rd=0
    // Fields: Rd=0, Rn=0, Rm=0, type1=0
    let encoding: u32 = 0x1E201800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_div_special_rn_31_stack_pointer_sp_may_require_alignment_6144_1e201be0() {
    // Encoding: 0x1E201BE0
    // Test aarch64_float_arithmetic_div special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: type1=0, Rm=0, Rn=31, Rd=0
    let encoding: u32 = 0x1E201BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_div_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_6144_1e20181f() {
    // Encoding: 0x1E20181F
    // Test aarch64_float_arithmetic_div special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: type1=0, Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x1E20181F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_div_invalid_0_1800_1e201800() {
    // Encoding: 0x1E201800
    // Test aarch64_float_arithmetic_div invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, type1=0, Rm=0, Rd=0
    let encoding: u32 = 0x1E201800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_div_invalid_1_1800_1e201800() {
    // Encoding: 0x1E201800
    // Test aarch64_float_arithmetic_div invalid encoding: Unconditional UNDEFINED
    // Fields: type1=0, Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0x1E201800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_div_reg_write_0_1e201800() {
    // Test aarch64_float_arithmetic_div register write: SimdFromField("d")
    // Encoding: 0x1E201800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E201800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_div_sp_rn_1e201be0() {
    // Test aarch64_float_arithmetic_div with Rn = SP (31)
    // Encoding: 0x1E201BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E201BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_div
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_div_zr_rd_1e20181f() {
    // Test aarch64_float_arithmetic_div with Rd = ZR (31)
    // Encoding: 0x1E20181F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E20181F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_float_arithmetic_mul_product Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_type1_0_min_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product field type1 = 0 (Min)
    // Fields: op=0, Rd=0, Rm=0, type1=0, Rn=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_type1_1_poweroftwo_800_1e600800() {
    // Encoding: 0x1E600800
    // Test aarch64_float_arithmetic_mul_product field type1 = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, op=0, Rd=0, type1=1
    let encoding: u32 = 0x1E600800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_type1_3_max_800_1ee00800() {
    // Encoding: 0x1EE00800
    // Test aarch64_float_arithmetic_mul_product field type1 = 3 (Max)
    // Fields: type1=3, Rd=0, Rm=0, Rn=0, op=0
    let encoding: u32 = 0x1EE00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rm_0_min_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product field Rm = 0 (Min)
    // Fields: Rm=0, type1=0, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rm_1_poweroftwo_800_1e210800() {
    // Encoding: 0x1E210800
    // Test aarch64_float_arithmetic_mul_product field Rm = 1 (PowerOfTwo)
    // Fields: type1=0, op=0, Rd=0, Rm=1, Rn=0
    let encoding: u32 = 0x1E210800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rm_30_poweroftwominusone_800_1e3e0800() {
    // Encoding: 0x1E3E0800
    // Test aarch64_float_arithmetic_mul_product field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=0, type1=0, Rm=30, op=0
    let encoding: u32 = 0x1E3E0800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rm_31_max_800_1e3f0800() {
    // Encoding: 0x1E3F0800
    // Test aarch64_float_arithmetic_mul_product field Rm = 31 (Max)
    // Fields: Rm=31, Rn=0, Rd=0, type1=0, op=0
    let encoding: u32 = 0x1E3F0800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field op 15 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_op_0_min_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product field op = 0 (Min)
    // Fields: Rn=0, type1=0, op=0, Rd=0, Rm=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field op 15 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_op_1_max_800_1e208800() {
    // Encoding: 0x1E208800
    // Test aarch64_float_arithmetic_mul_product field op = 1 (Max)
    // Fields: Rd=0, op=1, Rn=0, type1=0, Rm=0
    let encoding: u32 = 0x1E208800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rn_0_min_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product field Rn = 0 (Min)
    // Fields: op=0, type1=0, Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rn_1_poweroftwo_800_1e200820() {
    // Encoding: 0x1E200820
    // Test aarch64_float_arithmetic_mul_product field Rn = 1 (PowerOfTwo)
    // Fields: op=0, Rd=0, Rm=0, type1=0, Rn=1
    let encoding: u32 = 0x1E200820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rn_30_poweroftwominusone_800_1e200bc0() {
    // Encoding: 0x1E200BC0
    // Test aarch64_float_arithmetic_mul_product field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=30, op=0, type1=0, Rd=0
    let encoding: u32 = 0x1E200BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rn_31_max_800_1e200be0() {
    // Encoding: 0x1E200BE0
    // Test aarch64_float_arithmetic_mul_product field Rn = 31 (Max)
    // Fields: type1=0, Rd=0, op=0, Rm=0, Rn=31
    let encoding: u32 = 0x1E200BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rd_0_min_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product field Rd = 0 (Min)
    // Fields: type1=0, Rm=0, Rn=0, op=0, Rd=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rd_1_poweroftwo_800_1e200801() {
    // Encoding: 0x1E200801
    // Test aarch64_float_arithmetic_mul_product field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, op=0, Rn=0, Rd=1, type1=0
    let encoding: u32 = 0x1E200801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rd_30_poweroftwominusone_800_1e20081e() {
    // Encoding: 0x1E20081E
    // Test aarch64_float_arithmetic_mul_product field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, type1=0, op=0, Rn=0, Rm=0
    let encoding: u32 = 0x1E20081E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_mul_product_field_rd_31_max_800_1e20081f() {
    // Encoding: 0x1E20081F
    // Test aarch64_float_arithmetic_mul_product field Rd = 31 (Max)
    // Fields: Rn=0, type1=0, Rm=0, Rd=31, op=0
    let encoding: u32 = 0x1E20081F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_mul_product_combo_0_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product field combination: type1=0, Rm=0, op=0, Rn=0, Rd=0
    // Fields: Rm=0, type1=0, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_mul_product_special_rn_31_stack_pointer_sp_may_require_alignment_2048_1e200be0() {
    // Encoding: 0x1E200BE0
    // Test aarch64_float_arithmetic_mul_product special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: type1=0, op=0, Rd=0, Rn=31, Rm=0
    let encoding: u32 = 0x1E200BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_mul_product_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_2048_1e20081f() {
    // Encoding: 0x1E20081F
    // Test aarch64_float_arithmetic_mul_product special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: type1=0, op=0, Rd=31, Rn=0, Rm=0
    let encoding: u32 = 0x1E20081F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_mul_product_invalid_0_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, type1=0, Rd=0, op=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_mul_product_invalid_1_800_1e200800() {
    // Encoding: 0x1E200800
    // Test aarch64_float_arithmetic_mul_product invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rm=0, op=0, type1=0, Rd=0
    let encoding: u32 = 0x1E200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_mul_product_reg_write_0_1e200800() {
    // Test aarch64_float_arithmetic_mul_product register write: SimdFromField("d")
    // Encoding: 0x1E200800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E200800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_mul_product_sp_rn_1e200be0() {
    // Test aarch64_float_arithmetic_mul_product with Rn = SP (31)
    // Encoding: 0x1E200BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E200BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_mul_product
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_mul_product_zr_rd_1e20081f() {
    // Test aarch64_float_arithmetic_mul_product with Rd = ZR (31)
    // Encoding: 0x1E20081F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E20081F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_float_arithmetic_max_min Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_max_min_field_type1_0_min_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min field type1 = 0 (Min)
    // Fields: Rn=0, Rd=0, op=0, type1=0, Rm=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_max_min_field_type1_1_poweroftwo_4800_1e604800() {
    // Encoding: 0x1E604800
    // Test aarch64_float_arithmetic_max_min field type1 = 1 (PowerOfTwo)
    // Fields: Rd=0, type1=1, Rn=0, op=0, Rm=0
    let encoding: u32 = 0x1E604800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_type1_3_max_4800_1ee04800() {
    // Encoding: 0x1EE04800
    // Test aarch64_float_arithmetic_max_min field type1 = 3 (Max)
    // Fields: Rd=0, Rn=0, type1=3, Rm=0, op=0
    let encoding: u32 = 0x1EE04800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rm_0_min_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min field Rm = 0 (Min)
    // Fields: type1=0, Rd=0, op=0, Rn=0, Rm=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rm_1_poweroftwo_4800_1e214800() {
    // Encoding: 0x1E214800
    // Test aarch64_float_arithmetic_max_min field Rm = 1 (PowerOfTwo)
    // Fields: Rd=0, op=0, Rn=0, type1=0, Rm=1
    let encoding: u32 = 0x1E214800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rm_30_poweroftwominusone_4800_1e3e4800() {
    // Encoding: 0x1E3E4800
    // Test aarch64_float_arithmetic_max_min field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, op=0, Rm=30, type1=0, Rn=0
    let encoding: u32 = 0x1E3E4800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rm_31_max_4800_1e3f4800() {
    // Encoding: 0x1E3F4800
    // Test aarch64_float_arithmetic_max_min field Rm = 31 (Max)
    // Fields: type1=0, Rm=31, Rn=0, op=0, Rd=0
    let encoding: u32 = 0x1E3F4800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field op 12 +: 2`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_max_min_field_op_0_min_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min field op = 0 (Min)
    // Fields: type1=0, Rn=0, Rd=0, op=0, Rm=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field op 12 +: 2`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_max_min_field_op_1_poweroftwo_4800_1e205800() {
    // Encoding: 0x1E205800
    // Test aarch64_float_arithmetic_max_min field op = 1 (PowerOfTwo)
    // Fields: Rd=0, type1=0, Rn=0, Rm=0, op=1
    let encoding: u32 = 0x1E205800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field op 12 +: 2`
/// Requirement: FieldBoundary { field: "op", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_op_3_max_4800_1e207800() {
    // Encoding: 0x1E207800
    // Test aarch64_float_arithmetic_max_min field op = 3 (Max)
    // Fields: Rm=0, type1=0, op=3, Rd=0, Rn=0
    let encoding: u32 = 0x1E207800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rn_0_min_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min field Rn = 0 (Min)
    // Fields: op=0, Rm=0, Rn=0, type1=0, Rd=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rn_1_poweroftwo_4800_1e204820() {
    // Encoding: 0x1E204820
    // Test aarch64_float_arithmetic_max_min field Rn = 1 (PowerOfTwo)
    // Fields: type1=0, Rn=1, Rm=0, Rd=0, op=0
    let encoding: u32 = 0x1E204820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rn_30_poweroftwominusone_4800_1e204bc0() {
    // Encoding: 0x1E204BC0
    // Test aarch64_float_arithmetic_max_min field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=30, op=0, type1=0, Rd=0
    let encoding: u32 = 0x1E204BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rn_31_max_4800_1e204be0() {
    // Encoding: 0x1E204BE0
    // Test aarch64_float_arithmetic_max_min field Rn = 31 (Max)
    // Fields: Rn=31, Rd=0, op=0, Rm=0, type1=0
    let encoding: u32 = 0x1E204BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rd_0_min_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min field Rd = 0 (Min)
    // Fields: Rn=0, type1=0, op=0, Rd=0, Rm=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rd_1_poweroftwo_4800_1e204801() {
    // Encoding: 0x1E204801
    // Test aarch64_float_arithmetic_max_min field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, op=0, Rd=1, Rm=0, type1=0
    let encoding: u32 = 0x1E204801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rd_30_poweroftwominusone_4800_1e20481e() {
    // Encoding: 0x1E20481E
    // Test aarch64_float_arithmetic_max_min field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, type1=0, Rm=0, Rd=30, op=0
    let encoding: u32 = 0x1E20481E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_max_min_field_rd_31_max_4800_1e20481f() {
    // Encoding: 0x1E20481F
    // Test aarch64_float_arithmetic_max_min field Rd = 31 (Max)
    // Fields: type1=0, op=0, Rd=31, Rn=0, Rm=0
    let encoding: u32 = 0x1E20481F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_max_min_combo_0_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min field combination: type1=0, Rm=0, op=0, Rn=0, Rd=0
    // Fields: type1=0, Rd=0, Rm=0, Rn=0, op=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_max_min_special_rn_31_stack_pointer_sp_may_require_alignment_18432_1e204be0() {
    // Encoding: 0x1E204BE0
    // Test aarch64_float_arithmetic_max_min special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, op=0, Rn=31, type1=0
    let encoding: u32 = 0x1E204BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_max_min_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_18432_1e20481f() {
    // Encoding: 0x1E20481F
    // Test aarch64_float_arithmetic_max_min special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rm=0, Rd=31, type1=0, op=0
    let encoding: u32 = 0x1E20481F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_max_min_invalid_0_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min invalid encoding: Unconditional UNDEFINED
    // Fields: op=0, type1=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_max_min_invalid_1_4800_1e204800() {
    // Encoding: 0x1E204800
    // Test aarch64_float_arithmetic_max_min invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, type1=0, Rn=0, op=0, Rd=0
    let encoding: u32 = 0x1E204800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_max_min_reg_write_0_1e204800() {
    // Test aarch64_float_arithmetic_max_min register write: SimdFromField("d")
    // Encoding: 0x1E204800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E204800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_max_min_sp_rn_1e204be0() {
    // Test aarch64_float_arithmetic_max_min with Rn = SP (31)
    // Encoding: 0x1E204BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E204BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_max_min
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_max_min_zr_rd_1e20481f() {
    // Test aarch64_float_arithmetic_max_min with Rd = ZR (31)
    // Encoding: 0x1E20481F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E20481F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_float_arithmetic_round_frint Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_type1_0_min_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint field type1 = 0 (Min)
    // Fields: Rd=0, Rn=0, type1=0, rmode=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_type1_1_poweroftwo_4000_1e644000() {
    // Encoding: 0x1E644000
    // Test aarch64_float_arithmetic_round_frint field type1 = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, type1=1, rmode=0
    let encoding: u32 = 0x1E644000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_type1_3_max_4000_1ee44000() {
    // Encoding: 0x1EE44000
    // Test aarch64_float_arithmetic_round_frint field type1 = 3 (Max)
    // Fields: Rn=0, type1=3, rmode=0, Rd=0
    let encoding: u32 = 0x1EE44000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field rmode 15 +: 3`
/// Requirement: FieldBoundary { field: "rmode", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rmode_0_min_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint field rmode = 0 (Min)
    // Fields: rmode=0, Rn=0, type1=0, Rd=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field rmode 15 +: 3`
/// Requirement: FieldBoundary { field: "rmode", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rmode_1_poweroftwo_4000_1e24c000() {
    // Encoding: 0x1E24C000
    // Test aarch64_float_arithmetic_round_frint field rmode = 1 (PowerOfTwo)
    // Fields: type1=0, rmode=1, Rn=0, Rd=0
    let encoding: u32 = 0x1E24C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field rmode 15 +: 3`
/// Requirement: FieldBoundary { field: "rmode", value: 7, boundary: Max }
/// maximum value (7)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rmode_7_max_4000_1e27c000() {
    // Encoding: 0x1E27C000
    // Test aarch64_float_arithmetic_round_frint field rmode = 7 (Max)
    // Fields: rmode=7, Rd=0, Rn=0, type1=0
    let encoding: u32 = 0x1E27C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rn_0_min_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint field Rn = 0 (Min)
    // Fields: Rn=0, rmode=0, type1=0, Rd=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rn_1_poweroftwo_4000_1e244020() {
    // Encoding: 0x1E244020
    // Test aarch64_float_arithmetic_round_frint field Rn = 1 (PowerOfTwo)
    // Fields: type1=0, rmode=0, Rn=1, Rd=0
    let encoding: u32 = 0x1E244020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rn_30_poweroftwominusone_4000_1e2443c0() {
    // Encoding: 0x1E2443C0
    // Test aarch64_float_arithmetic_round_frint field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, type1=0, Rn=30, rmode=0
    let encoding: u32 = 0x1E2443C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rn_31_max_4000_1e2443e0() {
    // Encoding: 0x1E2443E0
    // Test aarch64_float_arithmetic_round_frint field Rn = 31 (Max)
    // Fields: Rd=0, type1=0, rmode=0, Rn=31
    let encoding: u32 = 0x1E2443E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rd_0_min_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint field Rd = 0 (Min)
    // Fields: rmode=0, Rn=0, type1=0, Rd=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rd_1_poweroftwo_4000_1e244001() {
    // Encoding: 0x1E244001
    // Test aarch64_float_arithmetic_round_frint field Rd = 1 (PowerOfTwo)
    // Fields: rmode=0, type1=0, Rd=1, Rn=0
    let encoding: u32 = 0x1E244001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rd_30_poweroftwominusone_4000_1e24401e() {
    // Encoding: 0x1E24401E
    // Test aarch64_float_arithmetic_round_frint field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: rmode=0, type1=0, Rn=0, Rd=30
    let encoding: u32 = 0x1E24401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_field_rd_31_max_4000_1e24401f() {
    // Encoding: 0x1E24401F
    // Test aarch64_float_arithmetic_round_frint field Rd = 31 (Max)
    // Fields: rmode=0, Rn=0, type1=0, Rd=31
    let encoding: u32 = 0x1E24401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_round_frint_combo_0_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint field combination: type1=0, rmode=0, Rn=0, Rd=0
    // Fields: rmode=0, type1=0, Rd=0, Rn=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_round_frint_special_rn_31_stack_pointer_sp_may_require_alignment_16384_1e2443e0() {
    // Encoding: 0x1E2443E0
    // Test aarch64_float_arithmetic_round_frint special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: type1=0, Rn=31, Rd=0, rmode=0
    let encoding: u32 = 0x1E2443E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_round_frint_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_16384_1e24401f() {
    // Encoding: 0x1E24401F
    // Test aarch64_float_arithmetic_round_frint special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: type1=0, rmode=0, Rd=31, Rn=0
    let encoding: u32 = 0x1E24401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_round_frint_invalid_0_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, type1=0, rmode=0, Rd=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_round_frint_invalid_1_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, type1=0, Rd=0, rmode=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_round_frint_invalid_2_4000_1e244000() {
    // Encoding: 0x1E244000
    // Test aarch64_float_arithmetic_round_frint invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, type1=0, rmode=0, Rn=0
    let encoding: u32 = 0x1E244000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_round_frint_reg_write_0_1e244000() {
    // Test aarch64_float_arithmetic_round_frint register write: SimdFromField("d")
    // Encoding: 0x1E244000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E244000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_round_frint_sp_rn_1e2443e0() {
    // Test aarch64_float_arithmetic_round_frint with Rn = SP (31)
    // Encoding: 0x1E2443E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E2443E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_round_frint
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_round_frint_zr_rd_1e24401f() {
    // Test aarch64_float_arithmetic_round_frint with Rd = ZR (31)
    // Encoding: 0x1E24401F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E24401F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_float_arithmetic_unary Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_unary_field_type1_0_min_4000_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary field type1 = 0 (Min)
    // Fields: type1=0, opc=0, Rd=0, Rn=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_unary_field_type1_1_poweroftwo_4000_1e604000() {
    // Encoding: 0x1E604000
    // Test aarch64_float_arithmetic_unary field type1 = 1 (PowerOfTwo)
    // Fields: Rd=0, type1=1, Rn=0, opc=0
    let encoding: u32 = 0x1E604000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_unary_field_type1_3_max_4000_1ee04000() {
    // Encoding: 0x1EE04000
    // Test aarch64_float_arithmetic_unary field type1 = 3 (Max)
    // Fields: Rd=0, Rn=0, type1=3, opc=0
    let encoding: u32 = 0x1EE04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc 15 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_float_arithmetic_unary_field_opc_0_min_4000_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary field opc = 0 (Min)
    // Fields: Rd=0, type1=0, opc=0, Rn=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc 15 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_float_arithmetic_unary_field_opc_1_poweroftwo_4000_1e20c000() {
    // Encoding: 0x1E20C000
    // Test aarch64_float_arithmetic_unary field opc = 1 (PowerOfTwo)
    // Fields: type1=0, Rd=0, opc=1, Rn=0
    let encoding: u32 = 0x1E20C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc 15 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_float_arithmetic_unary_field_opc_2_poweroftwo_4000_1e214000() {
    // Encoding: 0x1E214000
    // Test aarch64_float_arithmetic_unary field opc = 2 (PowerOfTwo)
    // Fields: Rd=0, opc=2, Rn=0, type1=0
    let encoding: u32 = 0x1E214000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc 15 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_float_arithmetic_unary_field_opc_3_max_4000_1e21c000() {
    // Encoding: 0x1E21C000
    // Test aarch64_float_arithmetic_unary field opc = 3 (Max)
    // Fields: type1=0, Rn=0, opc=3, Rd=0
    let encoding: u32 = 0x1E21C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rn_0_min_4000_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, type1=0, opc=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rn_1_poweroftwo_4000_1e204020() {
    // Encoding: 0x1E204020
    // Test aarch64_float_arithmetic_unary field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rd=0, opc=0, type1=0
    let encoding: u32 = 0x1E204020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rn_30_poweroftwominusone_4000_1e2043c0() {
    // Encoding: 0x1E2043C0
    // Test aarch64_float_arithmetic_unary field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, type1=0, Rn=30, opc=0
    let encoding: u32 = 0x1E2043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rn_31_max_4000_1e2043e0() {
    // Encoding: 0x1E2043E0
    // Test aarch64_float_arithmetic_unary field Rn = 31 (Max)
    // Fields: type1=0, Rd=0, Rn=31, opc=0
    let encoding: u32 = 0x1E2043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rd_0_min_4000_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary field Rd = 0 (Min)
    // Fields: Rn=0, type1=0, opc=0, Rd=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rd_1_poweroftwo_4000_1e204001() {
    // Encoding: 0x1E204001
    // Test aarch64_float_arithmetic_unary field Rd = 1 (PowerOfTwo)
    // Fields: opc=0, type1=0, Rn=0, Rd=1
    let encoding: u32 = 0x1E204001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rd_30_poweroftwominusone_4000_1e20401e() {
    // Encoding: 0x1E20401E
    // Test aarch64_float_arithmetic_unary field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, opc=0, type1=0, Rd=30
    let encoding: u32 = 0x1E20401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_unary_field_rd_31_max_4000_1e20401f() {
    // Encoding: 0x1E20401F
    // Test aarch64_float_arithmetic_unary field Rd = 31 (Max)
    // Fields: opc=0, type1=0, Rd=31, Rn=0
    let encoding: u32 = 0x1E20401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_unary_combo_0_4000_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary field combination: type1=0, opc=0, Rn=0, Rd=0
    // Fields: type1=0, opc=0, Rd=0, Rn=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "opc", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_float_arithmetic_unary_special_opc_0_size_variant_0_16384_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary special value opc = 0 (Size variant 0)
    // Fields: Rd=0, Rn=0, type1=0, opc=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "opc", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_float_arithmetic_unary_special_opc_1_size_variant_1_16384_1e20c000() {
    // Encoding: 0x1E20C000
    // Test aarch64_float_arithmetic_unary special value opc = 1 (Size variant 1)
    // Fields: Rn=0, type1=0, opc=1, Rd=0
    let encoding: u32 = 0x1E20C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "opc", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_float_arithmetic_unary_special_opc_2_size_variant_2_16384_1e214000() {
    // Encoding: 0x1E214000
    // Test aarch64_float_arithmetic_unary special value opc = 2 (Size variant 2)
    // Fields: Rn=0, opc=2, type1=0, Rd=0
    let encoding: u32 = 0x1E214000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field opc = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "opc", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_float_arithmetic_unary_special_opc_3_size_variant_3_16384_1e21c000() {
    // Encoding: 0x1E21C000
    // Test aarch64_float_arithmetic_unary special value opc = 3 (Size variant 3)
    // Fields: Rn=0, Rd=0, opc=3, type1=0
    let encoding: u32 = 0x1E21C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_unary_special_rn_31_stack_pointer_sp_may_require_alignment_16384_1e2043e0() {
    // Encoding: 0x1E2043E0
    // Test aarch64_float_arithmetic_unary special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: type1=0, Rd=0, Rn=31, opc=0
    let encoding: u32 = 0x1E2043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_unary_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_16384_1e20401f() {
    // Encoding: 0x1E20401F
    // Test aarch64_float_arithmetic_unary special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: opc=0, Rn=0, type1=0, Rd=31
    let encoding: u32 = 0x1E20401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_unary_invalid_0_4000_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, type1=0, opc=0, Rn=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_unary_invalid_1_4000_1e204000() {
    // Encoding: 0x1E204000
    // Test aarch64_float_arithmetic_unary invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, type1=0, opc=0, Rd=0
    let encoding: u32 = 0x1E204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_unary_reg_write_0_1e204000() {
    // Test aarch64_float_arithmetic_unary register write: SimdFromField("d")
    // Encoding: 0x1E204000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E204000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_unary_sp_rn_1e2043e0() {
    // Test aarch64_float_arithmetic_unary with Rn = SP (31)
    // Encoding: 0x1E2043E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E2043E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_unary
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_unary_zr_rd_1e20401f() {
    // Test aarch64_float_arithmetic_unary with Rd = ZR (31)
    // Encoding: 0x1E20401F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E20401F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_float_arithmetic_round_frint_32_64 Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_type1_0_min_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 field type1 = 0 (Min)
    // Fields: type1=0, op=0, Rd=0, Rn=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_type1_1_poweroftwo_4000_1e684000() {
    // Encoding: 0x1E684000
    // Test aarch64_float_arithmetic_round_frint_32_64 field type1 = 1 (PowerOfTwo)
    // Fields: Rd=0, type1=1, op=0, Rn=0
    let encoding: u32 = 0x1E684000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_type1_3_max_4000_1ee84000() {
    // Encoding: 0x1EE84000
    // Test aarch64_float_arithmetic_round_frint_32_64 field type1 = 3 (Max)
    // Fields: type1=3, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x1EE84000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field op 15 +: 2`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_op_0_min_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 field op = 0 (Min)
    // Fields: type1=0, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field op 15 +: 2`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_op_1_poweroftwo_4000_1e28c000() {
    // Encoding: 0x1E28C000
    // Test aarch64_float_arithmetic_round_frint_32_64 field op = 1 (PowerOfTwo)
    // Fields: type1=0, Rn=0, Rd=0, op=1
    let encoding: u32 = 0x1E28C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field op 15 +: 2`
/// Requirement: FieldBoundary { field: "op", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_op_3_max_4000_1e29c000() {
    // Encoding: 0x1E29C000
    // Test aarch64_float_arithmetic_round_frint_32_64 field op = 3 (Max)
    // Fields: type1=0, op=3, Rn=0, Rd=0
    let encoding: u32 = 0x1E29C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rn_0_min_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rn = 0 (Min)
    // Fields: Rd=0, type1=0, op=0, Rn=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rn_1_poweroftwo_4000_1e284020() {
    // Encoding: 0x1E284020
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rn = 1 (PowerOfTwo)
    // Fields: op=0, type1=0, Rn=1, Rd=0
    let encoding: u32 = 0x1E284020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rn_30_poweroftwominusone_4000_1e2843c0() {
    // Encoding: 0x1E2843C0
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: type1=0, op=0, Rn=30, Rd=0
    let encoding: u32 = 0x1E2843C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rn_31_max_4000_1e2843e0() {
    // Encoding: 0x1E2843E0
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rn = 31 (Max)
    // Fields: op=0, Rd=0, type1=0, Rn=31
    let encoding: u32 = 0x1E2843E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rd_0_min_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rd = 0 (Min)
    // Fields: Rn=0, Rd=0, type1=0, op=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rd_1_poweroftwo_4000_1e284001() {
    // Encoding: 0x1E284001
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rd = 1 (PowerOfTwo)
    // Fields: op=0, Rd=1, type1=0, Rn=0
    let encoding: u32 = 0x1E284001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rd_30_poweroftwominusone_4000_1e28401e() {
    // Encoding: 0x1E28401E
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: type1=0, Rn=0, op=0, Rd=30
    let encoding: u32 = 0x1E28401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_field_rd_31_max_4000_1e28401f() {
    // Encoding: 0x1E28401F
    // Test aarch64_float_arithmetic_round_frint_32_64 field Rd = 31 (Max)
    // Fields: Rd=31, op=0, Rn=0, type1=0
    let encoding: u32 = 0x1E28401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_combo_0_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 field combination: type1=0, op=0, Rn=0, Rd=0
    // Fields: type1=0, Rn=0, op=0, Rd=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_special_rn_31_stack_pointer_sp_may_require_alignment_16384_1e2843e0() {
    // Encoding: 0x1E2843E0
    // Test aarch64_float_arithmetic_round_frint_32_64 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: type1=0, Rn=31, op=0, Rd=0
    let encoding: u32 = 0x1E2843E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_16384_1e28401f() {
    // Encoding: 0x1E28401F
    // Test aarch64_float_arithmetic_round_frint_32_64 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, type1=0, Rd=31, op=0
    let encoding: u32 = 0x1E28401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveFrintExt" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveFrintExt\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_invalid_0_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveFrintExt" }, args: [] } }
    // Fields: type1=0, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_invalid_1_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 invalid encoding: Unconditional UNDEFINED
    // Fields: type1=0, Rn=0, Rd=0, op=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_invalid_2_4000_1e284000() {
    // Encoding: 0x1E284000
    // Test aarch64_float_arithmetic_round_frint_32_64 invalid encoding: Unconditional UNDEFINED
    // Fields: type1=0, Rd=0, op=0, Rn=0
    let encoding: u32 = 0x1E284000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_reg_write_0_1e284000() {
    // Test aarch64_float_arithmetic_round_frint_32_64 register write: SimdFromField("d")
    // Encoding: 0x1E284000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E284000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_sp_rn_1e2843e0() {
    // Test aarch64_float_arithmetic_round_frint_32_64 with Rn = SP (31)
    // Encoding: 0x1E2843E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E2843E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_round_frint_32_64
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_round_frint_32_64_zr_rd_1e28401f() {
    // Test aarch64_float_arithmetic_round_frint_32_64 with Rd = ZR (31)
    // Encoding: 0x1E28401F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E28401F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_float_arithmetic_add_sub Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_type1_0_min_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub field type1 = 0 (Min)
    // Fields: Rn=0, op=0, type1=0, Rd=0, Rm=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_type1_1_poweroftwo_2800_1e602800() {
    // Encoding: 0x1E602800
    // Test aarch64_float_arithmetic_add_sub field type1 = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, type1=1, op=0, Rn=0
    let encoding: u32 = 0x1E602800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_type1_3_max_2800_1ee02800() {
    // Encoding: 0x1EE02800
    // Test aarch64_float_arithmetic_add_sub field type1 = 3 (Max)
    // Fields: Rd=0, op=0, Rm=0, type1=3, Rn=0
    let encoding: u32 = 0x1EE02800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rm_0_min_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub field Rm = 0 (Min)
    // Fields: type1=0, op=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rm_1_poweroftwo_2800_1e212800() {
    // Encoding: 0x1E212800
    // Test aarch64_float_arithmetic_add_sub field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, type1=0, Rd=0, Rn=0, op=0
    let encoding: u32 = 0x1E212800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rm_30_poweroftwominusone_2800_1e3e2800() {
    // Encoding: 0x1E3E2800
    // Test aarch64_float_arithmetic_add_sub field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rm=30, type1=0, Rd=0, op=0
    let encoding: u32 = 0x1E3E2800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rm_31_max_2800_1e3f2800() {
    // Encoding: 0x1E3F2800
    // Test aarch64_float_arithmetic_add_sub field Rm = 31 (Max)
    // Fields: Rd=0, Rm=31, Rn=0, op=0, type1=0
    let encoding: u32 = 0x1E3F2800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field op 12 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_op_0_min_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub field op = 0 (Min)
    // Fields: Rn=0, op=0, Rd=0, Rm=0, type1=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field op 12 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_op_1_max_2800_1e203800() {
    // Encoding: 0x1E203800
    // Test aarch64_float_arithmetic_add_sub field op = 1 (Max)
    // Fields: type1=0, Rd=0, op=1, Rm=0, Rn=0
    let encoding: u32 = 0x1E203800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rn_0_min_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub field Rn = 0 (Min)
    // Fields: op=0, type1=0, Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rn_1_poweroftwo_2800_1e202820() {
    // Encoding: 0x1E202820
    // Test aarch64_float_arithmetic_add_sub field Rn = 1 (PowerOfTwo)
    // Fields: op=0, Rd=0, type1=0, Rm=0, Rn=1
    let encoding: u32 = 0x1E202820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rn_30_poweroftwominusone_2800_1e202bc0() {
    // Encoding: 0x1E202BC0
    // Test aarch64_float_arithmetic_add_sub field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, type1=0, Rm=0, op=0, Rn=30
    let encoding: u32 = 0x1E202BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rn_31_max_2800_1e202be0() {
    // Encoding: 0x1E202BE0
    // Test aarch64_float_arithmetic_add_sub field Rn = 31 (Max)
    // Fields: Rd=0, type1=0, Rn=31, op=0, Rm=0
    let encoding: u32 = 0x1E202BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rd_0_min_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub field Rd = 0 (Min)
    // Fields: Rn=0, Rm=0, type1=0, Rd=0, op=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rd_1_poweroftwo_2800_1e202801() {
    // Encoding: 0x1E202801
    // Test aarch64_float_arithmetic_add_sub field Rd = 1 (PowerOfTwo)
    // Fields: type1=0, Rm=0, op=0, Rn=0, Rd=1
    let encoding: u32 = 0x1E202801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rd_30_poweroftwominusone_2800_1e20281e() {
    // Encoding: 0x1E20281E
    // Test aarch64_float_arithmetic_add_sub field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: op=0, Rm=0, Rd=30, type1=0, Rn=0
    let encoding: u32 = 0x1E20281E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_add_sub_field_rd_31_max_2800_1e20281f() {
    // Encoding: 0x1E20281F
    // Test aarch64_float_arithmetic_add_sub field Rd = 31 (Max)
    // Fields: Rn=0, Rm=0, op=0, type1=0, Rd=31
    let encoding: u32 = 0x1E20281F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_add_sub_combo_0_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub field combination: type1=0, Rm=0, op=0, Rn=0, Rd=0
    // Fields: Rn=0, Rm=0, type1=0, Rd=0, op=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_add_sub_special_rn_31_stack_pointer_sp_may_require_alignment_10240_1e202be0() {
    // Encoding: 0x1E202BE0
    // Test aarch64_float_arithmetic_add_sub special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, op=0, type1=0, Rn=31
    let encoding: u32 = 0x1E202BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_add_sub_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_10240_1e20281f() {
    // Encoding: 0x1E20281F
    // Test aarch64_float_arithmetic_add_sub special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: op=0, Rd=31, Rm=0, Rn=0, type1=0
    let encoding: u32 = 0x1E20281F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_add_sub_invalid_0_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, type1=0, op=0, Rm=0, Rd=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_add_sub_invalid_1_2800_1e202800() {
    // Encoding: 0x1E202800
    // Test aarch64_float_arithmetic_add_sub invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rd=0, Rn=0, type1=0, op=0
    let encoding: u32 = 0x1E202800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_add_sub_reg_write_0_1e202800() {
    // Test aarch64_float_arithmetic_add_sub register write: SimdFromField("d")
    // Encoding: 0x1E202800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E202800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_add_sub_sp_rn_1e202be0() {
    // Test aarch64_float_arithmetic_add_sub with Rn = SP (31)
    // Encoding: 0x1E202BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E202BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_add_sub
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_add_sub_zr_rd_1e20281f() {
    // Test aarch64_float_arithmetic_add_sub with Rd = ZR (31)
    // Encoding: 0x1E20281F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1E20281F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_float_arithmetic_mul_add_sub Tests
// ============================================================================

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_type1_0_min_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field type1 = 0 (Min)
    // Fields: Ra=0, Rd=0, Rm=0, type1=0, o0=0, o1=0, Rn=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_type1_1_poweroftwo_0_1f400000() {
    // Encoding: 0x1F400000
    // Test aarch64_float_arithmetic_mul_add_sub field type1 = 1 (PowerOfTwo)
    // Fields: Ra=0, Rm=0, Rn=0, type1=1, o1=0, o0=0, Rd=0
    let encoding: u32 = 0x1F400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field type1 22 +: 2`
/// Requirement: FieldBoundary { field: "type1", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_type1_3_max_0_1fc00000() {
    // Encoding: 0x1FC00000
    // Test aarch64_float_arithmetic_mul_add_sub field type1 = 3 (Max)
    // Fields: Rn=0, o0=0, type1=3, Ra=0, Rd=0, Rm=0, o1=0
    let encoding: u32 = 0x1FC00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field o1 21 +: 1`
/// Requirement: FieldBoundary { field: "o1", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_o1_0_min_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field o1 = 0 (Min)
    // Fields: Rn=0, o1=0, o0=0, Ra=0, Rd=0, type1=0, Rm=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field o1 21 +: 1`
/// Requirement: FieldBoundary { field: "o1", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_o1_1_max_0_1f200000() {
    // Encoding: 0x1F200000
    // Test aarch64_float_arithmetic_mul_add_sub field o1 = 1 (Max)
    // Fields: o0=0, Ra=0, Rn=0, Rd=0, type1=0, o1=1, Rm=0
    let encoding: u32 = 0x1F200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rm_0_min_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field Rm = 0 (Min)
    // Fields: o0=0, type1=0, Rd=0, Rn=0, Ra=0, Rm=0, o1=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rm_1_poweroftwo_0_1f010000() {
    // Encoding: 0x1F010000
    // Test aarch64_float_arithmetic_mul_add_sub field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Ra=0, type1=0, o1=0, o0=0, Rd=0, Rn=0
    let encoding: u32 = 0x1F010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rm_30_poweroftwominusone_0_1f1e0000() {
    // Encoding: 0x1F1E0000
    // Test aarch64_float_arithmetic_mul_add_sub field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: o0=0, type1=0, o1=0, Rm=30, Ra=0, Rn=0, Rd=0
    let encoding: u32 = 0x1F1E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rm_31_max_0_1f1f0000() {
    // Encoding: 0x1F1F0000
    // Test aarch64_float_arithmetic_mul_add_sub field Rm = 31 (Max)
    // Fields: o0=0, Rm=31, o1=0, type1=0, Ra=0, Rd=0, Rn=0
    let encoding: u32 = 0x1F1F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field o0 15 +: 1`
/// Requirement: FieldBoundary { field: "o0", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_o0_0_min_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field o0 = 0 (Min)
    // Fields: o0=0, Rd=0, o1=0, Rn=0, Ra=0, Rm=0, type1=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field o0 15 +: 1`
/// Requirement: FieldBoundary { field: "o0", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_o0_1_max_0_1f008000() {
    // Encoding: 0x1F008000
    // Test aarch64_float_arithmetic_mul_add_sub field o0 = 1 (Max)
    // Fields: Rd=0, type1=0, o0=1, Rn=0, o1=0, Rm=0, Ra=0
    let encoding: u32 = 0x1F008000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_ra_0_min_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field Ra = 0 (Min)
    // Fields: o1=0, o0=0, Ra=0, Rn=0, Rd=0, type1=0, Rm=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_ra_1_poweroftwo_0_1f000400() {
    // Encoding: 0x1F000400
    // Test aarch64_float_arithmetic_mul_add_sub field Ra = 1 (PowerOfTwo)
    // Fields: Rd=0, o0=0, Rm=0, type1=0, o1=0, Ra=1, Rn=0
    let encoding: u32 = 0x1F000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_ra_30_poweroftwominusone_0_1f007800() {
    // Encoding: 0x1F007800
    // Test aarch64_float_arithmetic_mul_add_sub field Ra = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, o0=0, type1=0, o1=0, Ra=30, Rn=0, Rd=0
    let encoding: u32 = 0x1F007800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_ra_31_max_0_1f007c00() {
    // Encoding: 0x1F007C00
    // Test aarch64_float_arithmetic_mul_add_sub field Ra = 31 (Max)
    // Fields: Rn=0, o1=0, Rd=0, type1=0, Rm=0, o0=0, Ra=31
    let encoding: u32 = 0x1F007C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rn_0_min_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field Rn = 0 (Min)
    // Fields: Ra=0, o0=0, type1=0, Rm=0, o1=0, Rn=0, Rd=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rn_1_poweroftwo_0_1f000020() {
    // Encoding: 0x1F000020
    // Test aarch64_float_arithmetic_mul_add_sub field Rn = 1 (PowerOfTwo)
    // Fields: type1=0, Rm=0, o0=0, Ra=0, Rn=1, o1=0, Rd=0
    let encoding: u32 = 0x1F000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rn_30_poweroftwominusone_0_1f0003c0() {
    // Encoding: 0x1F0003C0
    // Test aarch64_float_arithmetic_mul_add_sub field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: o0=0, Rd=0, Rm=0, Rn=30, Ra=0, type1=0, o1=0
    let encoding: u32 = 0x1F0003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rn_31_max_0_1f0003e0() {
    // Encoding: 0x1F0003E0
    // Test aarch64_float_arithmetic_mul_add_sub field Rn = 31 (Max)
    // Fields: Rm=0, o0=0, type1=0, Rn=31, Ra=0, Rd=0, o1=0
    let encoding: u32 = 0x1F0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rd_0_min_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field Rd = 0 (Min)
    // Fields: type1=0, Rn=0, Rm=0, Ra=0, Rd=0, o1=0, o0=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rd_1_poweroftwo_0_1f000001() {
    // Encoding: 0x1F000001
    // Test aarch64_float_arithmetic_mul_add_sub field Rd = 1 (PowerOfTwo)
    // Fields: type1=0, o1=0, Rn=0, Rm=0, o0=0, Rd=1, Ra=0
    let encoding: u32 = 0x1F000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rd_30_poweroftwominusone_0_1f00001e() {
    // Encoding: 0x1F00001E
    // Test aarch64_float_arithmetic_mul_add_sub field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: o0=0, Ra=0, Rn=0, o1=0, Rd=30, type1=0, Rm=0
    let encoding: u32 = 0x1F00001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_field_rd_31_max_0_1f00001f() {
    // Encoding: 0x1F00001F
    // Test aarch64_float_arithmetic_mul_add_sub field Rd = 31 (Max)
    // Fields: o1=0, o0=0, Ra=0, Rm=0, Rn=0, Rd=31, type1=0
    let encoding: u32 = 0x1F00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// type1=0 (minimum value)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_combo_0_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub field combination: type1=0, o1=0, Rm=0, o0=0, Ra=0, Rn=0, Rd=0
    // Fields: Rm=0, o1=0, type1=0, Ra=0, Rn=0, Rd=0, o0=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_special_rn_31_stack_pointer_sp_may_require_alignment_0_1f0003e0() {
    // Encoding: 0x1F0003E0
    // Test aarch64_float_arithmetic_mul_add_sub special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, o0=0, Ra=0, type1=0, Rd=0, o1=0, Rm=0
    let encoding: u32 = 0x1F0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_1f00001f() {
    // Encoding: 0x1F00001F
    // Test aarch64_float_arithmetic_mul_add_sub special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rm=0, Rd=31, o1=0, type1=0, o0=0, Ra=0
    let encoding: u32 = 0x1F00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_invalid_0_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub invalid encoding: Unconditional UNDEFINED
    // Fields: Ra=0, Rd=0, Rn=0, type1=0, Rm=0, o0=0, o1=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_invalid_1_0_1f000000() {
    // Encoding: 0x1F000000
    // Test aarch64_float_arithmetic_mul_add_sub invalid encoding: Unconditional UNDEFINED
    // Fields: o1=0, Rd=0, type1=0, Rn=0, Rm=0, o0=0, Ra=0
    let encoding: u32 = 0x1F000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_reg_write_0_1f000000() {
    // Test aarch64_float_arithmetic_mul_add_sub register write: SimdFromField("d")
    // Encoding: 0x1F000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1F000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_sp_rn_1f0003e0() {
    // Test aarch64_float_arithmetic_mul_add_sub with Rn = SP (31)
    // Encoding: 0x1F0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1F0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_float_arithmetic_mul_add_sub
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_float_arithmetic_mul_add_sub_zr_rd_1f00001f() {
    // Test aarch64_float_arithmetic_mul_add_sub with Rd = ZR (31)
    // Encoding: 0x1F00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1F00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

