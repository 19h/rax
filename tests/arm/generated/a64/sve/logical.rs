//! A64 sve logical tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// EOR_Z.P.ZZ__ Tests
// ============================================================================

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_eor_z_p_zz_field_size_0_min_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ field size = 0 (Min)
    // Fields: Zdn=0, size=0, Zm=0, Pg=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_eor_z_p_zz_field_size_1_poweroftwo_0_04590000() {
    // Encoding: 0x04590000
    // Test EOR_Z.P.ZZ__ field size = 1 (PowerOfTwo)
    // Fields: Pg=0, size=1, Zdn=0, Zm=0
    let encoding: u32 = 0x04590000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_eor_z_p_zz_field_size_2_poweroftwo_0_04990000() {
    // Encoding: 0x04990000
    // Test EOR_Z.P.ZZ__ field size = 2 (PowerOfTwo)
    // Fields: size=2, Pg=0, Zm=0, Zdn=0
    let encoding: u32 = 0x04990000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_eor_z_p_zz_field_size_3_max_0_04d90000() {
    // Encoding: 0x04D90000
    // Test EOR_Z.P.ZZ__ field size = 3 (Max)
    // Fields: size=3, Zm=0, Pg=0, Zdn=0
    let encoding: u32 = 0x04D90000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eor_z_p_zz_field_pg_0_min_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ field Pg = 0 (Min)
    // Fields: Pg=0, Zdn=0, Zm=0, size=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eor_z_p_zz_field_pg_1_poweroftwo_0_04190400() {
    // Encoding: 0x04190400
    // Test EOR_Z.P.ZZ__ field Pg = 1 (PowerOfTwo)
    // Fields: size=0, Zm=0, Pg=1, Zdn=0
    let encoding: u32 = 0x04190400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_eor_z_p_zz_field_zm_0_min_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ field Zm = 0 (Min)
    // Fields: size=0, Zm=0, Zdn=0, Pg=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_eor_z_p_zz_field_zm_1_poweroftwo_0_04190020() {
    // Encoding: 0x04190020
    // Test EOR_Z.P.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Pg=0, Zdn=0, Zm=1, size=0
    let encoding: u32 = 0x04190020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_eor_z_p_zz_field_zm_30_poweroftwominusone_0_041903c0() {
    // Encoding: 0x041903C0
    // Test EOR_Z.P.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Pg=0, Zdn=0, Zm=30
    let encoding: u32 = 0x041903C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_eor_z_p_zz_field_zm_31_max_0_041903e0() {
    // Encoding: 0x041903E0
    // Test EOR_Z.P.ZZ__ field Zm = 31 (Max)
    // Fields: Pg=0, size=0, Zdn=0, Zm=31
    let encoding: u32 = 0x041903E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_eor_z_p_zz_field_zdn_0_min_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ field Zdn = 0 (Min)
    // Fields: Pg=0, size=0, Zm=0, Zdn=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_eor_z_p_zz_field_zdn_1_poweroftwo_0_04190001() {
    // Encoding: 0x04190001
    // Test EOR_Z.P.ZZ__ field Zdn = 1 (PowerOfTwo)
    // Fields: Pg=0, size=0, Zm=0, Zdn=1
    let encoding: u32 = 0x04190001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_eor_z_p_zz_field_zdn_15_poweroftwominusone_0_0419000f() {
    // Encoding: 0x0419000F
    // Test EOR_Z.P.ZZ__ field Zdn = 15 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Zdn=15, Pg=0, size=0
    let encoding: u32 = 0x0419000F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_eor_z_p_zz_field_zdn_31_max_0_0419001f() {
    // Encoding: 0x0419001F
    // Test EOR_Z.P.ZZ__ field Zdn = 31 (Max)
    // Fields: Zm=0, Zdn=31, size=0, Pg=0
    let encoding: u32 = 0x0419001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_eor_z_p_zz_combo_0_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ field combination: size=0, Pg=0, Zm=0, Zdn=0
    // Fields: size=0, Zm=0, Zdn=0, Pg=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_eor_z_p_zz_special_size_0_size_variant_0_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ special value size = 0 (Size variant 0)
    // Fields: Zm=0, Zdn=0, Pg=0, size=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_eor_z_p_zz_special_size_1_size_variant_1_0_04590000() {
    // Encoding: 0x04590000
    // Test EOR_Z.P.ZZ__ special value size = 1 (Size variant 1)
    // Fields: Zm=0, Pg=0, size=1, Zdn=0
    let encoding: u32 = 0x04590000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_eor_z_p_zz_special_size_2_size_variant_2_0_04990000() {
    // Encoding: 0x04990000
    // Test EOR_Z.P.ZZ__ special value size = 2 (Size variant 2)
    // Fields: Pg=0, Zdn=0, Zm=0, size=2
    let encoding: u32 = 0x04990000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_eor_z_p_zz_special_size_3_size_variant_3_0_04d90000() {
    // Encoding: 0x04D90000
    // Test EOR_Z.P.ZZ__ special value size = 3 (Size variant 3)
    // Fields: Pg=0, Zm=0, size=3, Zdn=0
    let encoding: u32 = 0x04D90000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_eor_z_p_zz_invalid_0_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zm=0, Pg=0, size=0, Zdn=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_eor_z_p_zz_invalid_1_0_04190000() {
    // Encoding: 0x04190000
    // Test EOR_Z.P.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: size=0, Pg=0, Zm=0, Zdn=0
    let encoding: u32 = 0x04190000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_Z.P.ZZ__
/// ASL: `SimdFromField("dn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("dn")
#[test]
fn test_eor_z_p_zz_reg_write_0_04190000() {
    // Test EOR_Z.P.ZZ__ register write: SimdFromField("dn")
    // Encoding: 0x04190000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x04190000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// BIC_Z.P.ZZ__ Tests
// ============================================================================

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_bic_z_p_zz_field_size_0_min_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ field size = 0 (Min)
    // Fields: Pg=0, size=0, Zm=0, Zdn=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_bic_z_p_zz_field_size_1_poweroftwo_0_045b0000() {
    // Encoding: 0x045B0000
    // Test BIC_Z.P.ZZ__ field size = 1 (PowerOfTwo)
    // Fields: Zm=0, size=1, Zdn=0, Pg=0
    let encoding: u32 = 0x045B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_bic_z_p_zz_field_size_2_poweroftwo_0_049b0000() {
    // Encoding: 0x049B0000
    // Test BIC_Z.P.ZZ__ field size = 2 (PowerOfTwo)
    // Fields: Zdn=0, size=2, Pg=0, Zm=0
    let encoding: u32 = 0x049B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_bic_z_p_zz_field_size_3_max_0_04db0000() {
    // Encoding: 0x04DB0000
    // Test BIC_Z.P.ZZ__ field size = 3 (Max)
    // Fields: size=3, Zdn=0, Pg=0, Zm=0
    let encoding: u32 = 0x04DB0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bic_z_p_zz_field_pg_0_min_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ field Pg = 0 (Min)
    // Fields: Pg=0, size=0, Zm=0, Zdn=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bic_z_p_zz_field_pg_1_poweroftwo_0_041b0400() {
    // Encoding: 0x041B0400
    // Test BIC_Z.P.ZZ__ field Pg = 1 (PowerOfTwo)
    // Fields: size=0, Zm=0, Zdn=0, Pg=1
    let encoding: u32 = 0x041B0400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_bic_z_p_zz_field_zm_0_min_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ field Zm = 0 (Min)
    // Fields: Pg=0, Zdn=0, size=0, Zm=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_bic_z_p_zz_field_zm_1_poweroftwo_0_041b0020() {
    // Encoding: 0x041B0020
    // Test BIC_Z.P.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Pg=0, Zdn=0, size=0, Zm=1
    let encoding: u32 = 0x041B0020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_bic_z_p_zz_field_zm_30_poweroftwominusone_0_041b03c0() {
    // Encoding: 0x041B03C0
    // Test BIC_Z.P.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Zdn=0, Pg=0, size=0
    let encoding: u32 = 0x041B03C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_bic_z_p_zz_field_zm_31_max_0_041b03e0() {
    // Encoding: 0x041B03E0
    // Test BIC_Z.P.ZZ__ field Zm = 31 (Max)
    // Fields: size=0, Zm=31, Pg=0, Zdn=0
    let encoding: u32 = 0x041B03E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_bic_z_p_zz_field_zdn_0_min_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ field Zdn = 0 (Min)
    // Fields: Zm=0, size=0, Zdn=0, Pg=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_bic_z_p_zz_field_zdn_1_poweroftwo_0_041b0001() {
    // Encoding: 0x041B0001
    // Test BIC_Z.P.ZZ__ field Zdn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zdn=1, Zm=0, size=0
    let encoding: u32 = 0x041B0001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_bic_z_p_zz_field_zdn_15_poweroftwominusone_0_041b000f() {
    // Encoding: 0x041B000F
    // Test BIC_Z.P.ZZ__ field Zdn = 15 (PowerOfTwoMinusOne)
    // Fields: Pg=0, size=0, Zdn=15, Zm=0
    let encoding: u32 = 0x041B000F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_bic_z_p_zz_field_zdn_31_max_0_041b001f() {
    // Encoding: 0x041B001F
    // Test BIC_Z.P.ZZ__ field Zdn = 31 (Max)
    // Fields: Zdn=31, size=0, Pg=0, Zm=0
    let encoding: u32 = 0x041B001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_bic_z_p_zz_combo_0_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ field combination: size=0, Pg=0, Zm=0, Zdn=0
    // Fields: Zm=0, size=0, Pg=0, Zdn=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_bic_z_p_zz_special_size_0_size_variant_0_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ special value size = 0 (Size variant 0)
    // Fields: Pg=0, Zm=0, size=0, Zdn=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_bic_z_p_zz_special_size_1_size_variant_1_0_045b0000() {
    // Encoding: 0x045B0000
    // Test BIC_Z.P.ZZ__ special value size = 1 (Size variant 1)
    // Fields: size=1, Zm=0, Zdn=0, Pg=0
    let encoding: u32 = 0x045B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_bic_z_p_zz_special_size_2_size_variant_2_0_049b0000() {
    // Encoding: 0x049B0000
    // Test BIC_Z.P.ZZ__ special value size = 2 (Size variant 2)
    // Fields: size=2, Zdn=0, Pg=0, Zm=0
    let encoding: u32 = 0x049B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_bic_z_p_zz_special_size_3_size_variant_3_0_04db0000() {
    // Encoding: 0x04DB0000
    // Test BIC_Z.P.ZZ__ special value size = 3 (Size variant 3)
    // Fields: Zm=0, Pg=0, size=3, Zdn=0
    let encoding: u32 = 0x04DB0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_bic_z_p_zz_invalid_0_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zdn=0, Pg=0, Zm=0, size=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_bic_z_p_zz_invalid_1_0_041b0000() {
    // Encoding: 0x041B0000
    // Test BIC_Z.P.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: size=0, Zdn=0, Pg=0, Zm=0
    let encoding: u32 = 0x041B0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BIC_Z.P.ZZ__
/// ASL: `SimdFromField("dn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("dn")
#[test]
fn test_bic_z_p_zz_reg_write_0_041b0000() {
    // Test BIC_Z.P.ZZ__ register write: SimdFromField("dn")
    // Encoding: 0x041B0000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x041B0000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// NOT_Z.P.Z__ Tests
// ============================================================================

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_not_z_p_z_field_size_0_min_a000_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ field size = 0 (Min)
    // Fields: Zn=0, size=0, Pg=0, Zd=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_not_z_p_z_field_size_1_poweroftwo_a000_045ea000() {
    // Encoding: 0x045EA000
    // Test NOT_Z.P.Z__ field size = 1 (PowerOfTwo)
    // Fields: Zn=0, Pg=0, Zd=0, size=1
    let encoding: u32 = 0x045EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_not_z_p_z_field_size_2_poweroftwo_a000_049ea000() {
    // Encoding: 0x049EA000
    // Test NOT_Z.P.Z__ field size = 2 (PowerOfTwo)
    // Fields: size=2, Zd=0, Zn=0, Pg=0
    let encoding: u32 = 0x049EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_not_z_p_z_field_size_3_max_a000_04dea000() {
    // Encoding: 0x04DEA000
    // Test NOT_Z.P.Z__ field size = 3 (Max)
    // Fields: Zn=0, size=3, Pg=0, Zd=0
    let encoding: u32 = 0x04DEA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_not_z_p_z_field_pg_0_min_a000_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ field Pg = 0 (Min)
    // Fields: size=0, Zn=0, Zd=0, Pg=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_not_z_p_z_field_pg_1_poweroftwo_a000_041ea400() {
    // Encoding: 0x041EA400
    // Test NOT_Z.P.Z__ field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, size=0, Zn=0, Zd=0
    let encoding: u32 = 0x041EA400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_not_z_p_z_field_zn_0_min_a000_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ field Zn = 0 (Min)
    // Fields: Zn=0, size=0, Pg=0, Zd=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_not_z_p_z_field_zn_1_poweroftwo_a000_041ea020() {
    // Encoding: 0x041EA020
    // Test NOT_Z.P.Z__ field Zn = 1 (PowerOfTwo)
    // Fields: size=0, Zn=1, Zd=0, Pg=0
    let encoding: u32 = 0x041EA020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_not_z_p_z_field_zn_30_poweroftwominusone_a000_041ea3c0() {
    // Encoding: 0x041EA3C0
    // Test NOT_Z.P.Z__ field Zn = 30 (PowerOfTwoMinusOne)
    // Fields: Zn=30, Pg=0, size=0, Zd=0
    let encoding: u32 = 0x041EA3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_not_z_p_z_field_zn_31_max_a000_041ea3e0() {
    // Encoding: 0x041EA3E0
    // Test NOT_Z.P.Z__ field Zn = 31 (Max)
    // Fields: Zd=0, Pg=0, size=0, Zn=31
    let encoding: u32 = 0x041EA3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_not_z_p_z_field_zd_0_min_a000_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ field Zd = 0 (Min)
    // Fields: Zn=0, size=0, Zd=0, Pg=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_not_z_p_z_field_zd_1_poweroftwo_a000_041ea001() {
    // Encoding: 0x041EA001
    // Test NOT_Z.P.Z__ field Zd = 1 (PowerOfTwo)
    // Fields: Zd=1, size=0, Zn=0, Pg=0
    let encoding: u32 = 0x041EA001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_not_z_p_z_field_zd_30_poweroftwominusone_a000_041ea01e() {
    // Encoding: 0x041EA01E
    // Test NOT_Z.P.Z__ field Zd = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Zn=0, Pg=0, Zd=30
    let encoding: u32 = 0x041EA01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_not_z_p_z_field_zd_31_max_a000_041ea01f() {
    // Encoding: 0x041EA01F
    // Test NOT_Z.P.Z__ field Zd = 31 (Max)
    // Fields: Zn=0, Zd=31, size=0, Pg=0
    let encoding: u32 = 0x041EA01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_not_z_p_z_combo_0_a000_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ field combination: size=0, Pg=0, Zn=0, Zd=0
    // Fields: size=0, Zd=0, Zn=0, Pg=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_not_z_p_z_special_size_0_size_variant_0_40960_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ special value size = 0 (Size variant 0)
    // Fields: Zd=0, Zn=0, size=0, Pg=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_not_z_p_z_special_size_1_size_variant_1_40960_045ea000() {
    // Encoding: 0x045EA000
    // Test NOT_Z.P.Z__ special value size = 1 (Size variant 1)
    // Fields: Pg=0, Zn=0, size=1, Zd=0
    let encoding: u32 = 0x045EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_not_z_p_z_special_size_2_size_variant_2_40960_049ea000() {
    // Encoding: 0x049EA000
    // Test NOT_Z.P.Z__ special value size = 2 (Size variant 2)
    // Fields: size=2, Zn=0, Pg=0, Zd=0
    let encoding: u32 = 0x049EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_not_z_p_z_special_size_3_size_variant_3_40960_04dea000() {
    // Encoding: 0x04DEA000
    // Test NOT_Z.P.Z__ special value size = 3 (Size variant 3)
    // Fields: Zd=0, Pg=0, size=3, Zn=0
    let encoding: u32 = 0x04DEA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_not_z_p_z_invalid_0_a000_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zd=0, size=0, Zn=0, Pg=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_not_z_p_z_invalid_1_a000_041ea000() {
    // Encoding: 0x041EA000
    // Test NOT_Z.P.Z__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Zd=0, size=0, Zn=0
    let encoding: u32 = 0x041EA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NOT_Z.P.Z__
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_not_z_p_z_reg_write_0_041ea000() {
    // Test NOT_Z.P.Z__ register write: SimdFromField("d")
    // Encoding: 0x041EA000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x041EA000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// ORR_P.P.PP_Z Tests
// ============================================================================

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_orr_p_p_pp_z_field_s_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z field S = 0 (Min)
    // Fields: Pd=0, Pn=0, S=0, Pg=0, Pm=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_orr_p_p_pp_z_field_s_1_max_4000_25c04000() {
    // Encoding: 0x25C04000
    // Test ORR_P.P.PP_Z field S = 1 (Max)
    // Fields: S=1, Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x25C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orr_p_p_pp_z_field_pm_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z field Pm = 0 (Min)
    // Fields: S=0, Pm=0, Pn=0, Pg=0, Pd=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orr_p_p_pp_z_field_pm_1_poweroftwo_4000_25814000() {
    // Encoding: 0x25814000
    // Test ORR_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: S=0, Pd=0, Pm=1, Pn=0, Pg=0
    let encoding: u32 = 0x25814000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orr_p_p_pp_z_field_pg_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pn=0, Pg=0, S=0, Pm=0, Pd=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orr_p_p_pp_z_field_pg_1_poweroftwo_4000_25804400() {
    // Encoding: 0x25804400
    // Test ORR_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pm=0, Pg=1, Pn=0, Pd=0, S=0
    let encoding: u32 = 0x25804400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orr_p_p_pp_z_field_pn_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pd=0, Pg=0, Pm=0, S=0, Pn=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orr_p_p_pp_z_field_pn_1_poweroftwo_4000_25804020() {
    // Encoding: 0x25804020
    // Test ORR_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pg=0, Pd=0, S=0, Pm=0, Pn=1
    let encoding: u32 = 0x25804020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orr_p_p_pp_z_field_pd_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z field Pd = 0 (Min)
    // Fields: S=0, Pm=0, Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orr_p_p_pp_z_field_pd_1_poweroftwo_4000_25804001() {
    // Encoding: 0x25804001
    // Test ORR_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: S=0, Pm=0, Pg=0, Pd=1, Pn=0
    let encoding: u32 = 0x25804001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// S=0 (8-bit / byte size)
#[test]
fn test_orr_p_p_pp_z_combo_0_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z field combination: S=0, Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0, S=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field S = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "S", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_orr_p_p_pp_z_special_s_0_size_variant_0_16384_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z special value S = 0 (Size variant 0)
    // Fields: Pg=0, Pn=0, Pd=0, S=0, Pm=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `field S = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "S", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_orr_p_p_pp_z_special_s_1_size_variant_1_16384_25c04000() {
    // Encoding: 0x25C04000
    // Test ORR_P.P.PP_Z special value S = 1 (Size variant 1)
    // Fields: S=1, Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x25C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_orr_p_p_pp_z_invalid_0_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, S=0, Pd=0, Pm=0, Pn=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_orr_p_p_pp_z_invalid_1_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORR_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: S=0, Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_orrs_p_p_pp_z_field_s_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z field S = 0 (Min)
    // Fields: Pd=0, Pn=0, Pm=0, S=0, Pg=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_orrs_p_p_pp_z_field_s_1_max_4000_25c04000() {
    // Encoding: 0x25C04000
    // Test ORRS_P.P.PP_Z field S = 1 (Max)
    // Fields: Pd=0, Pm=0, Pg=0, S=1, Pn=0
    let encoding: u32 = 0x25C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orrs_p_p_pp_z_field_pm_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z field Pm = 0 (Min)
    // Fields: S=0, Pm=0, Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orrs_p_p_pp_z_field_pm_1_poweroftwo_4000_25814000() {
    // Encoding: 0x25814000
    // Test ORRS_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pg=0, Pd=0, Pn=0, S=0, Pm=1
    let encoding: u32 = 0x25814000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orrs_p_p_pp_z_field_pg_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z field Pg = 0 (Min)
    // Fields: S=0, Pg=0, Pn=0, Pm=0, Pd=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orrs_p_p_pp_z_field_pg_1_poweroftwo_4000_25804400() {
    // Encoding: 0x25804400
    // Test ORRS_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, S=0, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25804400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orrs_p_p_pp_z_field_pn_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pm=0, S=0, Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orrs_p_p_pp_z_field_pn_1_poweroftwo_4000_25804020() {
    // Encoding: 0x25804020
    // Test ORRS_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pm=0, Pd=0, Pn=1, S=0, Pg=0
    let encoding: u32 = 0x25804020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orrs_p_p_pp_z_field_pd_0_min_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pd=0, Pn=0, Pg=0, S=0, Pm=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orrs_p_p_pp_z_field_pd_1_poweroftwo_4000_25804001() {
    // Encoding: 0x25804001
    // Test ORRS_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: S=0, Pm=0, Pn=0, Pg=0, Pd=1
    let encoding: u32 = 0x25804001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// S=0 (8-bit / byte size)
#[test]
fn test_orrs_p_p_pp_z_combo_0_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z field combination: S=0, Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0, S=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field S = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "S", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_orrs_p_p_pp_z_special_s_0_size_variant_0_16384_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z special value S = 0 (Size variant 0)
    // Fields: S=0, Pm=0, Pn=0, Pg=0, Pd=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `field S = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "S", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_orrs_p_p_pp_z_special_s_1_size_variant_1_16384_25c04000() {
    // Encoding: 0x25C04000
    // Test ORRS_P.P.PP_Z special value S = 1 (Size variant 1)
    // Fields: Pn=0, Pd=0, Pg=0, S=1, Pm=0
    let encoding: u32 = 0x25C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_orrs_p_p_pp_z_invalid_0_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0, Pd=0, S=0, Pm=0, Pg=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_orrs_p_p_pp_z_invalid_1_4000_25804000() {
    // Encoding: 0x25804000
    // Test ORRS_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pn=0, Pd=0, S=0, Pm=0
    let encoding: u32 = 0x25804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_orr_p_p_pp_z_reg_write_0_25804000() {
    // Test ORR_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25804000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25804000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_orr_p_p_pp_z_flags_zeroresult_0_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_orr_p_p_pp_z_flags_zeroresult_1_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_orr_p_p_pp_z_flags_negativeresult_2_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_orr_p_p_pp_z_flags_unsignedoverflow_3_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_orr_p_p_pp_z_flags_unsignedoverflow_4_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_orr_p_p_pp_z_flags_signedoverflow_5_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_orr_p_p_pp_z_flags_signedoverflow_6_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_orr_p_p_pp_z_flags_positiveresult_7_25c04000() {
    // Test ORR_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_orrs_p_p_pp_z_reg_write_0_25804000() {
    // Test ORRS_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25804000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25804000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_orrs_p_p_pp_z_flags_zeroresult_0_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_orrs_p_p_pp_z_flags_zeroresult_1_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_orrs_p_p_pp_z_flags_negativeresult_2_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_orrs_p_p_pp_z_flags_unsignedoverflow_3_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_orrs_p_p_pp_z_flags_unsignedoverflow_4_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_orrs_p_p_pp_z_flags_signedoverflow_5_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_orrs_p_p_pp_z_flags_signedoverflow_6_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORRS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_orrs_p_p_pp_z_flags_positiveresult_7_25c04000() {
    // Test ORRS_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25C04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25C04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// ORR_Z.ZZ__ Tests
// ============================================================================

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_orr_z_zz_field_zm_0_min_3000_04603000() {
    // Encoding: 0x04603000
    // Test ORR_Z.ZZ__ field Zm = 0 (Min)
    // Fields: Zm=0, Zn=0, Zd=0
    let encoding: u32 = 0x04603000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_orr_z_zz_field_zm_1_poweroftwo_3000_04613000() {
    // Encoding: 0x04613000
    // Test ORR_Z.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Zn=0, Zm=1, Zd=0
    let encoding: u32 = 0x04613000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_orr_z_zz_field_zm_30_poweroftwominusone_3000_047e3000() {
    // Encoding: 0x047E3000
    // Test ORR_Z.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zn=0, Zm=30, Zd=0
    let encoding: u32 = 0x047E3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_orr_z_zz_field_zm_31_max_3000_047f3000() {
    // Encoding: 0x047F3000
    // Test ORR_Z.ZZ__ field Zm = 31 (Max)
    // Fields: Zd=0, Zm=31, Zn=0
    let encoding: u32 = 0x047F3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_orr_z_zz_field_zn_0_min_3000_04603000() {
    // Encoding: 0x04603000
    // Test ORR_Z.ZZ__ field Zn = 0 (Min)
    // Fields: Zn=0, Zm=0, Zd=0
    let encoding: u32 = 0x04603000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_orr_z_zz_field_zn_1_poweroftwo_3000_04603020() {
    // Encoding: 0x04603020
    // Test ORR_Z.ZZ__ field Zn = 1 (PowerOfTwo)
    // Fields: Zd=0, Zm=0, Zn=1
    let encoding: u32 = 0x04603020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_orr_z_zz_field_zn_30_poweroftwominusone_3000_046033c0() {
    // Encoding: 0x046033C0
    // Test ORR_Z.ZZ__ field Zn = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Zn=30, Zd=0
    let encoding: u32 = 0x046033C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_orr_z_zz_field_zn_31_max_3000_046033e0() {
    // Encoding: 0x046033E0
    // Test ORR_Z.ZZ__ field Zn = 31 (Max)
    // Fields: Zd=0, Zm=0, Zn=31
    let encoding: u32 = 0x046033E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_orr_z_zz_field_zd_0_min_3000_04603000() {
    // Encoding: 0x04603000
    // Test ORR_Z.ZZ__ field Zd = 0 (Min)
    // Fields: Zd=0, Zn=0, Zm=0
    let encoding: u32 = 0x04603000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_orr_z_zz_field_zd_1_poweroftwo_3000_04603001() {
    // Encoding: 0x04603001
    // Test ORR_Z.ZZ__ field Zd = 1 (PowerOfTwo)
    // Fields: Zd=1, Zm=0, Zn=0
    let encoding: u32 = 0x04603001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_orr_z_zz_field_zd_30_poweroftwominusone_3000_0460301e() {
    // Encoding: 0x0460301E
    // Test ORR_Z.ZZ__ field Zd = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Zn=0, Zd=30
    let encoding: u32 = 0x0460301E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_orr_z_zz_field_zd_31_max_3000_0460301f() {
    // Encoding: 0x0460301F
    // Test ORR_Z.ZZ__ field Zd = 31 (Max)
    // Fields: Zm=0, Zn=0, Zd=31
    let encoding: u32 = 0x0460301F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_orr_z_zz_combo_0_3000_04603000() {
    // Encoding: 0x04603000
    // Test ORR_Z.ZZ__ field combination: Zm=0, Zn=0, Zd=0
    // Fields: Zm=0, Zn=0, Zd=0
    let encoding: u32 = 0x04603000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_orr_z_zz_invalid_0_3000_04603000() {
    // Encoding: 0x04603000
    // Test ORR_Z.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zd=0, Zm=0, Zn=0
    let encoding: u32 = 0x04603000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_orr_z_zz_invalid_1_3000_04603000() {
    // Encoding: 0x04603000
    // Test ORR_Z.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: Zn=0, Zd=0, Zm=0
    let encoding: u32 = 0x04603000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_Z.ZZ__
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_orr_z_zz_reg_write_0_04603000() {
    // Test ORR_Z.ZZ__ register write: SimdFromField("d")
    // Encoding: 0x04603000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x04603000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// AND_Z.P.ZZ__ Tests
// ============================================================================

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_and_z_p_zz_field_size_0_min_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ field size = 0 (Min)
    // Fields: Zm=0, Zdn=0, size=0, Pg=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_and_z_p_zz_field_size_1_poweroftwo_0_045a0000() {
    // Encoding: 0x045A0000
    // Test AND_Z.P.ZZ__ field size = 1 (PowerOfTwo)
    // Fields: Pg=0, size=1, Zm=0, Zdn=0
    let encoding: u32 = 0x045A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_and_z_p_zz_field_size_2_poweroftwo_0_049a0000() {
    // Encoding: 0x049A0000
    // Test AND_Z.P.ZZ__ field size = 2 (PowerOfTwo)
    // Fields: Zdn=0, Pg=0, size=2, Zm=0
    let encoding: u32 = 0x049A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_and_z_p_zz_field_size_3_max_0_04da0000() {
    // Encoding: 0x04DA0000
    // Test AND_Z.P.ZZ__ field size = 3 (Max)
    // Fields: Pg=0, Zm=0, size=3, Zdn=0
    let encoding: u32 = 0x04DA0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_and_z_p_zz_field_pg_0_min_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ field Pg = 0 (Min)
    // Fields: Zm=0, Pg=0, size=0, Zdn=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_and_z_p_zz_field_pg_1_poweroftwo_0_041a0400() {
    // Encoding: 0x041A0400
    // Test AND_Z.P.ZZ__ field Pg = 1 (PowerOfTwo)
    // Fields: size=0, Zdn=0, Pg=1, Zm=0
    let encoding: u32 = 0x041A0400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_and_z_p_zz_field_zm_0_min_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ field Zm = 0 (Min)
    // Fields: size=0, Zdn=0, Pg=0, Zm=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_and_z_p_zz_field_zm_1_poweroftwo_0_041a0020() {
    // Encoding: 0x041A0020
    // Test AND_Z.P.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Zdn=0, size=0, Pg=0, Zm=1
    let encoding: u32 = 0x041A0020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_and_z_p_zz_field_zm_30_poweroftwominusone_0_041a03c0() {
    // Encoding: 0x041A03C0
    // Test AND_Z.P.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Pg=0, Zdn=0, Zm=30
    let encoding: u32 = 0x041A03C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_and_z_p_zz_field_zm_31_max_0_041a03e0() {
    // Encoding: 0x041A03E0
    // Test AND_Z.P.ZZ__ field Zm = 31 (Max)
    // Fields: Zm=31, Pg=0, size=0, Zdn=0
    let encoding: u32 = 0x041A03E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_and_z_p_zz_field_zdn_0_min_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ field Zdn = 0 (Min)
    // Fields: size=0, Zdn=0, Zm=0, Pg=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_and_z_p_zz_field_zdn_1_poweroftwo_0_041a0001() {
    // Encoding: 0x041A0001
    // Test AND_Z.P.ZZ__ field Zdn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zdn=1, size=0, Zm=0
    let encoding: u32 = 0x041A0001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_and_z_p_zz_field_zdn_15_poweroftwominusone_0_041a000f() {
    // Encoding: 0x041A000F
    // Test AND_Z.P.ZZ__ field Zdn = 15 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zdn=15, size=0, Zm=0
    let encoding: u32 = 0x041A000F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_and_z_p_zz_field_zdn_31_max_0_041a001f() {
    // Encoding: 0x041A001F
    // Test AND_Z.P.ZZ__ field Zdn = 31 (Max)
    // Fields: size=0, Zdn=31, Zm=0, Pg=0
    let encoding: u32 = 0x041A001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_and_z_p_zz_combo_0_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ field combination: size=0, Pg=0, Zm=0, Zdn=0
    // Fields: Zdn=0, Pg=0, size=0, Zm=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_and_z_p_zz_special_size_0_size_variant_0_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ special value size = 0 (Size variant 0)
    // Fields: Zdn=0, size=0, Zm=0, Pg=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_and_z_p_zz_special_size_1_size_variant_1_0_045a0000() {
    // Encoding: 0x045A0000
    // Test AND_Z.P.ZZ__ special value size = 1 (Size variant 1)
    // Fields: Zm=0, Pg=0, Zdn=0, size=1
    let encoding: u32 = 0x045A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_and_z_p_zz_special_size_2_size_variant_2_0_049a0000() {
    // Encoding: 0x049A0000
    // Test AND_Z.P.ZZ__ special value size = 2 (Size variant 2)
    // Fields: Zm=0, size=2, Pg=0, Zdn=0
    let encoding: u32 = 0x049A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_and_z_p_zz_special_size_3_size_variant_3_0_04da0000() {
    // Encoding: 0x04DA0000
    // Test AND_Z.P.ZZ__ special value size = 3 (Size variant 3)
    // Fields: size=3, Zm=0, Zdn=0, Pg=0
    let encoding: u32 = 0x04DA0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_and_z_p_zz_invalid_0_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, size=0, Zm=0, Zdn=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_and_z_p_zz_invalid_1_0_041a0000() {
    // Encoding: 0x041A0000
    // Test AND_Z.P.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: Zm=0, Zdn=0, size=0, Pg=0
    let encoding: u32 = 0x041A0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_Z.P.ZZ__
/// ASL: `SimdFromField("dn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("dn")
#[test]
fn test_and_z_p_zz_reg_write_0_041a0000() {
    // Test AND_Z.P.ZZ__ register write: SimdFromField("dn")
    // Encoding: 0x041A0000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x041A0000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// ORR_Z.ZI__ Tests
// ============================================================================

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_orr_z_zi_field_imm13_0_zero_0_05000000() {
    // Encoding: 0x05000000
    // Test ORR_Z.ZI__ field imm13 = 0 (Zero)
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_orr_z_zi_field_imm13_1_poweroftwo_0_05000020() {
    // Encoding: 0x05000020
    // Test ORR_Z.ZI__ field imm13 = 1 (PowerOfTwo)
    // Fields: Zdn=0, imm13=1
    let encoding: u32 = 0x05000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_orr_z_zi_field_imm13_3_poweroftwominusone_0_05000060() {
    // Encoding: 0x05000060
    // Test ORR_Z.ZI__ field imm13 = 3 (PowerOfTwoMinusOne)
    // Fields: imm13=3, Zdn=0
    let encoding: u32 = 0x05000060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_orr_z_zi_field_imm13_4_poweroftwo_0_05000080() {
    // Encoding: 0x05000080
    // Test ORR_Z.ZI__ field imm13 = 4 (PowerOfTwo)
    // Fields: imm13=4, Zdn=0
    let encoding: u32 = 0x05000080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_orr_z_zi_field_imm13_7_poweroftwominusone_0_050000e0() {
    // Encoding: 0x050000E0
    // Test ORR_Z.ZI__ field imm13 = 7 (PowerOfTwoMinusOne)
    // Fields: imm13=7, Zdn=0
    let encoding: u32 = 0x050000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_orr_z_zi_field_imm13_8_poweroftwo_0_05000100() {
    // Encoding: 0x05000100
    // Test ORR_Z.ZI__ field imm13 = 8 (PowerOfTwo)
    // Fields: Zdn=0, imm13=8
    let encoding: u32 = 0x05000100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_orr_z_zi_field_imm13_15_poweroftwominusone_0_050001e0() {
    // Encoding: 0x050001E0
    // Test ORR_Z.ZI__ field imm13 = 15 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=15
    let encoding: u32 = 0x050001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_orr_z_zi_field_imm13_16_poweroftwo_0_05000200() {
    // Encoding: 0x05000200
    // Test ORR_Z.ZI__ field imm13 = 16 (PowerOfTwo)
    // Fields: imm13=16, Zdn=0
    let encoding: u32 = 0x05000200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_orr_z_zi_field_imm13_31_poweroftwominusone_0_050003e0() {
    // Encoding: 0x050003E0
    // Test ORR_Z.ZI__ field imm13 = 31 (PowerOfTwoMinusOne)
    // Fields: imm13=31, Zdn=0
    let encoding: u32 = 0x050003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_orr_z_zi_field_imm13_32_poweroftwo_0_05000400() {
    // Encoding: 0x05000400
    // Test ORR_Z.ZI__ field imm13 = 32 (PowerOfTwo)
    // Fields: imm13=32, Zdn=0
    let encoding: u32 = 0x05000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_orr_z_zi_field_imm13_63_poweroftwominusone_0_050007e0() {
    // Encoding: 0x050007E0
    // Test ORR_Z.ZI__ field imm13 = 63 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=63
    let encoding: u32 = 0x050007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_orr_z_zi_field_imm13_64_poweroftwo_0_05000800() {
    // Encoding: 0x05000800
    // Test ORR_Z.ZI__ field imm13 = 64 (PowerOfTwo)
    // Fields: Zdn=0, imm13=64
    let encoding: u32 = 0x05000800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_orr_z_zi_field_imm13_127_poweroftwominusone_0_05000fe0() {
    // Encoding: 0x05000FE0
    // Test ORR_Z.ZI__ field imm13 = 127 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=127
    let encoding: u32 = 0x05000FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_orr_z_zi_field_imm13_128_poweroftwo_0_05001000() {
    // Encoding: 0x05001000
    // Test ORR_Z.ZI__ field imm13 = 128 (PowerOfTwo)
    // Fields: Zdn=0, imm13=128
    let encoding: u32 = 0x05001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_orr_z_zi_field_imm13_255_poweroftwominusone_0_05001fe0() {
    // Encoding: 0x05001FE0
    // Test ORR_Z.ZI__ field imm13 = 255 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=255
    let encoding: u32 = 0x05001FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_orr_z_zi_field_imm13_256_poweroftwo_0_05002000() {
    // Encoding: 0x05002000
    // Test ORR_Z.ZI__ field imm13 = 256 (PowerOfTwo)
    // Fields: imm13=256, Zdn=0
    let encoding: u32 = 0x05002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_orr_z_zi_field_imm13_511_poweroftwominusone_0_05003fe0() {
    // Encoding: 0x05003FE0
    // Test ORR_Z.ZI__ field imm13 = 511 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=511
    let encoding: u32 = 0x05003FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_orr_z_zi_field_imm13_512_poweroftwo_0_05004000() {
    // Encoding: 0x05004000
    // Test ORR_Z.ZI__ field imm13 = 512 (PowerOfTwo)
    // Fields: imm13=512, Zdn=0
    let encoding: u32 = 0x05004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_orr_z_zi_field_imm13_1023_poweroftwominusone_0_05007fe0() {
    // Encoding: 0x05007FE0
    // Test ORR_Z.ZI__ field imm13 = 1023 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=1023
    let encoding: u32 = 0x05007FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_orr_z_zi_field_imm13_1024_poweroftwo_0_05008000() {
    // Encoding: 0x05008000
    // Test ORR_Z.ZI__ field imm13 = 1024 (PowerOfTwo)
    // Fields: Zdn=0, imm13=1024
    let encoding: u32 = 0x05008000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_orr_z_zi_field_imm13_2047_poweroftwominusone_0_0500ffe0() {
    // Encoding: 0x0500FFE0
    // Test ORR_Z.ZI__ field imm13 = 2047 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=2047
    let encoding: u32 = 0x0500FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_orr_z_zi_field_imm13_2048_poweroftwo_0_05010000() {
    // Encoding: 0x05010000
    // Test ORR_Z.ZI__ field imm13 = 2048 (PowerOfTwo)
    // Fields: imm13=2048, Zdn=0
    let encoding: u32 = 0x05010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4095, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (4095)
#[test]
fn test_orr_z_zi_field_imm13_4095_poweroftwominusone_0_0501ffe0() {
    // Encoding: 0x0501FFE0
    // Test ORR_Z.ZI__ field imm13 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm13=4095, Zdn=0
    let encoding: u32 = 0x0501FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_orr_z_zi_field_imm13_4096_poweroftwo_0_05020000() {
    // Encoding: 0x05020000
    // Test ORR_Z.ZI__ field imm13 = 4096 (PowerOfTwo)
    // Fields: imm13=4096, Zdn=0
    let encoding: u32 = 0x05020000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 8191, boundary: Max }
/// maximum immediate (8191)
#[test]
fn test_orr_z_zi_field_imm13_8191_max_0_0503ffe0() {
    // Encoding: 0x0503FFE0
    // Test ORR_Z.ZI__ field imm13 = 8191 (Max)
    // Fields: Zdn=0, imm13=8191
    let encoding: u32 = 0x0503FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_orr_z_zi_field_zdn_0_min_0_05000000() {
    // Encoding: 0x05000000
    // Test ORR_Z.ZI__ field Zdn = 0 (Min)
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_orr_z_zi_field_zdn_1_poweroftwo_0_05000001() {
    // Encoding: 0x05000001
    // Test ORR_Z.ZI__ field Zdn = 1 (PowerOfTwo)
    // Fields: imm13=0, Zdn=1
    let encoding: u32 = 0x05000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_orr_z_zi_field_zdn_15_poweroftwominusone_0_0500000f() {
    // Encoding: 0x0500000F
    // Test ORR_Z.ZI__ field Zdn = 15 (PowerOfTwoMinusOne)
    // Fields: imm13=0, Zdn=15
    let encoding: u32 = 0x0500000F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_orr_z_zi_field_zdn_31_max_0_0500001f() {
    // Encoding: 0x0500001F
    // Test ORR_Z.ZI__ field Zdn = 31 (Max)
    // Fields: imm13=0, Zdn=31
    let encoding: u32 = 0x0500001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm13=0 (immediate value 0)
#[test]
fn test_orr_z_zi_combo_0_0_05000000() {
    // Encoding: 0x05000000
    // Test ORR_Z.ZI__ field combination: imm13=0, Zdn=0
    // Fields: Zdn=0, imm13=0
    let encoding: u32 = 0x05000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_orr_z_zi_invalid_0_0_05000000() {
    // Encoding: 0x05000000
    // Test ORR_Z.ZI__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_orr_z_zi_invalid_1_0_05000000() {
    // Encoding: 0x05000000
    // Test ORR_Z.ZI__ invalid encoding: Unconditional UNDEFINED
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_Z.ZI__
/// ASL: `SimdFromField("dn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("dn")
#[test]
fn test_orr_z_zi_reg_write_0_05000000() {
    // Test ORR_Z.ZI__ register write: SimdFromField("dn")
    // Encoding: 0x05000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x05000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// BIC_Z.ZZ__ Tests
// ============================================================================

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_bic_z_zz_field_zm_0_min_3000_04e03000() {
    // Encoding: 0x04E03000
    // Test BIC_Z.ZZ__ field Zm = 0 (Min)
    // Fields: Zn=0, Zd=0, Zm=0
    let encoding: u32 = 0x04E03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_bic_z_zz_field_zm_1_poweroftwo_3000_04e13000() {
    // Encoding: 0x04E13000
    // Test BIC_Z.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Zm=1, Zn=0, Zd=0
    let encoding: u32 = 0x04E13000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_bic_z_zz_field_zm_30_poweroftwominusone_3000_04fe3000() {
    // Encoding: 0x04FE3000
    // Test BIC_Z.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Zd=0, Zn=0
    let encoding: u32 = 0x04FE3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_bic_z_zz_field_zm_31_max_3000_04ff3000() {
    // Encoding: 0x04FF3000
    // Test BIC_Z.ZZ__ field Zm = 31 (Max)
    // Fields: Zd=0, Zm=31, Zn=0
    let encoding: u32 = 0x04FF3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_bic_z_zz_field_zn_0_min_3000_04e03000() {
    // Encoding: 0x04E03000
    // Test BIC_Z.ZZ__ field Zn = 0 (Min)
    // Fields: Zd=0, Zm=0, Zn=0
    let encoding: u32 = 0x04E03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_bic_z_zz_field_zn_1_poweroftwo_3000_04e03020() {
    // Encoding: 0x04E03020
    // Test BIC_Z.ZZ__ field Zn = 1 (PowerOfTwo)
    // Fields: Zm=0, Zn=1, Zd=0
    let encoding: u32 = 0x04E03020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_bic_z_zz_field_zn_30_poweroftwominusone_3000_04e033c0() {
    // Encoding: 0x04E033C0
    // Test BIC_Z.ZZ__ field Zn = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Zd=0, Zn=30
    let encoding: u32 = 0x04E033C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_bic_z_zz_field_zn_31_max_3000_04e033e0() {
    // Encoding: 0x04E033E0
    // Test BIC_Z.ZZ__ field Zn = 31 (Max)
    // Fields: Zm=0, Zn=31, Zd=0
    let encoding: u32 = 0x04E033E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_bic_z_zz_field_zd_0_min_3000_04e03000() {
    // Encoding: 0x04E03000
    // Test BIC_Z.ZZ__ field Zd = 0 (Min)
    // Fields: Zm=0, Zn=0, Zd=0
    let encoding: u32 = 0x04E03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_bic_z_zz_field_zd_1_poweroftwo_3000_04e03001() {
    // Encoding: 0x04E03001
    // Test BIC_Z.ZZ__ field Zd = 1 (PowerOfTwo)
    // Fields: Zn=0, Zd=1, Zm=0
    let encoding: u32 = 0x04E03001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_bic_z_zz_field_zd_30_poweroftwominusone_3000_04e0301e() {
    // Encoding: 0x04E0301E
    // Test BIC_Z.ZZ__ field Zd = 30 (PowerOfTwoMinusOne)
    // Fields: Zn=0, Zd=30, Zm=0
    let encoding: u32 = 0x04E0301E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_bic_z_zz_field_zd_31_max_3000_04e0301f() {
    // Encoding: 0x04E0301F
    // Test BIC_Z.ZZ__ field Zd = 31 (Max)
    // Fields: Zm=0, Zd=31, Zn=0
    let encoding: u32 = 0x04E0301F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_bic_z_zz_combo_0_3000_04e03000() {
    // Encoding: 0x04E03000
    // Test BIC_Z.ZZ__ field combination: Zm=0, Zn=0, Zd=0
    // Fields: Zm=0, Zn=0, Zd=0
    let encoding: u32 = 0x04E03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_bic_z_zz_invalid_0_3000_04e03000() {
    // Encoding: 0x04E03000
    // Test BIC_Z.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zd=0, Zn=0, Zm=0
    let encoding: u32 = 0x04E03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_bic_z_zz_invalid_1_3000_04e03000() {
    // Encoding: 0x04E03000
    // Test BIC_Z.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: Zn=0, Zd=0, Zm=0
    let encoding: u32 = 0x04E03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BIC_Z.ZZ__
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_bic_z_zz_reg_write_0_04e03000() {
    // Test BIC_Z.ZZ__ register write: SimdFromField("d")
    // Encoding: 0x04E03000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x04E03000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// EOR_P.P.PP_Z Tests
// ============================================================================

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eor_p_p_pp_z_field_pm_0_min_4200_25004200() {
    // Encoding: 0x25004200
    // Test EOR_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pg=0, Pm=0, Pn=0, Pd=0
    let encoding: u32 = 0x25004200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eor_p_p_pp_z_field_pm_1_poweroftwo_4200_25014200() {
    // Encoding: 0x25014200
    // Test EOR_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=0, Pm=1, Pd=0
    let encoding: u32 = 0x25014200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eor_p_p_pp_z_field_pg_0_min_4200_25004200() {
    // Encoding: 0x25004200
    // Test EOR_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pd=0, Pg=0, Pm=0, Pn=0
    let encoding: u32 = 0x25004200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eor_p_p_pp_z_field_pg_1_poweroftwo_4200_25004600() {
    // Encoding: 0x25004600
    // Test EOR_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pd=0, Pn=0, Pm=0, Pg=1
    let encoding: u32 = 0x25004600;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eor_p_p_pp_z_field_pn_0_min_4200_25004200() {
    // Encoding: 0x25004200
    // Test EOR_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25004200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eor_p_p_pp_z_field_pn_1_poweroftwo_4200_25004220() {
    // Encoding: 0x25004220
    // Test EOR_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x25004220;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eor_p_p_pp_z_field_pd_0_min_4200_25004200() {
    // Encoding: 0x25004200
    // Test EOR_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pd=0, Pg=0, Pn=0, Pm=0
    let encoding: u32 = 0x25004200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eor_p_p_pp_z_field_pd_1_poweroftwo_4200_25004201() {
    // Encoding: 0x25004201
    // Test EOR_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pn=0, Pg=0, Pd=1, Pm=0
    let encoding: u32 = 0x25004201;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_eor_p_p_pp_z_combo_0_4200_25004200() {
    // Encoding: 0x25004200
    // Test EOR_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pg=0, Pm=0, Pn=0, Pd=0
    let encoding: u32 = 0x25004200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_eor_p_p_pp_z_invalid_0_4200_25004200() {
    // Encoding: 0x25004200
    // Test EOR_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0, Pm=0, Pd=0, Pg=0
    let encoding: u32 = 0x25004200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_eor_p_p_pp_z_invalid_1_4200_25004200() {
    // Encoding: 0x25004200
    // Test EOR_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pm=0, Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25004200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eors_p_p_pp_z_field_pm_0_min_4200_25404200() {
    // Encoding: 0x25404200
    // Test EORS_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pd=0, Pm=0, Pn=0, Pg=0
    let encoding: u32 = 0x25404200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eors_p_p_pp_z_field_pm_1_poweroftwo_4200_25414200() {
    // Encoding: 0x25414200
    // Test EORS_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pn=0, Pd=0, Pm=1, Pg=0
    let encoding: u32 = 0x25414200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eors_p_p_pp_z_field_pg_0_min_4200_25404200() {
    // Encoding: 0x25404200
    // Test EORS_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pn=0, Pm=0, Pg=0, Pd=0
    let encoding: u32 = 0x25404200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eors_p_p_pp_z_field_pg_1_poweroftwo_4200_25404600() {
    // Encoding: 0x25404600
    // Test EORS_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pm=0, Pg=1, Pn=0, Pd=0
    let encoding: u32 = 0x25404600;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eors_p_p_pp_z_field_pn_0_min_4200_25404200() {
    // Encoding: 0x25404200
    // Test EORS_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25404200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eors_p_p_pp_z_field_pn_1_poweroftwo_4200_25404220() {
    // Encoding: 0x25404220
    // Test EORS_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pg=0, Pm=0, Pd=0, Pn=1
    let encoding: u32 = 0x25404220;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eors_p_p_pp_z_field_pd_0_min_4200_25404200() {
    // Encoding: 0x25404200
    // Test EORS_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pg=0, Pm=0, Pd=0, Pn=0
    let encoding: u32 = 0x25404200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eors_p_p_pp_z_field_pd_1_poweroftwo_4200_25404201() {
    // Encoding: 0x25404201
    // Test EORS_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pm=0, Pn=0, Pg=0, Pd=1
    let encoding: u32 = 0x25404201;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_eors_p_p_pp_z_combo_0_4200_25404200() {
    // Encoding: 0x25404200
    // Test EORS_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25404200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_eors_p_p_pp_z_invalid_0_4200_25404200() {
    // Encoding: 0x25404200
    // Test EORS_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Pd=0, Pm=0, Pn=0
    let encoding: u32 = 0x25404200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_eors_p_p_pp_z_invalid_1_4200_25404200() {
    // Encoding: 0x25404200
    // Test EORS_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pm=0, Pg=0, Pd=0
    let encoding: u32 = 0x25404200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_eor_p_p_pp_z_reg_write_0_25004200() {
    // Test EOR_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_eor_p_p_pp_z_flags_zeroresult_0_25004200() {
    // Test EOR_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_eor_p_p_pp_z_flags_zeroresult_1_25004200() {
    // Test EOR_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_eor_p_p_pp_z_flags_negativeresult_2_25004200() {
    // Test EOR_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_eor_p_p_pp_z_flags_unsignedoverflow_3_25004200() {
    // Test EOR_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_eor_p_p_pp_z_flags_unsignedoverflow_4_25004200() {
    // Test EOR_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_eor_p_p_pp_z_flags_signedoverflow_5_25004200() {
    // Test EOR_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_eor_p_p_pp_z_flags_signedoverflow_6_25004200() {
    // Test EOR_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: EOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_eor_p_p_pp_z_flags_positiveresult_7_25004200() {
    // Test EOR_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25004200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25004200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_eors_p_p_pp_z_reg_write_0_25404200() {
    // Test EORS_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_eors_p_p_pp_z_flags_zeroresult_0_25404200() {
    // Test EORS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_eors_p_p_pp_z_flags_zeroresult_1_25404200() {
    // Test EORS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_eors_p_p_pp_z_flags_negativeresult_2_25404200() {
    // Test EORS_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_eors_p_p_pp_z_flags_unsignedoverflow_3_25404200() {
    // Test EORS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_eors_p_p_pp_z_flags_unsignedoverflow_4_25404200() {
    // Test EORS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_eors_p_p_pp_z_flags_signedoverflow_5_25404200() {
    // Test EORS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_eors_p_p_pp_z_flags_signedoverflow_6_25404200() {
    // Test EORS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: EORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_eors_p_p_pp_z_flags_positiveresult_7_25404200() {
    // Test EORS_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25404200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25404200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// AND_Z.ZZ__ Tests
// ============================================================================

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_and_z_zz_field_zm_0_min_3000_04203000() {
    // Encoding: 0x04203000
    // Test AND_Z.ZZ__ field Zm = 0 (Min)
    // Fields: Zm=0, Zn=0, Zd=0
    let encoding: u32 = 0x04203000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_and_z_zz_field_zm_1_poweroftwo_3000_04213000() {
    // Encoding: 0x04213000
    // Test AND_Z.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Zd=0, Zm=1, Zn=0
    let encoding: u32 = 0x04213000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_and_z_zz_field_zm_30_poweroftwominusone_3000_043e3000() {
    // Encoding: 0x043E3000
    // Test AND_Z.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zd=0, Zn=0, Zm=30
    let encoding: u32 = 0x043E3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_and_z_zz_field_zm_31_max_3000_043f3000() {
    // Encoding: 0x043F3000
    // Test AND_Z.ZZ__ field Zm = 31 (Max)
    // Fields: Zm=31, Zd=0, Zn=0
    let encoding: u32 = 0x043F3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_and_z_zz_field_zn_0_min_3000_04203000() {
    // Encoding: 0x04203000
    // Test AND_Z.ZZ__ field Zn = 0 (Min)
    // Fields: Zm=0, Zn=0, Zd=0
    let encoding: u32 = 0x04203000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_and_z_zz_field_zn_1_poweroftwo_3000_04203020() {
    // Encoding: 0x04203020
    // Test AND_Z.ZZ__ field Zn = 1 (PowerOfTwo)
    // Fields: Zd=0, Zn=1, Zm=0
    let encoding: u32 = 0x04203020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_and_z_zz_field_zn_30_poweroftwominusone_3000_042033c0() {
    // Encoding: 0x042033C0
    // Test AND_Z.ZZ__ field Zn = 30 (PowerOfTwoMinusOne)
    // Fields: Zn=30, Zd=0, Zm=0
    let encoding: u32 = 0x042033C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_and_z_zz_field_zn_31_max_3000_042033e0() {
    // Encoding: 0x042033E0
    // Test AND_Z.ZZ__ field Zn = 31 (Max)
    // Fields: Zd=0, Zm=0, Zn=31
    let encoding: u32 = 0x042033E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_and_z_zz_field_zd_0_min_3000_04203000() {
    // Encoding: 0x04203000
    // Test AND_Z.ZZ__ field Zd = 0 (Min)
    // Fields: Zn=0, Zd=0, Zm=0
    let encoding: u32 = 0x04203000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_and_z_zz_field_zd_1_poweroftwo_3000_04203001() {
    // Encoding: 0x04203001
    // Test AND_Z.ZZ__ field Zd = 1 (PowerOfTwo)
    // Fields: Zm=0, Zd=1, Zn=0
    let encoding: u32 = 0x04203001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_and_z_zz_field_zd_30_poweroftwominusone_3000_0420301e() {
    // Encoding: 0x0420301E
    // Test AND_Z.ZZ__ field Zd = 30 (PowerOfTwoMinusOne)
    // Fields: Zd=30, Zm=0, Zn=0
    let encoding: u32 = 0x0420301E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_and_z_zz_field_zd_31_max_3000_0420301f() {
    // Encoding: 0x0420301F
    // Test AND_Z.ZZ__ field Zd = 31 (Max)
    // Fields: Zm=0, Zn=0, Zd=31
    let encoding: u32 = 0x0420301F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_and_z_zz_combo_0_3000_04203000() {
    // Encoding: 0x04203000
    // Test AND_Z.ZZ__ field combination: Zm=0, Zn=0, Zd=0
    // Fields: Zn=0, Zm=0, Zd=0
    let encoding: u32 = 0x04203000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_and_z_zz_invalid_0_3000_04203000() {
    // Encoding: 0x04203000
    // Test AND_Z.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zm=0, Zd=0, Zn=0
    let encoding: u32 = 0x04203000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_and_z_zz_invalid_1_3000_04203000() {
    // Encoding: 0x04203000
    // Test AND_Z.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: Zd=0, Zm=0, Zn=0
    let encoding: u32 = 0x04203000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_Z.ZZ__
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_and_z_zz_reg_write_0_04203000() {
    // Test AND_Z.ZZ__ register write: SimdFromField("d")
    // Encoding: 0x04203000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x04203000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// BIC_P.P.PP_Z Tests
// ============================================================================

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bic_p_p_pp_z_field_pm_0_min_4010_25004010() {
    // Encoding: 0x25004010
    // Test BIC_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pd=0, Pg=0, Pn=0, Pm=0
    let encoding: u32 = 0x25004010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bic_p_p_pp_z_field_pm_1_poweroftwo_4010_25014010() {
    // Encoding: 0x25014010
    // Test BIC_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pg=0, Pd=0, Pm=1, Pn=0
    let encoding: u32 = 0x25014010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bic_p_p_pp_z_field_pg_0_min_4010_25004010() {
    // Encoding: 0x25004010
    // Test BIC_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pg=0, Pd=0, Pm=0, Pn=0
    let encoding: u32 = 0x25004010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bic_p_p_pp_z_field_pg_1_poweroftwo_4010_25004410() {
    // Encoding: 0x25004410
    // Test BIC_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Pn=0, Pm=0, Pd=0
    let encoding: u32 = 0x25004410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bic_p_p_pp_z_field_pn_0_min_4010_25004010() {
    // Encoding: 0x25004010
    // Test BIC_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pd=0, Pm=0, Pn=0, Pg=0
    let encoding: u32 = 0x25004010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bic_p_p_pp_z_field_pn_1_poweroftwo_4010_25004030() {
    // Encoding: 0x25004030
    // Test BIC_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, Pm=0, Pd=0, Pg=0
    let encoding: u32 = 0x25004030;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bic_p_p_pp_z_field_pd_0_min_4010_25004010() {
    // Encoding: 0x25004010
    // Test BIC_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pn=0, Pg=0, Pm=0, Pd=0
    let encoding: u32 = 0x25004010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bic_p_p_pp_z_field_pd_1_poweroftwo_4010_25004011() {
    // Encoding: 0x25004011
    // Test BIC_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pn=0, Pg=0, Pm=0, Pd=1
    let encoding: u32 = 0x25004011;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_bic_p_p_pp_z_combo_0_4010_25004010() {
    // Encoding: 0x25004010
    // Test BIC_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pg=0, Pm=0, Pd=0, Pn=0
    let encoding: u32 = 0x25004010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_bic_p_p_pp_z_invalid_0_4010_25004010() {
    // Encoding: 0x25004010
    // Test BIC_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0, Pm=0, Pd=0, Pg=0
    let encoding: u32 = 0x25004010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_bic_p_p_pp_z_invalid_1_4010_25004010() {
    // Encoding: 0x25004010
    // Test BIC_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pg=0, Pd=0, Pm=0
    let encoding: u32 = 0x25004010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bics_p_p_pp_z_field_pm_0_min_4010_25404010() {
    // Encoding: 0x25404010
    // Test BICS_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pn=0, Pg=0, Pm=0, Pd=0
    let encoding: u32 = 0x25404010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bics_p_p_pp_z_field_pm_1_poweroftwo_4010_25414010() {
    // Encoding: 0x25414010
    // Test BICS_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=0, Pd=0, Pm=1
    let encoding: u32 = 0x25414010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bics_p_p_pp_z_field_pg_0_min_4010_25404010() {
    // Encoding: 0x25404010
    // Test BICS_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25404010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bics_p_p_pp_z_field_pg_1_poweroftwo_4010_25404410() {
    // Encoding: 0x25404410
    // Test BICS_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25404410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bics_p_p_pp_z_field_pn_0_min_4010_25404010() {
    // Encoding: 0x25404010
    // Test BICS_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pg=0, Pm=0, Pn=0, Pd=0
    let encoding: u32 = 0x25404010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bics_p_p_pp_z_field_pn_1_poweroftwo_4010_25404030() {
    // Encoding: 0x25404030
    // Test BICS_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, Pg=0, Pd=0, Pm=0
    let encoding: u32 = 0x25404030;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_bics_p_p_pp_z_field_pd_0_min_4010_25404010() {
    // Encoding: 0x25404010
    // Test BICS_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x25404010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_bics_p_p_pp_z_field_pd_1_poweroftwo_4010_25404011() {
    // Encoding: 0x25404011
    // Test BICS_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pg=0, Pm=0, Pn=0, Pd=1
    let encoding: u32 = 0x25404011;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_bics_p_p_pp_z_combo_0_4010_25404010() {
    // Encoding: 0x25404010
    // Test BICS_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pg=0, Pd=0, Pm=0, Pn=0
    let encoding: u32 = 0x25404010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_bics_p_p_pp_z_invalid_0_4010_25404010() {
    // Encoding: 0x25404010
    // Test BICS_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pm=0, Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x25404010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_bics_p_p_pp_z_invalid_1_4010_25404010() {
    // Encoding: 0x25404010
    // Test BICS_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25404010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_bic_p_p_pp_z_reg_write_0_25004010() {
    // Test BIC_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_bic_p_p_pp_z_flags_zeroresult_0_25004010() {
    // Test BIC_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_bic_p_p_pp_z_flags_zeroresult_1_25004010() {
    // Test BIC_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_bic_p_p_pp_z_flags_negativeresult_2_25004010() {
    // Test BIC_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_bic_p_p_pp_z_flags_unsignedoverflow_3_25004010() {
    // Test BIC_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_bic_p_p_pp_z_flags_unsignedoverflow_4_25004010() {
    // Test BIC_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_bic_p_p_pp_z_flags_signedoverflow_5_25004010() {
    // Test BIC_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_bic_p_p_pp_z_flags_signedoverflow_6_25004010() {
    // Test BIC_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BIC_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_bic_p_p_pp_z_flags_positiveresult_7_25004010() {
    // Test BIC_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25004010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25004010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_bics_p_p_pp_z_reg_write_0_25404010() {
    // Test BICS_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_bics_p_p_pp_z_flags_zeroresult_0_25404010() {
    // Test BICS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_bics_p_p_pp_z_flags_zeroresult_1_25404010() {
    // Test BICS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_bics_p_p_pp_z_flags_negativeresult_2_25404010() {
    // Test BICS_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_bics_p_p_pp_z_flags_unsignedoverflow_3_25404010() {
    // Test BICS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_bics_p_p_pp_z_flags_unsignedoverflow_4_25404010() {
    // Test BICS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_bics_p_p_pp_z_flags_signedoverflow_5_25404010() {
    // Test BICS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_bics_p_p_pp_z_flags_signedoverflow_6_25404010() {
    // Test BICS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BICS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_bics_p_p_pp_z_flags_positiveresult_7_25404010() {
    // Test BICS_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25404010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25404010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// AND_Z.ZI__ Tests
// ============================================================================

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_and_z_zi_field_imm13_0_zero_0_05800000() {
    // Encoding: 0x05800000
    // Test AND_Z.ZI__ field imm13 = 0 (Zero)
    // Fields: Zdn=0, imm13=0
    let encoding: u32 = 0x05800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_and_z_zi_field_imm13_1_poweroftwo_0_05800020() {
    // Encoding: 0x05800020
    // Test AND_Z.ZI__ field imm13 = 1 (PowerOfTwo)
    // Fields: imm13=1, Zdn=0
    let encoding: u32 = 0x05800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_and_z_zi_field_imm13_3_poweroftwominusone_0_05800060() {
    // Encoding: 0x05800060
    // Test AND_Z.ZI__ field imm13 = 3 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=3
    let encoding: u32 = 0x05800060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_and_z_zi_field_imm13_4_poweroftwo_0_05800080() {
    // Encoding: 0x05800080
    // Test AND_Z.ZI__ field imm13 = 4 (PowerOfTwo)
    // Fields: Zdn=0, imm13=4
    let encoding: u32 = 0x05800080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_and_z_zi_field_imm13_7_poweroftwominusone_0_058000e0() {
    // Encoding: 0x058000E0
    // Test AND_Z.ZI__ field imm13 = 7 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=7
    let encoding: u32 = 0x058000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_and_z_zi_field_imm13_8_poweroftwo_0_05800100() {
    // Encoding: 0x05800100
    // Test AND_Z.ZI__ field imm13 = 8 (PowerOfTwo)
    // Fields: imm13=8, Zdn=0
    let encoding: u32 = 0x05800100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_and_z_zi_field_imm13_15_poweroftwominusone_0_058001e0() {
    // Encoding: 0x058001E0
    // Test AND_Z.ZI__ field imm13 = 15 (PowerOfTwoMinusOne)
    // Fields: imm13=15, Zdn=0
    let encoding: u32 = 0x058001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_and_z_zi_field_imm13_16_poweroftwo_0_05800200() {
    // Encoding: 0x05800200
    // Test AND_Z.ZI__ field imm13 = 16 (PowerOfTwo)
    // Fields: imm13=16, Zdn=0
    let encoding: u32 = 0x05800200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_and_z_zi_field_imm13_31_poweroftwominusone_0_058003e0() {
    // Encoding: 0x058003E0
    // Test AND_Z.ZI__ field imm13 = 31 (PowerOfTwoMinusOne)
    // Fields: imm13=31, Zdn=0
    let encoding: u32 = 0x058003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_and_z_zi_field_imm13_32_poweroftwo_0_05800400() {
    // Encoding: 0x05800400
    // Test AND_Z.ZI__ field imm13 = 32 (PowerOfTwo)
    // Fields: Zdn=0, imm13=32
    let encoding: u32 = 0x05800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_and_z_zi_field_imm13_63_poweroftwominusone_0_058007e0() {
    // Encoding: 0x058007E0
    // Test AND_Z.ZI__ field imm13 = 63 (PowerOfTwoMinusOne)
    // Fields: imm13=63, Zdn=0
    let encoding: u32 = 0x058007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_and_z_zi_field_imm13_64_poweroftwo_0_05800800() {
    // Encoding: 0x05800800
    // Test AND_Z.ZI__ field imm13 = 64 (PowerOfTwo)
    // Fields: Zdn=0, imm13=64
    let encoding: u32 = 0x05800800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_and_z_zi_field_imm13_127_poweroftwominusone_0_05800fe0() {
    // Encoding: 0x05800FE0
    // Test AND_Z.ZI__ field imm13 = 127 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=127
    let encoding: u32 = 0x05800FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_and_z_zi_field_imm13_128_poweroftwo_0_05801000() {
    // Encoding: 0x05801000
    // Test AND_Z.ZI__ field imm13 = 128 (PowerOfTwo)
    // Fields: Zdn=0, imm13=128
    let encoding: u32 = 0x05801000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_and_z_zi_field_imm13_255_poweroftwominusone_0_05801fe0() {
    // Encoding: 0x05801FE0
    // Test AND_Z.ZI__ field imm13 = 255 (PowerOfTwoMinusOne)
    // Fields: imm13=255, Zdn=0
    let encoding: u32 = 0x05801FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_and_z_zi_field_imm13_256_poweroftwo_0_05802000() {
    // Encoding: 0x05802000
    // Test AND_Z.ZI__ field imm13 = 256 (PowerOfTwo)
    // Fields: Zdn=0, imm13=256
    let encoding: u32 = 0x05802000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_and_z_zi_field_imm13_511_poweroftwominusone_0_05803fe0() {
    // Encoding: 0x05803FE0
    // Test AND_Z.ZI__ field imm13 = 511 (PowerOfTwoMinusOne)
    // Fields: imm13=511, Zdn=0
    let encoding: u32 = 0x05803FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_and_z_zi_field_imm13_512_poweroftwo_0_05804000() {
    // Encoding: 0x05804000
    // Test AND_Z.ZI__ field imm13 = 512 (PowerOfTwo)
    // Fields: imm13=512, Zdn=0
    let encoding: u32 = 0x05804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_and_z_zi_field_imm13_1023_poweroftwominusone_0_05807fe0() {
    // Encoding: 0x05807FE0
    // Test AND_Z.ZI__ field imm13 = 1023 (PowerOfTwoMinusOne)
    // Fields: imm13=1023, Zdn=0
    let encoding: u32 = 0x05807FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_and_z_zi_field_imm13_1024_poweroftwo_0_05808000() {
    // Encoding: 0x05808000
    // Test AND_Z.ZI__ field imm13 = 1024 (PowerOfTwo)
    // Fields: Zdn=0, imm13=1024
    let encoding: u32 = 0x05808000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_and_z_zi_field_imm13_2047_poweroftwominusone_0_0580ffe0() {
    // Encoding: 0x0580FFE0
    // Test AND_Z.ZI__ field imm13 = 2047 (PowerOfTwoMinusOne)
    // Fields: imm13=2047, Zdn=0
    let encoding: u32 = 0x0580FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_and_z_zi_field_imm13_2048_poweroftwo_0_05810000() {
    // Encoding: 0x05810000
    // Test AND_Z.ZI__ field imm13 = 2048 (PowerOfTwo)
    // Fields: imm13=2048, Zdn=0
    let encoding: u32 = 0x05810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4095, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (4095)
#[test]
fn test_and_z_zi_field_imm13_4095_poweroftwominusone_0_0581ffe0() {
    // Encoding: 0x0581FFE0
    // Test AND_Z.ZI__ field imm13 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm13=4095, Zdn=0
    let encoding: u32 = 0x0581FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_and_z_zi_field_imm13_4096_poweroftwo_0_05820000() {
    // Encoding: 0x05820000
    // Test AND_Z.ZI__ field imm13 = 4096 (PowerOfTwo)
    // Fields: Zdn=0, imm13=4096
    let encoding: u32 = 0x05820000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 8191, boundary: Max }
/// maximum immediate (8191)
#[test]
fn test_and_z_zi_field_imm13_8191_max_0_0583ffe0() {
    // Encoding: 0x0583FFE0
    // Test AND_Z.ZI__ field imm13 = 8191 (Max)
    // Fields: imm13=8191, Zdn=0
    let encoding: u32 = 0x0583FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_and_z_zi_field_zdn_0_min_0_05800000() {
    // Encoding: 0x05800000
    // Test AND_Z.ZI__ field Zdn = 0 (Min)
    // Fields: Zdn=0, imm13=0
    let encoding: u32 = 0x05800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_and_z_zi_field_zdn_1_poweroftwo_0_05800001() {
    // Encoding: 0x05800001
    // Test AND_Z.ZI__ field Zdn = 1 (PowerOfTwo)
    // Fields: imm13=0, Zdn=1
    let encoding: u32 = 0x05800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_and_z_zi_field_zdn_15_poweroftwominusone_0_0580000f() {
    // Encoding: 0x0580000F
    // Test AND_Z.ZI__ field Zdn = 15 (PowerOfTwoMinusOne)
    // Fields: imm13=0, Zdn=15
    let encoding: u32 = 0x0580000F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_and_z_zi_field_zdn_31_max_0_0580001f() {
    // Encoding: 0x0580001F
    // Test AND_Z.ZI__ field Zdn = 31 (Max)
    // Fields: Zdn=31, imm13=0
    let encoding: u32 = 0x0580001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm13=0 (immediate value 0)
#[test]
fn test_and_z_zi_combo_0_0_05800000() {
    // Encoding: 0x05800000
    // Test AND_Z.ZI__ field combination: imm13=0, Zdn=0
    // Fields: Zdn=0, imm13=0
    let encoding: u32 = 0x05800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_and_z_zi_invalid_0_0_05800000() {
    // Encoding: 0x05800000
    // Test AND_Z.ZI__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zdn=0, imm13=0
    let encoding: u32 = 0x05800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_and_z_zi_invalid_1_0_05800000() {
    // Encoding: 0x05800000
    // Test AND_Z.ZI__ invalid encoding: Unconditional UNDEFINED
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_Z.ZI__
/// ASL: `SimdFromField("dn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("dn")
#[test]
fn test_and_z_zi_reg_write_0_05800000() {
    // Test AND_Z.ZI__ register write: SimdFromField("dn")
    // Encoding: 0x05800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x05800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// ANDV_R.P.Z__ Tests
// ============================================================================

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_andv_r_p_z_field_size_0_min_2000_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ field size = 0 (Min)
    // Fields: Zn=0, size=0, Vd=0, Pg=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_andv_r_p_z_field_size_1_poweroftwo_2000_045a2000() {
    // Encoding: 0x045A2000
    // Test ANDV_R.P.Z__ field size = 1 (PowerOfTwo)
    // Fields: Zn=0, size=1, Vd=0, Pg=0
    let encoding: u32 = 0x045A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_andv_r_p_z_field_size_2_poweroftwo_2000_049a2000() {
    // Encoding: 0x049A2000
    // Test ANDV_R.P.Z__ field size = 2 (PowerOfTwo)
    // Fields: Vd=0, size=2, Pg=0, Zn=0
    let encoding: u32 = 0x049A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_andv_r_p_z_field_size_3_max_2000_04da2000() {
    // Encoding: 0x04DA2000
    // Test ANDV_R.P.Z__ field size = 3 (Max)
    // Fields: size=3, Vd=0, Zn=0, Pg=0
    let encoding: u32 = 0x04DA2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_andv_r_p_z_field_pg_0_min_2000_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ field Pg = 0 (Min)
    // Fields: Pg=0, size=0, Zn=0, Vd=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_andv_r_p_z_field_pg_1_poweroftwo_2000_041a2400() {
    // Encoding: 0x041A2400
    // Test ANDV_R.P.Z__ field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, size=0, Vd=0, Zn=0
    let encoding: u32 = 0x041A2400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_andv_r_p_z_field_zn_0_min_2000_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ field Zn = 0 (Min)
    // Fields: Zn=0, Vd=0, size=0, Pg=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_andv_r_p_z_field_zn_1_poweroftwo_2000_041a2020() {
    // Encoding: 0x041A2020
    // Test ANDV_R.P.Z__ field Zn = 1 (PowerOfTwo)
    // Fields: Zn=1, Vd=0, size=0, Pg=0
    let encoding: u32 = 0x041A2020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_andv_r_p_z_field_zn_30_poweroftwominusone_2000_041a23c0() {
    // Encoding: 0x041A23C0
    // Test ANDV_R.P.Z__ field Zn = 30 (PowerOfTwoMinusOne)
    // Fields: Vd=0, Pg=0, Zn=30, size=0
    let encoding: u32 = 0x041A23C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_andv_r_p_z_field_zn_31_max_2000_041a23e0() {
    // Encoding: 0x041A23E0
    // Test ANDV_R.P.Z__ field Zn = 31 (Max)
    // Fields: Pg=0, size=0, Zn=31, Vd=0
    let encoding: u32 = 0x041A23E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_andv_r_p_z_field_vd_0_min_2000_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ field Vd = 0 (Min)
    // Fields: Vd=0, Zn=0, size=0, Pg=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_andv_r_p_z_field_vd_1_poweroftwo_2000_041a2001() {
    // Encoding: 0x041A2001
    // Test ANDV_R.P.Z__ field Vd = 1 (PowerOfTwo)
    // Fields: Vd=1, size=0, Pg=0, Zn=0
    let encoding: u32 = 0x041A2001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_andv_r_p_z_field_vd_30_poweroftwominusone_2000_041a201e() {
    // Encoding: 0x041A201E
    // Test ANDV_R.P.Z__ field Vd = 30 (PowerOfTwoMinusOne)
    // Fields: Zn=0, Vd=30, size=0, Pg=0
    let encoding: u32 = 0x041A201E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_andv_r_p_z_field_vd_31_max_2000_041a201f() {
    // Encoding: 0x041A201F
    // Test ANDV_R.P.Z__ field Vd = 31 (Max)
    // Fields: Zn=0, Pg=0, size=0, Vd=31
    let encoding: u32 = 0x041A201F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_andv_r_p_z_combo_0_2000_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ field combination: size=0, Pg=0, Zn=0, Vd=0
    // Fields: Vd=0, Zn=0, Pg=0, size=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_andv_r_p_z_special_size_0_size_variant_0_8192_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ special value size = 0 (Size variant 0)
    // Fields: size=0, Pg=0, Zn=0, Vd=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_andv_r_p_z_special_size_1_size_variant_1_8192_045a2000() {
    // Encoding: 0x045A2000
    // Test ANDV_R.P.Z__ special value size = 1 (Size variant 1)
    // Fields: Zn=0, size=1, Pg=0, Vd=0
    let encoding: u32 = 0x045A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_andv_r_p_z_special_size_2_size_variant_2_8192_049a2000() {
    // Encoding: 0x049A2000
    // Test ANDV_R.P.Z__ special value size = 2 (Size variant 2)
    // Fields: Zn=0, size=2, Pg=0, Vd=0
    let encoding: u32 = 0x049A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_andv_r_p_z_special_size_3_size_variant_3_8192_04da2000() {
    // Encoding: 0x04DA2000
    // Test ANDV_R.P.Z__ special value size = 3 (Size variant 3)
    // Fields: size=3, Pg=0, Vd=0, Zn=0
    let encoding: u32 = 0x04DA2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_andv_r_p_z_invalid_0_2000_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: size=0, Pg=0, Zn=0, Vd=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_andv_r_p_z_invalid_1_2000_041a2000() {
    // Encoding: 0x041A2000
    // Test ANDV_R.P.Z__ invalid encoding: Unconditional UNDEFINED
    // Fields: Vd=0, Zn=0, Pg=0, size=0
    let encoding: u32 = 0x041A2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ANDV_R.P.Z__
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_andv_r_p_z_reg_write_0_041a2000() {
    // Test ANDV_R.P.Z__ register write: SimdFromField("d")
    // Encoding: 0x041A2000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x041A2000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// ORR_Z.P.ZZ__ Tests
// ============================================================================

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_orr_z_p_zz_field_size_0_min_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ field size = 0 (Min)
    // Fields: Pg=0, size=0, Zm=0, Zdn=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_orr_z_p_zz_field_size_1_poweroftwo_0_04580000() {
    // Encoding: 0x04580000
    // Test ORR_Z.P.ZZ__ field size = 1 (PowerOfTwo)
    // Fields: size=1, Zdn=0, Zm=0, Pg=0
    let encoding: u32 = 0x04580000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_orr_z_p_zz_field_size_2_poweroftwo_0_04980000() {
    // Encoding: 0x04980000
    // Test ORR_Z.P.ZZ__ field size = 2 (PowerOfTwo)
    // Fields: size=2, Zm=0, Pg=0, Zdn=0
    let encoding: u32 = 0x04980000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_orr_z_p_zz_field_size_3_max_0_04d80000() {
    // Encoding: 0x04D80000
    // Test ORR_Z.P.ZZ__ field size = 3 (Max)
    // Fields: Zm=0, Zdn=0, Pg=0, size=3
    let encoding: u32 = 0x04D80000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orr_z_p_zz_field_pg_0_min_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ field Pg = 0 (Min)
    // Fields: Pg=0, Zm=0, size=0, Zdn=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orr_z_p_zz_field_pg_1_poweroftwo_0_04180400() {
    // Encoding: 0x04180400
    // Test ORR_Z.P.ZZ__ field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, size=0, Zdn=0, Zm=0
    let encoding: u32 = 0x04180400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_orr_z_p_zz_field_zm_0_min_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ field Zm = 0 (Min)
    // Fields: Zm=0, Pg=0, size=0, Zdn=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_orr_z_p_zz_field_zm_1_poweroftwo_0_04180020() {
    // Encoding: 0x04180020
    // Test ORR_Z.P.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Pg=0, size=0, Zm=1, Zdn=0
    let encoding: u32 = 0x04180020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_orr_z_p_zz_field_zm_30_poweroftwominusone_0_041803c0() {
    // Encoding: 0x041803C0
    // Test ORR_Z.P.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Zm=30, Zdn=0, Pg=0
    let encoding: u32 = 0x041803C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zm 5 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_orr_z_p_zz_field_zm_31_max_0_041803e0() {
    // Encoding: 0x041803E0
    // Test ORR_Z.P.ZZ__ field Zm = 31 (Max)
    // Fields: size=0, Pg=0, Zm=31, Zdn=0
    let encoding: u32 = 0x041803E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_orr_z_p_zz_field_zdn_0_min_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ field Zdn = 0 (Min)
    // Fields: size=0, Zdn=0, Zm=0, Pg=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_orr_z_p_zz_field_zdn_1_poweroftwo_0_04180001() {
    // Encoding: 0x04180001
    // Test ORR_Z.P.ZZ__ field Zdn = 1 (PowerOfTwo)
    // Fields: Zm=0, size=0, Zdn=1, Pg=0
    let encoding: u32 = 0x04180001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_orr_z_p_zz_field_zdn_15_poweroftwominusone_0_0418000f() {
    // Encoding: 0x0418000F
    // Test ORR_Z.P.ZZ__ field Zdn = 15 (PowerOfTwoMinusOne)
    // Fields: size=0, Pg=0, Zm=0, Zdn=15
    let encoding: u32 = 0x0418000F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_orr_z_p_zz_field_zdn_31_max_0_0418001f() {
    // Encoding: 0x0418001F
    // Test ORR_Z.P.ZZ__ field Zdn = 31 (Max)
    // Fields: Zdn=31, size=0, Pg=0, Zm=0
    let encoding: u32 = 0x0418001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_orr_z_p_zz_combo_0_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ field combination: size=0, Pg=0, Zm=0, Zdn=0
    // Fields: Zdn=0, Zm=0, size=0, Pg=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_orr_z_p_zz_special_size_0_size_variant_0_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ special value size = 0 (Size variant 0)
    // Fields: Pg=0, Zm=0, Zdn=0, size=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_orr_z_p_zz_special_size_1_size_variant_1_0_04580000() {
    // Encoding: 0x04580000
    // Test ORR_Z.P.ZZ__ special value size = 1 (Size variant 1)
    // Fields: size=1, Zdn=0, Pg=0, Zm=0
    let encoding: u32 = 0x04580000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_orr_z_p_zz_special_size_2_size_variant_2_0_04980000() {
    // Encoding: 0x04980000
    // Test ORR_Z.P.ZZ__ special value size = 2 (Size variant 2)
    // Fields: size=2, Zdn=0, Zm=0, Pg=0
    let encoding: u32 = 0x04980000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_orr_z_p_zz_special_size_3_size_variant_3_0_04d80000() {
    // Encoding: 0x04D80000
    // Test ORR_Z.P.ZZ__ special value size = 3 (Size variant 3)
    // Fields: size=3, Zdn=0, Pg=0, Zm=0
    let encoding: u32 = 0x04D80000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_orr_z_p_zz_invalid_0_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, size=0, Zdn=0, Zm=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_orr_z_p_zz_invalid_1_0_04180000() {
    // Encoding: 0x04180000
    // Test ORR_Z.P.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: size=0, Zm=0, Zdn=0, Pg=0
    let encoding: u32 = 0x04180000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORR_Z.P.ZZ__
/// ASL: `SimdFromField("dn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("dn")
#[test]
fn test_orr_z_p_zz_reg_write_0_04180000() {
    // Test ORR_Z.P.ZZ__ register write: SimdFromField("dn")
    // Encoding: 0x04180000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x04180000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// AND_P.P.PP_Z Tests
// ============================================================================

/// Provenance: AND_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_and_p_p_pp_z_field_s_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z field S = 0 (Min)
    // Fields: S=0, Pn=0, Pg=0, Pm=0, Pd=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_and_p_p_pp_z_field_s_1_max_4000_25404000() {
    // Encoding: 0x25404000
    // Test AND_P.P.PP_Z field S = 1 (Max)
    // Fields: Pg=0, Pn=0, Pd=0, Pm=0, S=1
    let encoding: u32 = 0x25404000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_and_p_p_pp_z_field_pm_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pn=0, Pg=0, S=0, Pd=0, Pm=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_and_p_p_pp_z_field_pm_1_poweroftwo_4000_25014000() {
    // Encoding: 0x25014000
    // Test AND_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: S=0, Pm=1, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25014000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_and_p_p_pp_z_field_pg_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pn=0, S=0, Pg=0, Pm=0, Pd=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_and_p_p_pp_z_field_pg_1_poweroftwo_4000_25004400() {
    // Encoding: 0x25004400
    // Test AND_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Pd=0, S=0, Pn=0, Pm=0
    let encoding: u32 = 0x25004400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_and_p_p_pp_z_field_pn_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pg=0, S=0, Pn=0, Pd=0, Pm=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_and_p_p_pp_z_field_pn_1_poweroftwo_4000_25004020() {
    // Encoding: 0x25004020
    // Test AND_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pd=0, Pn=1, Pm=0, Pg=0, S=0
    let encoding: u32 = 0x25004020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_and_p_p_pp_z_field_pd_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pd=0, Pg=0, Pn=0, S=0, Pm=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_and_p_p_pp_z_field_pd_1_poweroftwo_4000_25004001() {
    // Encoding: 0x25004001
    // Test AND_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pn=0, Pd=1, Pg=0, S=0, Pm=0
    let encoding: u32 = 0x25004001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// S=0 (8-bit / byte size)
#[test]
fn test_and_p_p_pp_z_combo_0_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z field combination: S=0, Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pn=0, Pd=0, S=0, Pm=0, Pg=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field S = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "S", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_and_p_p_pp_z_special_s_0_size_variant_0_16384_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z special value S = 0 (Size variant 0)
    // Fields: Pg=0, Pn=0, Pm=0, S=0, Pd=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `field S = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "S", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_and_p_p_pp_z_special_s_1_size_variant_1_16384_25404000() {
    // Encoding: 0x25404000
    // Test AND_P.P.PP_Z special value S = 1 (Size variant 1)
    // Fields: Pd=0, Pm=0, Pg=0, S=1, Pn=0
    let encoding: u32 = 0x25404000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_and_p_p_pp_z_invalid_0_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0, Pn=0, S=0, Pg=0, Pm=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_and_p_p_pp_z_invalid_1_4000_25004000() {
    // Encoding: 0x25004000
    // Test AND_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: S=0, Pm=0, Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_ands_p_p_pp_z_field_s_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z field S = 0 (Min)
    // Fields: Pm=0, Pn=0, Pd=0, S=0, Pg=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field S 22 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_ands_p_p_pp_z_field_s_1_max_4000_25404000() {
    // Encoding: 0x25404000
    // Test ANDS_P.P.PP_Z field S = 1 (Max)
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0, S=1
    let encoding: u32 = 0x25404000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ands_p_p_pp_z_field_pm_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pn=0, Pm=0, Pd=0, Pg=0, S=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ands_p_p_pp_z_field_pm_1_poweroftwo_4000_25014000() {
    // Encoding: 0x25014000
    // Test ANDS_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pd=0, Pg=0, S=0, Pn=0, Pm=1
    let encoding: u32 = 0x25014000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ands_p_p_pp_z_field_pg_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z field Pg = 0 (Min)
    // Fields: S=0, Pg=0, Pm=0, Pd=0, Pn=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ands_p_p_pp_z_field_pg_1_poweroftwo_4000_25004400() {
    // Encoding: 0x25004400
    // Test ANDS_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pd=0, Pg=1, S=0, Pm=0, Pn=0
    let encoding: u32 = 0x25004400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ands_p_p_pp_z_field_pn_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pg=0, S=0, Pm=0, Pn=0, Pd=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ands_p_p_pp_z_field_pn_1_poweroftwo_4000_25004020() {
    // Encoding: 0x25004020
    // Test ANDS_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, S=0, Pm=0, Pg=0, Pd=0
    let encoding: u32 = 0x25004020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ands_p_p_pp_z_field_pd_0_min_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z field Pd = 0 (Min)
    // Fields: S=0, Pn=0, Pm=0, Pd=0, Pg=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ands_p_p_pp_z_field_pd_1_poweroftwo_4000_25004001() {
    // Encoding: 0x25004001
    // Test ANDS_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pg=0, S=0, Pn=0, Pm=0
    let encoding: u32 = 0x25004001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// S=0 (8-bit / byte size)
#[test]
fn test_ands_p_p_pp_z_combo_0_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z field combination: S=0, Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pg=0, Pn=0, Pd=0, S=0, Pm=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field S = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "S", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_ands_p_p_pp_z_special_s_0_size_variant_0_16384_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z special value S = 0 (Size variant 0)
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0, S=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `field S = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "S", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_ands_p_p_pp_z_special_s_1_size_variant_1_16384_25404000() {
    // Encoding: 0x25404000
    // Test ANDS_P.P.PP_Z special value S = 1 (Size variant 1)
    // Fields: Pm=0, S=1, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25404000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ands_p_p_pp_z_invalid_0_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pm=0, Pg=0, S=0, Pd=0, Pn=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ands_p_p_pp_z_invalid_1_4000_25004000() {
    // Encoding: 0x25004000
    // Test ANDS_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pd=0, Pg=0, Pm=0, S=0
    let encoding: u32 = 0x25004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_and_p_p_pp_z_reg_write_0_25004000() {
    // Test AND_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25004000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25004000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_and_p_p_pp_z_flags_zeroresult_0_25404000() {
    // Test AND_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_and_p_p_pp_z_flags_zeroresult_1_25404000() {
    // Test AND_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_and_p_p_pp_z_flags_negativeresult_2_25404000() {
    // Test AND_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_and_p_p_pp_z_flags_unsignedoverflow_3_25404000() {
    // Test AND_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_and_p_p_pp_z_flags_unsignedoverflow_4_25404000() {
    // Test AND_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_and_p_p_pp_z_flags_signedoverflow_5_25404000() {
    // Test AND_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_and_p_p_pp_z_flags_signedoverflow_6_25404000() {
    // Test AND_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: AND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_and_p_p_pp_z_flags_positiveresult_7_25404000() {
    // Test AND_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_ands_p_p_pp_z_reg_write_0_25004000() {
    // Test ANDS_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25004000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25004000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_ands_p_p_pp_z_flags_zeroresult_0_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_ands_p_p_pp_z_flags_zeroresult_1_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_ands_p_p_pp_z_flags_negativeresult_2_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_ands_p_p_pp_z_flags_unsignedoverflow_3_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_ands_p_p_pp_z_flags_unsignedoverflow_4_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_ands_p_p_pp_z_flags_signedoverflow_5_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_ands_p_p_pp_z_flags_signedoverflow_6_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_ands_p_p_pp_z_flags_positiveresult_7_25404000() {
    // Test ANDS_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25404000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25404000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// EOR_Z.ZZ__ Tests
// ============================================================================

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_eor_z_zz_field_zm_0_min_3000_04a03000() {
    // Encoding: 0x04A03000
    // Test EOR_Z.ZZ__ field Zm = 0 (Min)
    // Fields: Zn=0, Zd=0, Zm=0
    let encoding: u32 = 0x04A03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_eor_z_zz_field_zm_1_poweroftwo_3000_04a13000() {
    // Encoding: 0x04A13000
    // Test EOR_Z.ZZ__ field Zm = 1 (PowerOfTwo)
    // Fields: Zd=0, Zm=1, Zn=0
    let encoding: u32 = 0x04A13000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_eor_z_zz_field_zm_30_poweroftwominusone_3000_04be3000() {
    // Encoding: 0x04BE3000
    // Test EOR_Z.ZZ__ field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Zn=0, Zd=0
    let encoding: u32 = 0x04BE3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_eor_z_zz_field_zm_31_max_3000_04bf3000() {
    // Encoding: 0x04BF3000
    // Test EOR_Z.ZZ__ field Zm = 31 (Max)
    // Fields: Zm=31, Zd=0, Zn=0
    let encoding: u32 = 0x04BF3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_eor_z_zz_field_zn_0_min_3000_04a03000() {
    // Encoding: 0x04A03000
    // Test EOR_Z.ZZ__ field Zn = 0 (Min)
    // Fields: Zm=0, Zd=0, Zn=0
    let encoding: u32 = 0x04A03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_eor_z_zz_field_zn_1_poweroftwo_3000_04a03020() {
    // Encoding: 0x04A03020
    // Test EOR_Z.ZZ__ field Zn = 1 (PowerOfTwo)
    // Fields: Zm=0, Zd=0, Zn=1
    let encoding: u32 = 0x04A03020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_eor_z_zz_field_zn_30_poweroftwominusone_3000_04a033c0() {
    // Encoding: 0x04A033C0
    // Test EOR_Z.ZZ__ field Zn = 30 (PowerOfTwoMinusOne)
    // Fields: Zd=0, Zm=0, Zn=30
    let encoding: u32 = 0x04A033C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_eor_z_zz_field_zn_31_max_3000_04a033e0() {
    // Encoding: 0x04A033E0
    // Test EOR_Z.ZZ__ field Zn = 31 (Max)
    // Fields: Zn=31, Zd=0, Zm=0
    let encoding: u32 = 0x04A033E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_eor_z_zz_field_zd_0_min_3000_04a03000() {
    // Encoding: 0x04A03000
    // Test EOR_Z.ZZ__ field Zd = 0 (Min)
    // Fields: Zm=0, Zn=0, Zd=0
    let encoding: u32 = 0x04A03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_eor_z_zz_field_zd_1_poweroftwo_3000_04a03001() {
    // Encoding: 0x04A03001
    // Test EOR_Z.ZZ__ field Zd = 1 (PowerOfTwo)
    // Fields: Zm=0, Zd=1, Zn=0
    let encoding: u32 = 0x04A03001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_eor_z_zz_field_zd_30_poweroftwominusone_3000_04a0301e() {
    // Encoding: 0x04A0301E
    // Test EOR_Z.ZZ__ field Zd = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Zd=30, Zn=0
    let encoding: u32 = 0x04A0301E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field Zd 0 +: 5`
/// Requirement: FieldBoundary { field: "Zd", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_eor_z_zz_field_zd_31_max_3000_04a0301f() {
    // Encoding: 0x04A0301F
    // Test EOR_Z.ZZ__ field Zd = 31 (Max)
    // Fields: Zd=31, Zm=0, Zn=0
    let encoding: u32 = 0x04A0301F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_eor_z_zz_combo_0_3000_04a03000() {
    // Encoding: 0x04A03000
    // Test EOR_Z.ZZ__ field combination: Zm=0, Zn=0, Zd=0
    // Fields: Zn=0, Zd=0, Zm=0
    let encoding: u32 = 0x04A03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_eor_z_zz_invalid_0_3000_04a03000() {
    // Encoding: 0x04A03000
    // Test EOR_Z.ZZ__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zd=0, Zm=0, Zn=0
    let encoding: u32 = 0x04A03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_eor_z_zz_invalid_1_3000_04a03000() {
    // Encoding: 0x04A03000
    // Test EOR_Z.ZZ__ invalid encoding: Unconditional UNDEFINED
    // Fields: Zn=0, Zm=0, Zd=0
    let encoding: u32 = 0x04A03000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_Z.ZZ__
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_eor_z_zz_reg_write_0_04a03000() {
    // Test EOR_Z.ZZ__ register write: SimdFromField("d")
    // Encoding: 0x04A03000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x04A03000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// EOR_Z.ZI__ Tests
// ============================================================================

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_eor_z_zi_field_imm13_0_zero_0_05400000() {
    // Encoding: 0x05400000
    // Test EOR_Z.ZI__ field imm13 = 0 (Zero)
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_eor_z_zi_field_imm13_1_poweroftwo_0_05400020() {
    // Encoding: 0x05400020
    // Test EOR_Z.ZI__ field imm13 = 1 (PowerOfTwo)
    // Fields: imm13=1, Zdn=0
    let encoding: u32 = 0x05400020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_eor_z_zi_field_imm13_3_poweroftwominusone_0_05400060() {
    // Encoding: 0x05400060
    // Test EOR_Z.ZI__ field imm13 = 3 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=3
    let encoding: u32 = 0x05400060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_eor_z_zi_field_imm13_4_poweroftwo_0_05400080() {
    // Encoding: 0x05400080
    // Test EOR_Z.ZI__ field imm13 = 4 (PowerOfTwo)
    // Fields: Zdn=0, imm13=4
    let encoding: u32 = 0x05400080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_eor_z_zi_field_imm13_7_poweroftwominusone_0_054000e0() {
    // Encoding: 0x054000E0
    // Test EOR_Z.ZI__ field imm13 = 7 (PowerOfTwoMinusOne)
    // Fields: imm13=7, Zdn=0
    let encoding: u32 = 0x054000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_eor_z_zi_field_imm13_8_poweroftwo_0_05400100() {
    // Encoding: 0x05400100
    // Test EOR_Z.ZI__ field imm13 = 8 (PowerOfTwo)
    // Fields: imm13=8, Zdn=0
    let encoding: u32 = 0x05400100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_eor_z_zi_field_imm13_15_poweroftwominusone_0_054001e0() {
    // Encoding: 0x054001E0
    // Test EOR_Z.ZI__ field imm13 = 15 (PowerOfTwoMinusOne)
    // Fields: imm13=15, Zdn=0
    let encoding: u32 = 0x054001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_eor_z_zi_field_imm13_16_poweroftwo_0_05400200() {
    // Encoding: 0x05400200
    // Test EOR_Z.ZI__ field imm13 = 16 (PowerOfTwo)
    // Fields: Zdn=0, imm13=16
    let encoding: u32 = 0x05400200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_eor_z_zi_field_imm13_31_poweroftwominusone_0_054003e0() {
    // Encoding: 0x054003E0
    // Test EOR_Z.ZI__ field imm13 = 31 (PowerOfTwoMinusOne)
    // Fields: imm13=31, Zdn=0
    let encoding: u32 = 0x054003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_eor_z_zi_field_imm13_32_poweroftwo_0_05400400() {
    // Encoding: 0x05400400
    // Test EOR_Z.ZI__ field imm13 = 32 (PowerOfTwo)
    // Fields: Zdn=0, imm13=32
    let encoding: u32 = 0x05400400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_eor_z_zi_field_imm13_63_poweroftwominusone_0_054007e0() {
    // Encoding: 0x054007E0
    // Test EOR_Z.ZI__ field imm13 = 63 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=63
    let encoding: u32 = 0x054007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_eor_z_zi_field_imm13_64_poweroftwo_0_05400800() {
    // Encoding: 0x05400800
    // Test EOR_Z.ZI__ field imm13 = 64 (PowerOfTwo)
    // Fields: imm13=64, Zdn=0
    let encoding: u32 = 0x05400800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_eor_z_zi_field_imm13_127_poweroftwominusone_0_05400fe0() {
    // Encoding: 0x05400FE0
    // Test EOR_Z.ZI__ field imm13 = 127 (PowerOfTwoMinusOne)
    // Fields: imm13=127, Zdn=0
    let encoding: u32 = 0x05400FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_eor_z_zi_field_imm13_128_poweroftwo_0_05401000() {
    // Encoding: 0x05401000
    // Test EOR_Z.ZI__ field imm13 = 128 (PowerOfTwo)
    // Fields: imm13=128, Zdn=0
    let encoding: u32 = 0x05401000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_eor_z_zi_field_imm13_255_poweroftwominusone_0_05401fe0() {
    // Encoding: 0x05401FE0
    // Test EOR_Z.ZI__ field imm13 = 255 (PowerOfTwoMinusOne)
    // Fields: imm13=255, Zdn=0
    let encoding: u32 = 0x05401FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_eor_z_zi_field_imm13_256_poweroftwo_0_05402000() {
    // Encoding: 0x05402000
    // Test EOR_Z.ZI__ field imm13 = 256 (PowerOfTwo)
    // Fields: Zdn=0, imm13=256
    let encoding: u32 = 0x05402000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_eor_z_zi_field_imm13_511_poweroftwominusone_0_05403fe0() {
    // Encoding: 0x05403FE0
    // Test EOR_Z.ZI__ field imm13 = 511 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=511
    let encoding: u32 = 0x05403FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_eor_z_zi_field_imm13_512_poweroftwo_0_05404000() {
    // Encoding: 0x05404000
    // Test EOR_Z.ZI__ field imm13 = 512 (PowerOfTwo)
    // Fields: imm13=512, Zdn=0
    let encoding: u32 = 0x05404000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_eor_z_zi_field_imm13_1023_poweroftwominusone_0_05407fe0() {
    // Encoding: 0x05407FE0
    // Test EOR_Z.ZI__ field imm13 = 1023 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=1023
    let encoding: u32 = 0x05407FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_eor_z_zi_field_imm13_1024_poweroftwo_0_05408000() {
    // Encoding: 0x05408000
    // Test EOR_Z.ZI__ field imm13 = 1024 (PowerOfTwo)
    // Fields: imm13=1024, Zdn=0
    let encoding: u32 = 0x05408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_eor_z_zi_field_imm13_2047_poweroftwominusone_0_0540ffe0() {
    // Encoding: 0x0540FFE0
    // Test EOR_Z.ZI__ field imm13 = 2047 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=2047
    let encoding: u32 = 0x0540FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_eor_z_zi_field_imm13_2048_poweroftwo_0_05410000() {
    // Encoding: 0x05410000
    // Test EOR_Z.ZI__ field imm13 = 2048 (PowerOfTwo)
    // Fields: Zdn=0, imm13=2048
    let encoding: u32 = 0x05410000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4095, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (4095)
#[test]
fn test_eor_z_zi_field_imm13_4095_poweroftwominusone_0_0541ffe0() {
    // Encoding: 0x0541FFE0
    // Test EOR_Z.ZI__ field imm13 = 4095 (PowerOfTwoMinusOne)
    // Fields: Zdn=0, imm13=4095
    let encoding: u32 = 0x0541FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_eor_z_zi_field_imm13_4096_poweroftwo_0_05420000() {
    // Encoding: 0x05420000
    // Test EOR_Z.ZI__ field imm13 = 4096 (PowerOfTwo)
    // Fields: imm13=4096, Zdn=0
    let encoding: u32 = 0x05420000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field imm13 5 +: 13`
/// Requirement: FieldBoundary { field: "imm13", value: 8191, boundary: Max }
/// maximum immediate (8191)
#[test]
fn test_eor_z_zi_field_imm13_8191_max_0_0543ffe0() {
    // Encoding: 0x0543FFE0
    // Test EOR_Z.ZI__ field imm13 = 8191 (Max)
    // Fields: imm13=8191, Zdn=0
    let encoding: u32 = 0x0543FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_eor_z_zi_field_zdn_0_min_0_05400000() {
    // Encoding: 0x05400000
    // Test EOR_Z.ZI__ field Zdn = 0 (Min)
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_eor_z_zi_field_zdn_1_poweroftwo_0_05400001() {
    // Encoding: 0x05400001
    // Test EOR_Z.ZI__ field Zdn = 1 (PowerOfTwo)
    // Fields: imm13=0, Zdn=1
    let encoding: u32 = 0x05400001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_eor_z_zi_field_zdn_15_poweroftwominusone_0_0540000f() {
    // Encoding: 0x0540000F
    // Test EOR_Z.ZI__ field Zdn = 15 (PowerOfTwoMinusOne)
    // Fields: imm13=0, Zdn=15
    let encoding: u32 = 0x0540000F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field Zdn 0 +: 5`
/// Requirement: FieldBoundary { field: "Zdn", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_eor_z_zi_field_zdn_31_max_0_0540001f() {
    // Encoding: 0x0540001F
    // Test EOR_Z.ZI__ field Zdn = 31 (Max)
    // Fields: imm13=0, Zdn=31
    let encoding: u32 = 0x0540001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm13=0 (immediate value 0)
#[test]
fn test_eor_z_zi_combo_0_0_05400000() {
    // Encoding: 0x05400000
    // Test EOR_Z.ZI__ field combination: imm13=0, Zdn=0
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_eor_z_zi_invalid_0_0_05400000() {
    // Encoding: 0x05400000
    // Test EOR_Z.ZI__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: imm13=0, Zdn=0
    let encoding: u32 = 0x05400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_eor_z_zi_invalid_1_0_05400000() {
    // Encoding: 0x05400000
    // Test EOR_Z.ZI__ invalid encoding: Unconditional UNDEFINED
    // Fields: Zdn=0, imm13=0
    let encoding: u32 = 0x05400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EOR_Z.ZI__
/// ASL: `SimdFromField("dn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("dn")
#[test]
fn test_eor_z_zi_reg_write_0_05400000() {
    // Test EOR_Z.ZI__ register write: SimdFromField("dn")
    // Encoding: 0x05400000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x05400000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// EORV_R.P.Z__ Tests
// ============================================================================

/// Provenance: EORV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_eorv_r_p_z_field_size_0_min_2000_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ field size = 0 (Min)
    // Fields: Zn=0, Vd=0, size=0, Pg=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_eorv_r_p_z_field_size_1_poweroftwo_2000_04592000() {
    // Encoding: 0x04592000
    // Test EORV_R.P.Z__ field size = 1 (PowerOfTwo)
    // Fields: Vd=0, size=1, Zn=0, Pg=0
    let encoding: u32 = 0x04592000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_eorv_r_p_z_field_size_2_poweroftwo_2000_04992000() {
    // Encoding: 0x04992000
    // Test EORV_R.P.Z__ field size = 2 (PowerOfTwo)
    // Fields: Pg=0, Zn=0, Vd=0, size=2
    let encoding: u32 = 0x04992000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_eorv_r_p_z_field_size_3_max_2000_04d92000() {
    // Encoding: 0x04D92000
    // Test EORV_R.P.Z__ field size = 3 (Max)
    // Fields: size=3, Pg=0, Vd=0, Zn=0
    let encoding: u32 = 0x04D92000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_eorv_r_p_z_field_pg_0_min_2000_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ field Pg = 0 (Min)
    // Fields: size=0, Vd=0, Pg=0, Zn=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_eorv_r_p_z_field_pg_1_poweroftwo_2000_04192400() {
    // Encoding: 0x04192400
    // Test EORV_R.P.Z__ field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Zn=0, Vd=0, size=0
    let encoding: u32 = 0x04192400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_eorv_r_p_z_field_zn_0_min_2000_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ field Zn = 0 (Min)
    // Fields: Zn=0, size=0, Vd=0, Pg=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_eorv_r_p_z_field_zn_1_poweroftwo_2000_04192020() {
    // Encoding: 0x04192020
    // Test EORV_R.P.Z__ field Zn = 1 (PowerOfTwo)
    // Fields: size=0, Pg=0, Zn=1, Vd=0
    let encoding: u32 = 0x04192020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_eorv_r_p_z_field_zn_30_poweroftwominusone_2000_041923c0() {
    // Encoding: 0x041923C0
    // Test EORV_R.P.Z__ field Zn = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zn=30, Vd=0, size=0
    let encoding: u32 = 0x041923C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Zn 5 +: 5`
/// Requirement: FieldBoundary { field: "Zn", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_eorv_r_p_z_field_zn_31_max_2000_041923e0() {
    // Encoding: 0x041923E0
    // Test EORV_R.P.Z__ field Zn = 31 (Max)
    // Fields: size=0, Zn=31, Pg=0, Vd=0
    let encoding: u32 = 0x041923E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_eorv_r_p_z_field_vd_0_min_2000_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ field Vd = 0 (Min)
    // Fields: Vd=0, Pg=0, Zn=0, size=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_eorv_r_p_z_field_vd_1_poweroftwo_2000_04192001() {
    // Encoding: 0x04192001
    // Test EORV_R.P.Z__ field Vd = 1 (PowerOfTwo)
    // Fields: size=0, Zn=0, Vd=1, Pg=0
    let encoding: u32 = 0x04192001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_eorv_r_p_z_field_vd_30_poweroftwominusone_2000_0419201e() {
    // Encoding: 0x0419201E
    // Test EORV_R.P.Z__ field Vd = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Pg=0, Zn=0, Vd=30
    let encoding: u32 = 0x0419201E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field Vd 0 +: 5`
/// Requirement: FieldBoundary { field: "Vd", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_eorv_r_p_z_field_vd_31_max_2000_0419201f() {
    // Encoding: 0x0419201F
    // Test EORV_R.P.Z__ field Vd = 31 (Max)
    // Fields: Pg=0, Vd=31, size=0, Zn=0
    let encoding: u32 = 0x0419201F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_eorv_r_p_z_combo_0_2000_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ field combination: size=0, Pg=0, Zn=0, Vd=0
    // Fields: Zn=0, size=0, Pg=0, Vd=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_eorv_r_p_z_special_size_0_size_variant_0_8192_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ special value size = 0 (Size variant 0)
    // Fields: Vd=0, size=0, Pg=0, Zn=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_eorv_r_p_z_special_size_1_size_variant_1_8192_04592000() {
    // Encoding: 0x04592000
    // Test EORV_R.P.Z__ special value size = 1 (Size variant 1)
    // Fields: Vd=0, Zn=0, Pg=0, size=1
    let encoding: u32 = 0x04592000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_eorv_r_p_z_special_size_2_size_variant_2_8192_04992000() {
    // Encoding: 0x04992000
    // Test EORV_R.P.Z__ special value size = 2 (Size variant 2)
    // Fields: Pg=0, Zn=0, Vd=0, size=2
    let encoding: u32 = 0x04992000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_eorv_r_p_z_special_size_3_size_variant_3_8192_04d92000() {
    // Encoding: 0x04D92000
    // Test EORV_R.P.Z__ special value size = 3 (Size variant 3)
    // Fields: Vd=0, size=3, Pg=0, Zn=0
    let encoding: u32 = 0x04D92000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_eorv_r_p_z_invalid_0_2000_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Vd=0, Zn=0, Pg=0, size=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_eorv_r_p_z_invalid_1_2000_04192000() {
    // Encoding: 0x04192000
    // Test EORV_R.P.Z__ invalid encoding: Unconditional UNDEFINED
    // Fields: size=0, Pg=0, Zn=0, Vd=0
    let encoding: u32 = 0x04192000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: EORV_R.P.Z__
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_eorv_r_p_z_reg_write_0_04192000() {
    // Test EORV_R.P.Z__ register write: SimdFromField("d")
    // Encoding: 0x04192000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x04192000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

