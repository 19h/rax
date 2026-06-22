//! A64 sve load tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// LDFF1B_Z.P.BZ_D.x32.unscaled Tests
// ============================================================================

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_xs_0_min_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field xs = 0 (Min)
    // Fields: Pg=0, xs=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_xs_1_max_6000_c4406000() {
    // Encoding: 0xC4406000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field xs = 1 (Max)
    // Fields: Pg=0, Zm=0, xs=1, Rn=0, Zt=0
    let encoding: u32 = 0xC4406000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zm_0_min_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zm = 0 (Min)
    // Fields: Zm=0, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zm_1_poweroftwo_6000_c4016000() {
    // Encoding: 0xC4016000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Zt=0, Pg=0, Rn=0, xs=0, Zm=1
    let encoding: u32 = 0xC4016000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zm_30_poweroftwominusone_6000_c41e6000() {
    // Encoding: 0xC41E6000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Zt=0, Pg=0, xs=0, Zm=30
    let encoding: u32 = 0xC41E6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zm_31_max_6000_c41f6000() {
    // Encoding: 0xC41F6000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zm = 31 (Max)
    // Fields: xs=0, Zt=0, Pg=0, Rn=0, Zm=31
    let encoding: u32 = 0xC41F6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_pg_0_min_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Pg = 0 (Min)
    // Fields: Rn=0, Pg=0, Zt=0, Zm=0, xs=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_pg_1_poweroftwo_6000_c4006400() {
    // Encoding: 0xC4006400
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: xs=0, Rn=0, Pg=1, Zm=0, Zt=0
    let encoding: u32 = 0xC4006400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_rn_0_min_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Rn = 0 (Min)
    // Fields: Pg=0, Zm=0, xs=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_rn_1_poweroftwo_6000_c4006020() {
    // Encoding: 0xC4006020
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: Zt=0, Pg=0, Zm=0, xs=0, Rn=1
    let encoding: u32 = 0xC4006020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_rn_30_poweroftwominusone_6000_c40063c0() {
    // Encoding: 0xC40063C0
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Rn=30, Pg=0, Zt=0, Zm=0
    let encoding: u32 = 0xC40063C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_rn_31_max_6000_c40063e0() {
    // Encoding: 0xC40063E0
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Rn = 31 (Max)
    // Fields: Zt=0, Zm=0, xs=0, Rn=31, Pg=0
    let encoding: u32 = 0xC40063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zt_0_min_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zt = 0 (Min)
    // Fields: Pg=0, Zt=0, xs=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zt_1_poweroftwo_6000_c4006001() {
    // Encoding: 0xC4006001
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: Zt=1, Rn=0, Pg=0, xs=0, Zm=0
    let encoding: u32 = 0xC4006001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zt_30_poweroftwominusone_6000_c400601e() {
    // Encoding: 0xC400601E
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    let encoding: u32 = 0xC400601E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_field_zt_31_max_6000_c400601f() {
    // Encoding: 0xC400601F
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field Zt = 31 (Max)
    // Fields: Zt=31, Pg=0, Rn=0, Zm=0, xs=0
    let encoding: u32 = 0xC400601F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_0_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Zm=0, xs=0, Pg=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_1_6000_c4406000() {
    // Encoding: 0xC4406000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=1, Pg=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4406000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_2_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, xs=0, Zm=0, Pg=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_3_6000_c4016000() {
    // Encoding: 0xC4016000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, xs=0, Rn=0, Zm=1
    let encoding: u32 = 0xC4016000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_4_6000_c41e6000() {
    // Encoding: 0xC41E6000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Pg=0, xs=0, Zm=30
    let encoding: u32 = 0xC41E6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_5_6000_c41f6000() {
    // Encoding: 0xC41F6000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Pg=0, Rn=0, Zt=0, Zm=31
    let encoding: u32 = 0xC41F6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_6_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Rn=0, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_7_6000_c4006400() {
    // Encoding: 0xC4006400
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Pg=1, Zm=0, xs=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4006400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_8_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, xs=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_9_6000_c4006020() {
    // Encoding: 0xC4006020
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: xs=0, Rn=1, Zm=0, Pg=0, Zt=0
    let encoding: u32 = 0xC4006020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_10_6000_c40063c0() {
    // Encoding: 0xC40063C0
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rn=30, Pg=0, xs=0, Zt=0, Zm=0
    let encoding: u32 = 0xC40063C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_11_6000_c40063e0() {
    // Encoding: 0xC40063E0
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zt=0, Zm=0, xs=0, Pg=0, Rn=31
    let encoding: u32 = 0xC40063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_12_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zt=0, Zm=0, xs=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_13_6000_c4006001() {
    // Encoding: 0xC4006001
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: xs=0, Rn=0, Pg=0, Zm=0, Zt=1
    let encoding: u32 = 0xC4006001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_14_6000_c400601e() {
    // Encoding: 0xC400601E
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Rn=0, Zt=30, Pg=0, xs=0, Zm=0
    let encoding: u32 = 0xC400601E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_15_6000_c400601f() {
    // Encoding: 0xC400601F
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zm=0, Zt=31, xs=0, Rn=0, Pg=0
    let encoding: u32 = 0xC400601F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_16_6000_c4006420() {
    // Encoding: 0xC4006420
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: xs=0, Zt=0, Zm=0, Pg=1, Rn=1
    let encoding: u32 = 0xC4006420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_combo_17_6000_c4007fe0() {
    // Encoding: 0xC4007FE0
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Zm=0, Rn=31, xs=0, Zt=0
    let encoding: u32 = 0xC4007FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_24576_c40063e0() {
    // Encoding: 0xC40063E0
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Pg=0, Zt=0, Zm=0, Rn=31, xs=0
    let encoding: u32 = 0xC40063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_invalid_0_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: xs=0, Zm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.x32.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldff1b_z_p_bz_d_x32_unscaled_invalid_1_6000_c4006000() {
    // Encoding: 0xC4006000
    // Test LDFF1B_Z.P.BZ_D.x32.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, Rn=0, Zm=0, Pg=0, xs=0
    let encoding: u32 = 0xC4006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_xs_0_min_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field xs = 0 (Min)
    // Fields: Pg=0, Zm=0, Rn=0, Zt=0, xs=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_xs_1_max_6000_84406000() {
    // Encoding: 0x84406000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field xs = 1 (Max)
    // Fields: xs=1, Rn=0, Zt=0, Zm=0, Pg=0
    let encoding: u32 = 0x84406000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zm_0_min_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zm = 0 (Min)
    // Fields: Zt=0, Rn=0, Zm=0, xs=0, Pg=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zm_1_poweroftwo_6000_84016000() {
    // Encoding: 0x84016000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Zm=1, xs=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0x84016000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zm_30_poweroftwominusone_6000_841e6000() {
    // Encoding: 0x841E6000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Pg=0, Rn=0, xs=0, Zt=0
    let encoding: u32 = 0x841E6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zm_31_max_6000_841f6000() {
    // Encoding: 0x841F6000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zm = 31 (Max)
    // Fields: xs=0, Pg=0, Zm=31, Rn=0, Zt=0
    let encoding: u32 = 0x841F6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_pg_0_min_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Pg = 0 (Min)
    // Fields: Zt=0, xs=0, Zm=0, Rn=0, Pg=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_pg_1_poweroftwo_6000_84006400() {
    // Encoding: 0x84006400
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: xs=0, Pg=1, Rn=0, Zt=0, Zm=0
    let encoding: u32 = 0x84006400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_rn_0_min_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Rn = 0 (Min)
    // Fields: Zt=0, Rn=0, xs=0, Zm=0, Pg=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_rn_1_poweroftwo_6000_84006020() {
    // Encoding: 0x84006020
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: xs=0, Pg=0, Zt=0, Zm=0, Rn=1
    let encoding: u32 = 0x84006020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_rn_30_poweroftwominusone_6000_840063c0() {
    // Encoding: 0x840063C0
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Zm=0, Pg=0, Zt=0, Rn=30
    let encoding: u32 = 0x840063C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_rn_31_max_6000_840063e0() {
    // Encoding: 0x840063E0
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Rn = 31 (Max)
    // Fields: Zm=0, Pg=0, xs=0, Rn=31, Zt=0
    let encoding: u32 = 0x840063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zt_0_min_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zt = 0 (Min)
    // Fields: xs=0, Rn=0, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zt_1_poweroftwo_6000_84006001() {
    // Encoding: 0x84006001
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: xs=0, Zm=0, Rn=0, Zt=1, Pg=0
    let encoding: u32 = 0x84006001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zt_30_poweroftwominusone_6000_8400601e() {
    // Encoding: 0x8400601E
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Rn=0, Zt=30, Zm=0, Pg=0
    let encoding: u32 = 0x8400601E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_field_zt_31_max_6000_8400601f() {
    // Encoding: 0x8400601F
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field Zt = 31 (Max)
    // Fields: Zm=0, xs=0, Rn=0, Pg=0, Zt=31
    let encoding: u32 = 0x8400601F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_0_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zm=0, xs=0, Pg=0, Zt=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_1_6000_84406000() {
    // Encoding: 0x84406000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, xs=1, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0x84406000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_2_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Zm=0, Pg=0, xs=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_3_6000_84016000() {
    // Encoding: 0x84016000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zm=1, Pg=0, Rn=0, xs=0, Zt=0
    let encoding: u32 = 0x84016000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_4_6000_841e6000() {
    // Encoding: 0x841E6000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=30, Pg=0, xs=0, Rn=0
    let encoding: u32 = 0x841E6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_5_6000_841f6000() {
    // Encoding: 0x841F6000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Pg=0, Rn=0, Zt=0, Zm=31
    let encoding: u32 = 0x841F6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_6_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Zm=0, Pg=0, xs=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_7_6000_84006400() {
    // Encoding: 0x84006400
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: xs=0, Zm=0, Rn=0, Zt=0, Pg=1
    let encoding: u32 = 0x84006400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_8_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zt=0, Zm=0, xs=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_9_6000_84006020() {
    // Encoding: 0x84006020
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    let encoding: u32 = 0x84006020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_10_6000_840063c0() {
    // Encoding: 0x840063C0
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zt=0, xs=0, Rn=30, Zm=0, Pg=0
    let encoding: u32 = 0x840063C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_11_6000_840063e0() {
    // Encoding: 0x840063E0
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: xs=0, Zm=0, Zt=0, Rn=31, Pg=0
    let encoding: u32 = 0x840063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_12_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Pg=0, Rn=0, xs=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_13_6000_84006001() {
    // Encoding: 0x84006001
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Zm=0, Rn=0, xs=0, Zt=1, Pg=0
    let encoding: u32 = 0x84006001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_14_6000_8400601e() {
    // Encoding: 0x8400601E
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zm=0, Rn=0, Zt=30, Pg=0, xs=0
    let encoding: u32 = 0x8400601E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_15_6000_8400601f() {
    // Encoding: 0x8400601F
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Rn=0, Zt=31, xs=0, Zm=0, Pg=0
    let encoding: u32 = 0x8400601F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_16_6000_84006420() {
    // Encoding: 0x84006420
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Zm=0, Rn=1, xs=0, Pg=1, Zt=0
    let encoding: u32 = 0x84006420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_combo_17_6000_84007fe0() {
    // Encoding: 0x84007FE0
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: xs=0, Zm=0, Zt=0, Pg=31, Rn=31
    let encoding: u32 = 0x84007FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_24576_840063e0() {
    // Encoding: 0x840063E0
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: xs=0, Rn=31, Pg=0, Zt=0, Zm=0
    let encoding: u32 = 0x840063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_invalid_0_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Rn=0, Zm=0, Zt=0, Pg=0, xs=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_S.x32.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldff1b_z_p_bz_s_x32_unscaled_invalid_1_6000_84006000() {
    // Encoding: 0x84006000
    // Test LDFF1B_Z.P.BZ_S.x32.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Zt=0, xs=0, Pg=0, Zm=0
    let encoding: u32 = 0x84006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zm_0_min_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zm = 0 (Min)
    // Fields: Zt=0, Rn=0, Pg=0, Zm=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zm_1_poweroftwo_e000_c441e000() {
    // Encoding: 0xC441E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Zm=1, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC441E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zm_30_poweroftwominusone_e000_c45ee000() {
    // Encoding: 0xC45EE000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xC45EE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zm_31_max_e000_c45fe000() {
    // Encoding: 0xC45FE000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zm = 31 (Max)
    // Fields: Zm=31, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC45FE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_pg_0_min_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Pg = 0 (Min)
    // Fields: Pg=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_pg_1_poweroftwo_e000_c440e400() {
    // Encoding: 0xC440E400
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: Zt=0, Pg=1, Zm=0, Rn=0
    let encoding: u32 = 0xC440E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_rn_0_min_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Rn = 0 (Min)
    // Fields: Rn=0, Zt=0, Zm=0, Pg=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_rn_1_poweroftwo_e000_c440e020() {
    // Encoding: 0xC440E020
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: Zt=0, Zm=0, Pg=0, Rn=1
    let encoding: u32 = 0xC440E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_rn_30_poweroftwominusone_e000_c440e3c0() {
    // Encoding: 0xC440E3C0
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Pg=0, Rn=30, Zt=0
    let encoding: u32 = 0xC440E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_rn_31_max_e000_c440e3e0() {
    // Encoding: 0xC440E3E0
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Rn = 31 (Max)
    // Fields: Zt=0, Rn=31, Pg=0, Zm=0
    let encoding: u32 = 0xC440E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zt_0_min_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zt = 0 (Min)
    // Fields: Zt=0, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zt_1_poweroftwo_e000_c440e001() {
    // Encoding: 0xC440E001
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: Zm=0, Pg=0, Zt=1, Rn=0
    let encoding: u32 = 0xC440E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zt_30_poweroftwominusone_e000_c440e01e() {
    // Encoding: 0xC440E01E
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=30, Zm=0, Rn=0, Pg=0
    let encoding: u32 = 0xC440E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_field_zt_31_max_e000_c440e01f() {
    // Encoding: 0xC440E01F
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field Zt = 31 (Max)
    // Fields: Zt=31, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0xC440E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_0_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=0, Zt=0, Rn=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_1_e000_c441e000() {
    // Encoding: 0xC441E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zm=1, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xC441E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_2_e000_c45ee000() {
    // Encoding: 0xC45EE000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Zm=30, Rn=0
    let encoding: u32 = 0xC45EE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_3_e000_c45fe000() {
    // Encoding: 0xC45FE000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zm=31, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xC45FE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_4_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=0, Zt=0, Rn=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_5_e000_c440e400() {
    // Encoding: 0xC440E400
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Zt=0, Pg=1, Rn=0, Zm=0
    let encoding: u32 = 0xC440E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_6_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_7_e000_c440e020() {
    // Encoding: 0xC440E020
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0xC440E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_8_e000_c440e3c0() {
    // Encoding: 0xC440E3C0
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rn=30, Zm=0, Pg=0, Zt=0
    let encoding: u32 = 0xC440E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_9_e000_c440e3e0() {
    // Encoding: 0xC440E3E0
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zm=0, Pg=0, Zt=0, Rn=31
    let encoding: u32 = 0xC440E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_10_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=0, Zt=0, Rn=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_11_e000_c440e001() {
    // Encoding: 0xC440E001
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Zt=1, Zm=0, Pg=0, Rn=0
    let encoding: u32 = 0xC440E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_12_e000_c440e01e() {
    // Encoding: 0xC440E01E
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Pg=0, Zm=0, Rn=0, Zt=30
    let encoding: u32 = 0xC440E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_13_e000_c440e01f() {
    // Encoding: 0xC440E01F
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, Zt=31, Rn=0, Zm=0
    let encoding: u32 = 0xC440E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_14_e000_c440e420() {
    // Encoding: 0xC440E420
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Zm=0, Zt=0, Pg=1, Rn=1
    let encoding: u32 = 0xC440E420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_combo_15_e000_c440ffe0() {
    // Encoding: 0xC440FFE0
    // Test LDFF1B_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Zt=0, Zm=0, Rn=31
    let encoding: u32 = 0xC440FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_57344_c440e3e0() {
    // Encoding: 0xC440E3E0
    // Test LDFF1B_Z.P.BZ_D.64.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0xC440E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_invalid_0_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Zm=0, Rn=0, Zt=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDFF1B_Z.P.BZ_D.64.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldff1b_z_p_bz_d_64_unscaled_invalid_1_e000_c440e000() {
    // Encoding: 0xC440E000
    // Test LDFF1B_Z.P.BZ_D.64.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Zm=0, Pg=0, Zt=0
    let encoding: u32 = 0xC440E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD1RQW_Z.P.BR_Contiguous Tests
// ============================================================================

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rm_0_min_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field Rm = 0 (Min)
    // Fields: Rm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rm_1_poweroftwo_0_a5010000() {
    // Encoding: 0xA5010000
    // Test LD1RQW_Z.P.BR_Contiguous field Rm = 1 (PowerOfTwo)
    // Fields: Pg=0, Rn=0, Zt=0, Rm=1
    let encoding: u32 = 0xA5010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rm_30_poweroftwominusone_0_a51e0000() {
    // Encoding: 0xA51E0000
    // Test LD1RQW_Z.P.BR_Contiguous field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Rm=30, Rn=0, Pg=0
    let encoding: u32 = 0xA51E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rm_31_max_0_a51f0000() {
    // Encoding: 0xA51F0000
    // Test LD1RQW_Z.P.BR_Contiguous field Rm = 31 (Max)
    // Fields: Rm=31, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA51F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_pg_0_min_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field Pg = 0 (Min)
    // Fields: Pg=0, Rm=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_pg_1_poweroftwo_0_a5000400() {
    // Encoding: 0xA5000400
    // Test LD1RQW_Z.P.BR_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Zt=0, Rn=0, Rm=0, Pg=1
    let encoding: u32 = 0xA5000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rn_0_min_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field Rn = 0 (Min)
    // Fields: Pg=0, Rn=0, Rm=0, Zt=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rn_1_poweroftwo_0_a5000020() {
    // Encoding: 0xA5000020
    // Test LD1RQW_Z.P.BR_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=1, Zt=0, Pg=0
    let encoding: u32 = 0xA5000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rn_30_poweroftwominusone_0_a50003c0() {
    // Encoding: 0xA50003C0
    // Test LD1RQW_Z.P.BR_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Pg=0, Zt=0, Rn=30
    let encoding: u32 = 0xA50003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_rn_31_max_0_a50003e0() {
    // Encoding: 0xA50003E0
    // Test LD1RQW_Z.P.BR_Contiguous field Rn = 31 (Max)
    // Fields: Zt=0, Pg=0, Rn=31, Rm=0
    let encoding: u32 = 0xA50003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_zt_0_min_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field Zt = 0 (Min)
    // Fields: Zt=0, Pg=0, Rn=0, Rm=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_zt_1_poweroftwo_0_a5000001() {
    // Encoding: 0xA5000001
    // Test LD1RQW_Z.P.BR_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, Zt=1, Pg=0
    let encoding: u32 = 0xA5000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_zt_30_poweroftwominusone_0_a500001e() {
    // Encoding: 0xA500001E
    // Test LD1RQW_Z.P.BR_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zt=30, Rm=0, Rn=0
    let encoding: u32 = 0xA500001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1rqw_z_p_br_contiguous_field_zt_31_max_0_a500001f() {
    // Encoding: 0xA500001F
    // Test LD1RQW_Z.P.BR_Contiguous field Zt = 31 (Max)
    // Fields: Rn=0, Pg=0, Rm=0, Zt=31
    let encoding: u32 = 0xA500001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_0_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Rm=0, Zt=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (register index 1 (second register))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_1_0_a5010000() {
    // Encoding: 0xA5010000
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Rm=1, Pg=0
    let encoding: u32 = 0xA5010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_2_0_a51e0000() {
    // Encoding: 0xA51E0000
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, Rm=30, Zt=0
    let encoding: u32 = 0xA51E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (register index 31 (special))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_3_0_a51f0000() {
    // Encoding: 0xA51F0000
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rm=31, Pg=0, Rn=0
    let encoding: u32 = 0xA51F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_4_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Rm=0, Rn=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_5_0_a5000400() {
    // Encoding: 0xA5000400
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rm=0, Pg=1, Zt=0, Rn=0
    let encoding: u32 = 0xA5000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_6_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_7_0_a5000020() {
    // Encoding: 0xA5000020
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Rn=1, Rm=0, Zt=0
    let encoding: u32 = 0xA5000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_8_0_a50003c0() {
    // Encoding: 0xA50003C0
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zt=0, Rn=30, Pg=0, Rm=0
    let encoding: u32 = 0xA50003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_9_0_a50003e0() {
    // Encoding: 0xA50003E0
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=31, Zt=0
    // Fields: Rm=0, Pg=0, Rn=31, Zt=0
    let encoding: u32 = 0xA50003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_10_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_11_0_a5000001() {
    // Encoding: 0xA5000001
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=1
    // Fields: Zt=1, Rn=0, Pg=0, Rm=0
    let encoding: u32 = 0xA5000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_12_0_a500001e() {
    // Encoding: 0xA500001E
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=30
    // Fields: Rm=0, Pg=0, Zt=30, Rn=0
    let encoding: u32 = 0xA500001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_13_0_a500001f() {
    // Encoding: 0xA500001F
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=31
    // Fields: Rn=0, Zt=31, Pg=0, Rm=0
    let encoding: u32 = 0xA500001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Pg=1 (same register test (reg=1))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_14_0_a5010400() {
    // Encoding: 0xA5010400
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=1, Pg=1, Rn=0, Zt=0
    // Fields: Pg=1, Rn=0, Zt=0, Rm=1
    let encoding: u32 = 0xA5010400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Pg=31 (same register test (reg=31))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_15_0_a51f1c00() {
    // Encoding: 0xA51F1C00
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=31, Pg=31, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Rm=31, Pg=31
    let encoding: u32 = 0xA51F1C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_16_0_a5010020() {
    // Encoding: 0xA5010020
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Zt=0, Pg=0, Rm=1
    let encoding: u32 = 0xA5010020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_17_0_a51f03e0() {
    // Encoding: 0xA51F03E0
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=31, Zt=0
    // Fields: Rm=31, Zt=0, Pg=0, Rn=31
    let encoding: u32 = 0xA51F03E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_18_0_a5000420() {
    // Encoding: 0xA5000420
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=1, Zt=0
    // Fields: Zt=0, Rm=0, Rn=1, Pg=1
    let encoding: u32 = 0xA5000420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field combination 19`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1rqw_z_p_br_contiguous_combo_19_0_a5001fe0() {
    // Encoding: 0xA5001FE0
    // Test LD1RQW_Z.P.BR_Contiguous field combination: Rm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Rm=0, Zt=0, Rn=31
    let encoding: u32 = 0xA5001FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1rqw_z_p_br_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_0_a50003e0() {
    // Encoding: 0xA50003E0
    // Test LD1RQW_Z.P.BR_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zt=0, Rm=0, Pg=0, Rn=31
    let encoding: u32 = 0xA50003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1rqw_z_p_br_contiguous_invalid_0_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Rm=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1rqw_z_p_br_contiguous_invalid_1_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rm=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Rm\" }), rhs: LitBits([true, true, true, true, true]) }" }
/// triggers Undefined
#[test]
fn test_ld1rqw_z_p_br_contiguous_invalid_2_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous invalid encoding: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }
    // Fields: Zt=0, Rn=0, Rm=0, Pg=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQW_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1rqw_z_p_br_contiguous_invalid_3_0_a5000000() {
    // Encoding: 0xA5000000
    // Test LD1RQW_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, Pg=0, Rm=0, Rn=0
    let encoding: u32 = 0xA5000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD2W_Z.P.BR_Contiguous Tests
// ============================================================================

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rm_0_min_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field Rm = 0 (Min)
    // Fields: Pg=0, Zt=0, Rn=0, Rm=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rm_1_poweroftwo_c000_a521c000() {
    // Encoding: 0xA521C000
    // Test LD2W_Z.P.BR_Contiguous field Rm = 1 (PowerOfTwo)
    // Fields: Zt=0, Pg=0, Rm=1, Rn=0
    let encoding: u32 = 0xA521C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rm_30_poweroftwominusone_c000_a53ec000() {
    // Encoding: 0xA53EC000
    // Test LD2W_Z.P.BR_Contiguous field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA53EC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rm_31_max_c000_a53fc000() {
    // Encoding: 0xA53FC000
    // Test LD2W_Z.P.BR_Contiguous field Rm = 31 (Max)
    // Fields: Pg=0, Rn=0, Rm=31, Zt=0
    let encoding: u32 = 0xA53FC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld2w_z_p_br_contiguous_field_pg_0_min_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field Pg = 0 (Min)
    // Fields: Rn=0, Pg=0, Rm=0, Zt=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld2w_z_p_br_contiguous_field_pg_1_poweroftwo_c000_a520c400() {
    // Encoding: 0xA520C400
    // Test LD2W_Z.P.BR_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Zt=0, Rm=0, Rn=0
    let encoding: u32 = 0xA520C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rn_0_min_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field Rn = 0 (Min)
    // Fields: Pg=0, Zt=0, Rn=0, Rm=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rn_1_poweroftwo_c000_a520c020() {
    // Encoding: 0xA520C020
    // Test LD2W_Z.P.BR_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Rn=1, Rm=0, Zt=0
    let encoding: u32 = 0xA520C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rn_30_poweroftwominusone_c000_a520c3c0() {
    // Encoding: 0xA520C3C0
    // Test LD2W_Z.P.BR_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Pg=0, Rn=30, Rm=0
    let encoding: u32 = 0xA520C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld2w_z_p_br_contiguous_field_rn_31_max_c000_a520c3e0() {
    // Encoding: 0xA520C3E0
    // Test LD2W_Z.P.BR_Contiguous field Rn = 31 (Max)
    // Fields: Rn=31, Pg=0, Zt=0, Rm=0
    let encoding: u32 = 0xA520C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld2w_z_p_br_contiguous_field_zt_0_min_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field Zt = 0 (Min)
    // Fields: Zt=0, Rm=0, Rn=0, Pg=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld2w_z_p_br_contiguous_field_zt_1_poweroftwo_c000_a520c001() {
    // Encoding: 0xA520C001
    // Test LD2W_Z.P.BR_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Pg=0, Rn=0, Zt=1, Rm=0
    let encoding: u32 = 0xA520C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld2w_z_p_br_contiguous_field_zt_30_poweroftwominusone_c000_a520c01e() {
    // Encoding: 0xA520C01E
    // Test LD2W_Z.P.BR_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=30, Rm=0, Pg=0, Rn=0
    let encoding: u32 = 0xA520C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld2w_z_p_br_contiguous_field_zt_31_max_c000_a520c01f() {
    // Encoding: 0xA520C01F
    // Test LD2W_Z.P.BR_Contiguous field Zt = 31 (Max)
    // Fields: Pg=0, Zt=31, Rn=0, Rm=0
    let encoding: u32 = 0xA520C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_0_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Pg=0, Rm=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (register index 1 (second register))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_1_c000_a521c000() {
    // Encoding: 0xA521C000
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=0, Zt=0
    // Fields: Rm=1, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA521C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_2_c000_a53ec000() {
    // Encoding: 0xA53EC000
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=30, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rm=30, Pg=0, Rn=0
    let encoding: u32 = 0xA53EC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (register index 31 (special))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_3_c000_a53fc000() {
    // Encoding: 0xA53FC000
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=0, Zt=0
    // Fields: Rm=31, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA53FC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_4_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Rm=0, Zt=0, Pg=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_5_c000_a520c400() {
    // Encoding: 0xA520C400
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rm=0, Zt=0, Rn=0, Pg=1
    let encoding: u32 = 0xA520C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_6_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_7_c000_a520c020() {
    // Encoding: 0xA520C020
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Rm=0, Zt=0, Pg=0
    let encoding: u32 = 0xA520C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_8_c000_a520c3c0() {
    // Encoding: 0xA520C3C0
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rm=0, Rn=30, Zt=0, Pg=0
    let encoding: u32 = 0xA520C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_9_c000_a520c3e0() {
    // Encoding: 0xA520C3E0
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=31, Zt=0
    // Fields: Pg=0, Rm=0, Zt=0, Rn=31
    let encoding: u32 = 0xA520C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld2w_z_p_br_contiguous_combo_10_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rm=0, Rn=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld2w_z_p_br_contiguous_combo_11_c000_a520c001() {
    // Encoding: 0xA520C001
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Rm=0, Zt=1, Rn=0
    let encoding: u32 = 0xA520C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld2w_z_p_br_contiguous_combo_12_c000_a520c01e() {
    // Encoding: 0xA520C01E
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=30
    // Fields: Rm=0, Pg=0, Rn=0, Zt=30
    let encoding: u32 = 0xA520C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld2w_z_p_br_contiguous_combo_13_c000_a520c01f() {
    // Encoding: 0xA520C01F
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, Rm=0, Rn=0, Zt=31
    let encoding: u32 = 0xA520C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Pg=1 (same register test (reg=1))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_14_c000_a521c400() {
    // Encoding: 0xA521C400
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=1, Pg=1, Rn=0, Zt=0
    // Fields: Rm=1, Pg=1, Rn=0, Zt=0
    let encoding: u32 = 0xA521C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Pg=31 (same register test (reg=31))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_15_c000_a53fdc00() {
    // Encoding: 0xA53FDC00
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=31, Pg=31, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Rm=31, Pg=31
    let encoding: u32 = 0xA53FDC00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_16_c000_a521c020() {
    // Encoding: 0xA521C020
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Rm=1, Rn=1, Zt=0
    let encoding: u32 = 0xA521C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_17_c000_a53fc3e0() {
    // Encoding: 0xA53FC3E0
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=31, Zt=0
    // Fields: Rm=31, Pg=0, Rn=31, Zt=0
    let encoding: u32 = 0xA53FC3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_18_c000_a520c420() {
    // Encoding: 0xA520C420
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=1, Zt=0
    // Fields: Rm=0, Pg=1, Rn=1, Zt=0
    let encoding: u32 = 0xA520C420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field combination 19`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld2w_z_p_br_contiguous_combo_19_c000_a520dfe0() {
    // Encoding: 0xA520DFE0
    // Test LD2W_Z.P.BR_Contiguous field combination: Rm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Rn=31, Rm=0, Zt=0
    let encoding: u32 = 0xA520DFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld2w_z_p_br_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_49152_a520c3e0() {
    // Encoding: 0xA520C3E0
    // Test LD2W_Z.P.BR_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xA520C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld2w_z_p_br_contiguous_invalid_0_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Rm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld2w_z_p_br_contiguous_invalid_1_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, Rm=0, Pg=0, Rn=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Rm\" }), rhs: LitBits([true, true, true, true, true]) }" }
/// triggers Undefined
#[test]
fn test_ld2w_z_p_br_contiguous_invalid_2_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous invalid encoding: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }
    // Fields: Rm=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2W_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld2w_z_p_br_contiguous_invalid_3_c000_a520c000() {
    // Encoding: 0xA520C000
    // Test LD2W_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA520C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LDNF1SB_Z.P.BI_S16 Tests
// ============================================================================

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_imm4_0_zero_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field imm4 = 0 (Zero)
    // Fields: Pg=0, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_imm4_1_poweroftwo_a000_a5d1a000() {
    // Encoding: 0xA5D1A000
    // Test LDNF1SB_Z.P.BI_S16 field imm4 = 1 (PowerOfTwo)
    // Fields: imm4=1, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5D1A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_imm4_3_poweroftwominusone_a000_a5d3a000() {
    // Encoding: 0xA5D3A000
    // Test LDNF1SB_Z.P.BI_S16 field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: Pg=0, imm4=3, Rn=0, Zt=0
    let encoding: u32 = 0xA5D3A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_imm4_4_poweroftwo_a000_a5d4a000() {
    // Encoding: 0xA5D4A000
    // Test LDNF1SB_Z.P.BI_S16 field imm4 = 4 (PowerOfTwo)
    // Fields: Rn=0, Zt=0, Pg=0, imm4=4
    let encoding: u32 = 0xA5D4A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_imm4_7_poweroftwominusone_a000_a5d7a000() {
    // Encoding: 0xA5D7A000
    // Test LDNF1SB_Z.P.BI_S16 field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: Pg=0, imm4=7, Rn=0, Zt=0
    let encoding: u32 = 0xA5D7A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_imm4_8_poweroftwo_a000_a5d8a000() {
    // Encoding: 0xA5D8A000
    // Test LDNF1SB_Z.P.BI_S16 field imm4 = 8 (PowerOfTwo)
    // Fields: imm4=8, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5D8A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_imm4_15_max_a000_a5dfa000() {
    // Encoding: 0xA5DFA000
    // Test LDNF1SB_Z.P.BI_S16 field imm4 = 15 (Max)
    // Fields: Zt=0, Rn=0, Pg=0, imm4=15
    let encoding: u32 = 0xA5DFA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_pg_0_min_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field Pg = 0 (Min)
    // Fields: imm4=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_pg_1_poweroftwo_a000_a5d0a400() {
    // Encoding: 0xA5D0A400
    // Test LDNF1SB_Z.P.BI_S16 field Pg = 1 (PowerOfTwo)
    // Fields: imm4=0, Zt=0, Rn=0, Pg=1
    let encoding: u32 = 0xA5D0A400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_rn_0_min_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field Rn = 0 (Min)
    // Fields: Rn=0, imm4=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_rn_1_poweroftwo_a000_a5d0a020() {
    // Encoding: 0xA5D0A020
    // Test LDNF1SB_Z.P.BI_S16 field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, imm4=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5D0A020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_rn_30_poweroftwominusone_a000_a5d0a3c0() {
    // Encoding: 0xA5D0A3C0
    // Test LDNF1SB_Z.P.BI_S16 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, imm4=0, Rn=30, Zt=0
    let encoding: u32 = 0xA5D0A3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_rn_31_max_a000_a5d0a3e0() {
    // Encoding: 0xA5D0A3E0
    // Test LDNF1SB_Z.P.BI_S16 field Rn = 31 (Max)
    // Fields: imm4=0, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xA5D0A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_zt_0_min_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field Zt = 0 (Min)
    // Fields: imm4=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_zt_1_poweroftwo_a000_a5d0a001() {
    // Encoding: 0xA5D0A001
    // Test LDNF1SB_Z.P.BI_S16 field Zt = 1 (PowerOfTwo)
    // Fields: Zt=1, Rn=0, imm4=0, Pg=0
    let encoding: u32 = 0xA5D0A001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_zt_30_poweroftwominusone_a000_a5d0a01e() {
    // Encoding: 0xA5D0A01E
    // Test LDNF1SB_Z.P.BI_S16 field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=30, Pg=0, imm4=0, Rn=0
    let encoding: u32 = 0xA5D0A01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldnf1sb_z_p_bi_s16_field_zt_31_max_a000_a5d0a01f() {
    // Encoding: 0xA5D0A01F
    // Test LDNF1SB_Z.P.BI_S16 field Zt = 31 (Max)
    // Fields: Pg=0, imm4=0, Rn=0, Zt=31
    let encoding: u32 = 0xA5D0A01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=0 (immediate value 0)
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_0_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, imm4=0, Rn=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=1 (immediate value 1)
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_1_a000_a5d1a000() {
    // Encoding: 0xA5D1A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=1, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, imm4=1, Pg=0, Rn=0
    let encoding: u32 = 0xA5D1A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=3 (2^2 - 1 = 3)
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_2_a000_a5d3a000() {
    // Encoding: 0xA5D3A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=3, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zt=0, imm4=3
    let encoding: u32 = 0xA5D3A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=4 (power of 2 (2^2 = 4))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_3_a000_a5d4a000() {
    // Encoding: 0xA5D4A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=4, Pg=0, Rn=0, Zt=0
    // Fields: imm4=4, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5D4A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=7 (immediate midpoint (7))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_4_a000_a5d7a000() {
    // Encoding: 0xA5D7A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=7, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Pg=0, imm4=7
    let encoding: u32 = 0xA5D7A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=8 (power of 2 (2^3 = 8))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_5_a000_a5d8a000() {
    // Encoding: 0xA5D8A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=8, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, imm4=8, Rn=0
    let encoding: u32 = 0xA5D8A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=15 (maximum immediate (15))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_6_a000_a5dfa000() {
    // Encoding: 0xA5DFA000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=15, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=15, Pg=0, Zt=0
    let encoding: u32 = 0xA5DFA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_7_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_8_a000_a5d0a400() {
    // Encoding: 0xA5D0A400
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=1, Rn=0, Zt=0
    // Fields: imm4=0, Rn=0, Pg=1, Zt=0
    let encoding: u32 = 0xA5D0A400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_9_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_10_a000_a5d0a020() {
    // Encoding: 0xA5D0A020
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=1, Zt=0
    // Fields: imm4=0, Pg=0, Rn=1, Zt=0
    let encoding: u32 = 0xA5D0A020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_11_a000_a5d0a3c0() {
    // Encoding: 0xA5D0A3C0
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=30, Zt=0
    // Fields: Pg=0, Zt=0, Rn=30, imm4=0
    let encoding: u32 = 0xA5D0A3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_12_a000_a5d0a3e0() {
    // Encoding: 0xA5D0A3E0
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=31, Zt=0
    // Fields: imm4=0, Rn=31, Pg=0, Zt=0
    let encoding: u32 = 0xA5D0A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_13_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_14_a000_a5d0a001() {
    // Encoding: 0xA5D0A001
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Rn=0, imm4=0, Zt=1
    let encoding: u32 = 0xA5D0A001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_15_a000_a5d0a01e() {
    // Encoding: 0xA5D0A01E
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=0, Zt=30
    // Fields: Rn=0, Pg=0, imm4=0, Zt=30
    let encoding: u32 = 0xA5D0A01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_16_a000_a5d0a01f() {
    // Encoding: 0xA5D0A01F
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=0, Rn=0, Zt=31
    // Fields: imm4=0, Zt=31, Rn=0, Pg=0
    let encoding: u32 = 0xA5D0A01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_17_a000_a5d0a420() {
    // Encoding: 0xA5D0A420
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=1, Rn=1, Zt=0
    // Fields: Zt=0, Rn=1, Pg=1, imm4=0
    let encoding: u32 = 0xA5D0A420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldnf1sb_z_p_bi_s16_combo_18_a000_a5d0bfe0() {
    // Encoding: 0xA5D0BFE0
    // Test LDNF1SB_Z.P.BI_S16 field combination: imm4=0, Pg=31, Rn=31, Zt=0
    // Fields: Rn=31, Pg=31, imm4=0, Zt=0
    let encoding: u32 = 0xA5D0BFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldnf1sb_z_p_bi_s16_special_rn_31_stack_pointer_sp_may_require_alignment_40960_a5d1a3e0() {
    // Encoding: 0xA5D1A3E0
    // Test LDNF1SB_Z.P.BI_S16 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zt=0, imm4=1, Pg=0, Rn=31
    let encoding: u32 = 0xA5D1A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldnf1sb_z_p_bi_s16_invalid_0_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zt=0, Rn=0, imm4=0, Pg=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S16
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldnf1sb_z_p_bi_s16_invalid_1_a000_a5d0a000() {
    // Encoding: 0xA5D0A000
    // Test LDNF1SB_Z.P.BI_S16 invalid encoding: Unconditional UNDEFINED
    // Fields: imm4=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5D0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_imm4_0_zero_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field imm4 = 0 (Zero)
    // Fields: Zt=0, Rn=0, Pg=0, imm4=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_imm4_1_poweroftwo_a000_a5b1a000() {
    // Encoding: 0xA5B1A000
    // Test LDNF1SB_Z.P.BI_S32 field imm4 = 1 (PowerOfTwo)
    // Fields: Rn=0, Pg=0, imm4=1, Zt=0
    let encoding: u32 = 0xA5B1A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_imm4_3_poweroftwominusone_a000_a5b3a000() {
    // Encoding: 0xA5B3A000
    // Test LDNF1SB_Z.P.BI_S32 field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: imm4=3, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5B3A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_imm4_4_poweroftwo_a000_a5b4a000() {
    // Encoding: 0xA5B4A000
    // Test LDNF1SB_Z.P.BI_S32 field imm4 = 4 (PowerOfTwo)
    // Fields: Zt=0, Rn=0, Pg=0, imm4=4
    let encoding: u32 = 0xA5B4A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_imm4_7_poweroftwominusone_a000_a5b7a000() {
    // Encoding: 0xA5B7A000
    // Test LDNF1SB_Z.P.BI_S32 field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Pg=0, imm4=7, Zt=0
    let encoding: u32 = 0xA5B7A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_imm4_8_poweroftwo_a000_a5b8a000() {
    // Encoding: 0xA5B8A000
    // Test LDNF1SB_Z.P.BI_S32 field imm4 = 8 (PowerOfTwo)
    // Fields: imm4=8, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA5B8A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_imm4_15_max_a000_a5bfa000() {
    // Encoding: 0xA5BFA000
    // Test LDNF1SB_Z.P.BI_S32 field imm4 = 15 (Max)
    // Fields: imm4=15, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5BFA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_pg_0_min_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field Pg = 0 (Min)
    // Fields: Pg=0, imm4=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_pg_1_poweroftwo_a000_a5b0a400() {
    // Encoding: 0xA5B0A400
    // Test LDNF1SB_Z.P.BI_S32 field Pg = 1 (PowerOfTwo)
    // Fields: imm4=0, Rn=0, Pg=1, Zt=0
    let encoding: u32 = 0xA5B0A400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_rn_0_min_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field Rn = 0 (Min)
    // Fields: Rn=0, Zt=0, imm4=0, Pg=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_rn_1_poweroftwo_a000_a5b0a020() {
    // Encoding: 0xA5B0A020
    // Test LDNF1SB_Z.P.BI_S32 field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Zt=0, Pg=0, imm4=0
    let encoding: u32 = 0xA5B0A020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_rn_30_poweroftwominusone_a000_a5b0a3c0() {
    // Encoding: 0xA5B0A3C0
    // Test LDNF1SB_Z.P.BI_S32 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Rn=30, Pg=0, Zt=0
    let encoding: u32 = 0xA5B0A3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_rn_31_max_a000_a5b0a3e0() {
    // Encoding: 0xA5B0A3E0
    // Test LDNF1SB_Z.P.BI_S32 field Rn = 31 (Max)
    // Fields: Zt=0, Pg=0, Rn=31, imm4=0
    let encoding: u32 = 0xA5B0A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_zt_0_min_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field Zt = 0 (Min)
    // Fields: Zt=0, imm4=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_zt_1_poweroftwo_a000_a5b0a001() {
    // Encoding: 0xA5B0A001
    // Test LDNF1SB_Z.P.BI_S32 field Zt = 1 (PowerOfTwo)
    // Fields: Rn=0, imm4=0, Pg=0, Zt=1
    let encoding: u32 = 0xA5B0A001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_zt_30_poweroftwominusone_a000_a5b0a01e() {
    // Encoding: 0xA5B0A01E
    // Test LDNF1SB_Z.P.BI_S32 field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=30, Pg=0, imm4=0, Rn=0
    let encoding: u32 = 0xA5B0A01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldnf1sb_z_p_bi_s32_field_zt_31_max_a000_a5b0a01f() {
    // Encoding: 0xA5B0A01F
    // Test LDNF1SB_Z.P.BI_S32 field Zt = 31 (Max)
    // Fields: Rn=0, Zt=31, Pg=0, imm4=0
    let encoding: u32 = 0xA5B0A01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=0 (immediate value 0)
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_0_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=1 (immediate value 1)
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_1_a000_a5b1a000() {
    // Encoding: 0xA5B1A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, imm4=1, Rn=0, Zt=0
    let encoding: u32 = 0xA5B1A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=3 (2^2 - 1 = 3)
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_2_a000_a5b3a000() {
    // Encoding: 0xA5B3A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=3, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zt=0, imm4=3
    let encoding: u32 = 0xA5B3A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=4 (power of 2 (2^2 = 4))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_3_a000_a5b4a000() {
    // Encoding: 0xA5B4A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=4, Pg=0, Rn=0, Zt=0
    // Fields: imm4=4, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5B4A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=7 (immediate midpoint (7))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_4_a000_a5b7a000() {
    // Encoding: 0xA5B7A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=7, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=7, Zt=0, Pg=0
    let encoding: u32 = 0xA5B7A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=8 (power of 2 (2^3 = 8))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_5_a000_a5b8a000() {
    // Encoding: 0xA5B8A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=8, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, imm4=8, Zt=0, Rn=0
    let encoding: u32 = 0xA5B8A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=15 (maximum immediate (15))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_6_a000_a5bfa000() {
    // Encoding: 0xA5BFA000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=15, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, Zt=0, imm4=15
    let encoding: u32 = 0xA5BFA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_7_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, imm4=0, Rn=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_8_a000_a5b0a400() {
    // Encoding: 0xA5B0A400
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=1, Rn=0, Zt=0
    // Fields: Pg=1, imm4=0, Zt=0, Rn=0
    let encoding: u32 = 0xA5B0A400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_9_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Rn=0, imm4=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_10_a000_a5b0a020() {
    // Encoding: 0xA5B0A020
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Pg=0, Zt=0, imm4=0
    let encoding: u32 = 0xA5B0A020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_11_a000_a5b0a3c0() {
    // Encoding: 0xA5B0A3C0
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=30, Zt=0
    // Fields: Zt=0, imm4=0, Rn=30, Pg=0
    let encoding: u32 = 0xA5B0A3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_12_a000_a5b0a3e0() {
    // Encoding: 0xA5B0A3E0
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=31, Zt=0
    // Fields: imm4=0, Rn=31, Pg=0, Zt=0
    let encoding: u32 = 0xA5B0A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_13_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_14_a000_a5b0a001() {
    // Encoding: 0xA5B0A001
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=0, Zt=1
    // Fields: imm4=0, Pg=0, Zt=1, Rn=0
    let encoding: u32 = 0xA5B0A001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_15_a000_a5b0a01e() {
    // Encoding: 0xA5B0A01E
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=0, Zt=30
    // Fields: imm4=0, Zt=30, Pg=0, Rn=0
    let encoding: u32 = 0xA5B0A01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_16_a000_a5b0a01f() {
    // Encoding: 0xA5B0A01F
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=0, Rn=0, Zt=31
    // Fields: imm4=0, Rn=0, Zt=31, Pg=0
    let encoding: u32 = 0xA5B0A01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_17_a000_a5b0a420() {
    // Encoding: 0xA5B0A420
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=1, Rn=1, Zt=0
    // Fields: Pg=1, Zt=0, imm4=0, Rn=1
    let encoding: u32 = 0xA5B0A420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldnf1sb_z_p_bi_s32_combo_18_a000_a5b0bfe0() {
    // Encoding: 0xA5B0BFE0
    // Test LDNF1SB_Z.P.BI_S32 field combination: imm4=0, Pg=31, Rn=31, Zt=0
    // Fields: Rn=31, Pg=31, Zt=0, imm4=0
    let encoding: u32 = 0xA5B0BFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldnf1sb_z_p_bi_s32_special_rn_31_stack_pointer_sp_may_require_alignment_40960_a5b1a3e0() {
    // Encoding: 0xA5B1A3E0
    // Test LDNF1SB_Z.P.BI_S32 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zt=0, imm4=1, Rn=31, Pg=0
    let encoding: u32 = 0xA5B1A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldnf1sb_z_p_bi_s32_invalid_0_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: imm4=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S32
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldnf1sb_z_p_bi_s32_invalid_1_a000_a5b0a000() {
    // Encoding: 0xA5B0A000
    // Test LDNF1SB_Z.P.BI_S32 invalid encoding: Unconditional UNDEFINED
    // Fields: imm4=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5B0A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_imm4_0_zero_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field imm4 = 0 (Zero)
    // Fields: Zt=0, Rn=0, Pg=0, imm4=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_imm4_1_poweroftwo_a000_a591a000() {
    // Encoding: 0xA591A000
    // Test LDNF1SB_Z.P.BI_S64 field imm4 = 1 (PowerOfTwo)
    // Fields: Rn=0, Pg=0, imm4=1, Zt=0
    let encoding: u32 = 0xA591A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_imm4_3_poweroftwominusone_a000_a593a000() {
    // Encoding: 0xA593A000
    // Test LDNF1SB_Z.P.BI_S64 field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Pg=0, imm4=3, Zt=0
    let encoding: u32 = 0xA593A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_imm4_4_poweroftwo_a000_a594a000() {
    // Encoding: 0xA594A000
    // Test LDNF1SB_Z.P.BI_S64 field imm4 = 4 (PowerOfTwo)
    // Fields: Pg=0, Zt=0, imm4=4, Rn=0
    let encoding: u32 = 0xA594A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_imm4_7_poweroftwominusone_a000_a597a000() {
    // Encoding: 0xA597A000
    // Test LDNF1SB_Z.P.BI_S64 field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Rn=0, imm4=7, Zt=0
    let encoding: u32 = 0xA597A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_imm4_8_poweroftwo_a000_a598a000() {
    // Encoding: 0xA598A000
    // Test LDNF1SB_Z.P.BI_S64 field imm4 = 8 (PowerOfTwo)
    // Fields: Pg=0, Rn=0, imm4=8, Zt=0
    let encoding: u32 = 0xA598A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_imm4_15_max_a000_a59fa000() {
    // Encoding: 0xA59FA000
    // Test LDNF1SB_Z.P.BI_S64 field imm4 = 15 (Max)
    // Fields: Pg=0, Zt=0, Rn=0, imm4=15
    let encoding: u32 = 0xA59FA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_pg_0_min_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field Pg = 0 (Min)
    // Fields: Rn=0, Zt=0, imm4=0, Pg=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_pg_1_poweroftwo_a000_a590a400() {
    // Encoding: 0xA590A400
    // Test LDNF1SB_Z.P.BI_S64 field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, imm4=0, Zt=0, Rn=0
    let encoding: u32 = 0xA590A400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_rn_0_min_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field Rn = 0 (Min)
    // Fields: imm4=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_rn_1_poweroftwo_a000_a590a020() {
    // Encoding: 0xA590A020
    // Test LDNF1SB_Z.P.BI_S64 field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zt=0, imm4=0, Rn=1
    let encoding: u32 = 0xA590A020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_rn_30_poweroftwominusone_a000_a590a3c0() {
    // Encoding: 0xA590A3C0
    // Test LDNF1SB_Z.P.BI_S64 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Zt=0, Rn=30, Pg=0
    let encoding: u32 = 0xA590A3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_rn_31_max_a000_a590a3e0() {
    // Encoding: 0xA590A3E0
    // Test LDNF1SB_Z.P.BI_S64 field Rn = 31 (Max)
    // Fields: imm4=0, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xA590A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_zt_0_min_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field Zt = 0 (Min)
    // Fields: imm4=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_zt_1_poweroftwo_a000_a590a001() {
    // Encoding: 0xA590A001
    // Test LDNF1SB_Z.P.BI_S64 field Zt = 1 (PowerOfTwo)
    // Fields: Rn=0, Pg=0, Zt=1, imm4=0
    let encoding: u32 = 0xA590A001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_zt_30_poweroftwominusone_a000_a590a01e() {
    // Encoding: 0xA590A01E
    // Test LDNF1SB_Z.P.BI_S64 field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Rn=0, imm4=0, Zt=30
    let encoding: u32 = 0xA590A01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldnf1sb_z_p_bi_s64_field_zt_31_max_a000_a590a01f() {
    // Encoding: 0xA590A01F
    // Test LDNF1SB_Z.P.BI_S64 field Zt = 31 (Max)
    // Fields: Zt=31, imm4=0, Pg=0, Rn=0
    let encoding: u32 = 0xA590A01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=0 (immediate value 0)
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_0_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=1 (immediate value 1)
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_1_a000_a591a000() {
    // Encoding: 0xA591A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=1, Pg=0, Rn=0, Zt=0
    // Fields: imm4=1, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA591A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=3 (2^2 - 1 = 3)
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_2_a000_a593a000() {
    // Encoding: 0xA593A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=3, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, imm4=3, Rn=0, Pg=0
    let encoding: u32 = 0xA593A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=4 (power of 2 (2^2 = 4))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_3_a000_a594a000() {
    // Encoding: 0xA594A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=4, Pg=0, Rn=0, Zt=0
    // Fields: imm4=4, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA594A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=7 (immediate midpoint (7))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_4_a000_a597a000() {
    // Encoding: 0xA597A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=7, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, imm4=7, Rn=0
    let encoding: u32 = 0xA597A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=8 (power of 2 (2^3 = 8))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_5_a000_a598a000() {
    // Encoding: 0xA598A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=8, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Pg=0, imm4=8
    let encoding: u32 = 0xA598A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=15 (maximum immediate (15))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_6_a000_a59fa000() {
    // Encoding: 0xA59FA000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=15, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=15, Zt=0, Pg=0
    let encoding: u32 = 0xA59FA000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_7_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_8_a000_a590a400() {
    // Encoding: 0xA590A400
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=1, Rn=0, Zt=0
    // Fields: Pg=1, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA590A400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_9_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rn=0, imm4=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_10_a000_a590a020() {
    // Encoding: 0xA590A020
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Zt=0, Rn=1, imm4=0
    let encoding: u32 = 0xA590A020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_11_a000_a590a3c0() {
    // Encoding: 0xA590A3C0
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=30, Zt=0
    // Fields: Pg=0, imm4=0, Rn=30, Zt=0
    let encoding: u32 = 0xA590A3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_12_a000_a590a3e0() {
    // Encoding: 0xA590A3E0
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=31, Zt=0
    // Fields: imm4=0, Pg=0, Zt=0, Rn=31
    let encoding: u32 = 0xA590A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_13_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_14_a000_a590a001() {
    // Encoding: 0xA590A001
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Zt=1, Rn=0, imm4=0
    let encoding: u32 = 0xA590A001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_15_a000_a590a01e() {
    // Encoding: 0xA590A01E
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=0, Zt=30
    // Fields: imm4=0, Pg=0, Rn=0, Zt=30
    let encoding: u32 = 0xA590A01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_16_a000_a590a01f() {
    // Encoding: 0xA590A01F
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, imm4=0, Rn=0, Zt=31
    let encoding: u32 = 0xA590A01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_17_a000_a590a420() {
    // Encoding: 0xA590A420
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=1, Rn=1, Zt=0
    // Fields: Zt=0, imm4=0, Pg=1, Rn=1
    let encoding: u32 = 0xA590A420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldnf1sb_z_p_bi_s64_combo_18_a000_a590bfe0() {
    // Encoding: 0xA590BFE0
    // Test LDNF1SB_Z.P.BI_S64 field combination: imm4=0, Pg=31, Rn=31, Zt=0
    // Fields: Rn=31, imm4=0, Pg=31, Zt=0
    let encoding: u32 = 0xA590BFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldnf1sb_z_p_bi_s64_special_rn_31_stack_pointer_sp_may_require_alignment_40960_a591a3e0() {
    // Encoding: 0xA591A3E0
    // Test LDNF1SB_Z.P.BI_S64 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: imm4=1, Rn=31, Pg=0, Zt=0
    let encoding: u32 = 0xA591A3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldnf1sb_z_p_bi_s64_invalid_0_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: imm4=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNF1SB_Z.P.BI_S64
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldnf1sb_z_p_bi_s64_invalid_1_a000_a590a000() {
    // Encoding: 0xA590A000
    // Test LDNF1SB_Z.P.BI_S64 invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA590A000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LDNT1W_Z.P.BI_Contiguous Tests
// ============================================================================

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_imm4_0_zero_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field imm4 = 0 (Zero)
    // Fields: Pg=0, imm4=0, Zt=0, Rn=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_imm4_1_poweroftwo_e000_a501e000() {
    // Encoding: 0xA501E000
    // Test LDNT1W_Z.P.BI_Contiguous field imm4 = 1 (PowerOfTwo)
    // Fields: Zt=0, imm4=1, Pg=0, Rn=0
    let encoding: u32 = 0xA501E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_imm4_3_poweroftwominusone_e000_a503e000() {
    // Encoding: 0xA503E000
    // Test LDNT1W_Z.P.BI_Contiguous field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: Rn=0, imm4=3, Pg=0, Zt=0
    let encoding: u32 = 0xA503E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_imm4_4_poweroftwo_e000_a504e000() {
    // Encoding: 0xA504E000
    // Test LDNT1W_Z.P.BI_Contiguous field imm4 = 4 (PowerOfTwo)
    // Fields: Pg=0, imm4=4, Zt=0, Rn=0
    let encoding: u32 = 0xA504E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_imm4_7_poweroftwominusone_e000_a507e000() {
    // Encoding: 0xA507E000
    // Test LDNT1W_Z.P.BI_Contiguous field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Zt=0, imm4=7, Pg=0
    let encoding: u32 = 0xA507E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_imm4_8_poweroftwo_e000_a508e000() {
    // Encoding: 0xA508E000
    // Test LDNT1W_Z.P.BI_Contiguous field imm4 = 8 (PowerOfTwo)
    // Fields: imm4=8, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA508E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_imm4_15_max_e000_a50fe000() {
    // Encoding: 0xA50FE000
    // Test LDNT1W_Z.P.BI_Contiguous field imm4 = 15 (Max)
    // Fields: Rn=0, imm4=15, Pg=0, Zt=0
    let encoding: u32 = 0xA50FE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_pg_0_min_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field Pg = 0 (Min)
    // Fields: Zt=0, Pg=0, Rn=0, imm4=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_pg_1_poweroftwo_e000_a500e400() {
    // Encoding: 0xA500E400
    // Test LDNT1W_Z.P.BI_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Zt=0, Rn=0, imm4=0
    let encoding: u32 = 0xA500E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_rn_0_min_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field Rn = 0 (Min)
    // Fields: imm4=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_rn_1_poweroftwo_e000_a500e020() {
    // Encoding: 0xA500E020
    // Test LDNT1W_Z.P.BI_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, imm4=0, Rn=1, Zt=0
    let encoding: u32 = 0xA500E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_rn_30_poweroftwominusone_e000_a500e3c0() {
    // Encoding: 0xA500E3C0
    // Test LDNT1W_Z.P.BI_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Rn=30, Zt=0, imm4=0
    let encoding: u32 = 0xA500E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_rn_31_max_e000_a500e3e0() {
    // Encoding: 0xA500E3E0
    // Test LDNT1W_Z.P.BI_Contiguous field Rn = 31 (Max)
    // Fields: Rn=31, imm4=0, Zt=0, Pg=0
    let encoding: u32 = 0xA500E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_zt_0_min_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field Zt = 0 (Min)
    // Fields: Zt=0, Rn=0, imm4=0, Pg=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_zt_1_poweroftwo_e000_a500e001() {
    // Encoding: 0xA500E001
    // Test LDNT1W_Z.P.BI_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Pg=0, imm4=0, Zt=1, Rn=0
    let encoding: u32 = 0xA500E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_zt_30_poweroftwominusone_e000_a500e01e() {
    // Encoding: 0xA500E01E
    // Test LDNT1W_Z.P.BI_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Rn=0, Zt=30, Pg=0
    let encoding: u32 = 0xA500E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldnt1w_z_p_bi_contiguous_field_zt_31_max_e000_a500e01f() {
    // Encoding: 0xA500E01F
    // Test LDNT1W_Z.P.BI_Contiguous field Zt = 31 (Max)
    // Fields: Pg=0, Rn=0, Zt=31, imm4=0
    let encoding: u32 = 0xA500E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=0 (immediate value 0)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_0_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, imm4=0, Rn=0, Zt=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=1 (immediate value 1)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_1_e000_a501e000() {
    // Encoding: 0xA501E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rn=0, imm4=1
    let encoding: u32 = 0xA501E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=3 (2^2 - 1 = 3)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_2_e000_a503e000() {
    // Encoding: 0xA503E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=3, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rn=0, imm4=3
    let encoding: u32 = 0xA503E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=4 (power of 2 (2^2 = 4))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_3_e000_a504e000() {
    // Encoding: 0xA504E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=4, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=4, Zt=0, Pg=0
    let encoding: u32 = 0xA504E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=7 (immediate midpoint (7))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_4_e000_a507e000() {
    // Encoding: 0xA507E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=7, Pg=0, Rn=0, Zt=0
    // Fields: imm4=7, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA507E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=8 (power of 2 (2^3 = 8))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_5_e000_a508e000() {
    // Encoding: 0xA508E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=8, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, Zt=0, imm4=8
    let encoding: u32 = 0xA508E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=15 (maximum immediate (15))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_6_e000_a50fe000() {
    // Encoding: 0xA50FE000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=15, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, imm4=15, Pg=0
    let encoding: u32 = 0xA50FE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_7_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, Zt=0, imm4=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_8_e000_a500e400() {
    // Encoding: 0xA500E400
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=1, Rn=0, Zt=0
    // Fields: Pg=1, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA500E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_9_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, imm4=0, Zt=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_10_e000_a500e020() {
    // Encoding: 0xA500E020
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, imm4=0, Pg=0, Zt=0
    let encoding: u32 = 0xA500E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_11_e000_a500e3c0() {
    // Encoding: 0xA500E3C0
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=30, Zt=0
    // Fields: Zt=0, imm4=0, Pg=0, Rn=30
    let encoding: u32 = 0xA500E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_12_e000_a500e3e0() {
    // Encoding: 0xA500E3E0
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=31, Zt=0
    // Fields: Pg=0, Zt=0, imm4=0, Rn=31
    let encoding: u32 = 0xA500E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_13_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_14_e000_a500e001() {
    // Encoding: 0xA500E001
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=1
    // Fields: imm4=0, Pg=0, Zt=1, Rn=0
    let encoding: u32 = 0xA500E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_15_e000_a500e01e() {
    // Encoding: 0xA500E01E
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=30
    // Fields: Zt=30, Rn=0, Pg=0, imm4=0
    let encoding: u32 = 0xA500E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_16_e000_a500e01f() {
    // Encoding: 0xA500E01F
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, Zt=31, imm4=0, Rn=0
    let encoding: u32 = 0xA500E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_17_e000_a500e420() {
    // Encoding: 0xA500E420
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=1, Rn=1, Zt=0
    // Fields: Rn=1, Zt=0, Pg=1, imm4=0
    let encoding: u32 = 0xA500E420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldnt1w_z_p_bi_contiguous_combo_18_e000_a500ffe0() {
    // Encoding: 0xA500FFE0
    // Test LDNT1W_Z.P.BI_Contiguous field combination: imm4=0, Pg=31, Rn=31, Zt=0
    // Fields: Zt=0, imm4=0, Pg=31, Rn=31
    let encoding: u32 = 0xA500FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldnt1w_z_p_bi_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_57344_a501e3e0() {
    // Encoding: 0xA501E3E0
    // Test LDNT1W_Z.P.BI_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Pg=0, Rn=31, imm4=1, Zt=0
    let encoding: u32 = 0xA501E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldnt1w_z_p_bi_contiguous_invalid_0_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zt=0, Rn=0, Pg=0, imm4=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1W_Z.P.BI_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldnt1w_z_p_bi_contiguous_invalid_1_e000_a500e000() {
    // Encoding: 0xA500E000
    // Test LDNT1W_Z.P.BI_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, imm4=0, Pg=0, Rn=0
    let encoding: u32 = 0xA500E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LDNT1D_Z.P.BI_Contiguous Tests
// ============================================================================

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_imm4_0_zero_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field imm4 = 0 (Zero)
    // Fields: Zt=0, Rn=0, imm4=0, Pg=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_imm4_1_poweroftwo_e000_a581e000() {
    // Encoding: 0xA581E000
    // Test LDNT1D_Z.P.BI_Contiguous field imm4 = 1 (PowerOfTwo)
    // Fields: Rn=0, Zt=0, imm4=1, Pg=0
    let encoding: u32 = 0xA581E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_imm4_3_poweroftwominusone_e000_a583e000() {
    // Encoding: 0xA583E000
    // Test LDNT1D_Z.P.BI_Contiguous field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Pg=0, imm4=3, Rn=0
    let encoding: u32 = 0xA583E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_imm4_4_poweroftwo_e000_a584e000() {
    // Encoding: 0xA584E000
    // Test LDNT1D_Z.P.BI_Contiguous field imm4 = 4 (PowerOfTwo)
    // Fields: imm4=4, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA584E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_imm4_7_poweroftwominusone_e000_a587e000() {
    // Encoding: 0xA587E000
    // Test LDNT1D_Z.P.BI_Contiguous field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: imm4=7, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA587E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_imm4_8_poweroftwo_e000_a588e000() {
    // Encoding: 0xA588E000
    // Test LDNT1D_Z.P.BI_Contiguous field imm4 = 8 (PowerOfTwo)
    // Fields: Pg=0, imm4=8, Zt=0, Rn=0
    let encoding: u32 = 0xA588E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_imm4_15_max_e000_a58fe000() {
    // Encoding: 0xA58FE000
    // Test LDNT1D_Z.P.BI_Contiguous field imm4 = 15 (Max)
    // Fields: imm4=15, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA58FE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_pg_0_min_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field Pg = 0 (Min)
    // Fields: Pg=0, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_pg_1_poweroftwo_e000_a580e400() {
    // Encoding: 0xA580E400
    // Test LDNT1D_Z.P.BI_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Zt=0, Rn=0, imm4=0, Pg=1
    let encoding: u32 = 0xA580E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_rn_0_min_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field Rn = 0 (Min)
    // Fields: imm4=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_rn_1_poweroftwo_e000_a580e020() {
    // Encoding: 0xA580E020
    // Test LDNT1D_Z.P.BI_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zt=0, Rn=1, imm4=0
    let encoding: u32 = 0xA580E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_rn_30_poweroftwominusone_e000_a580e3c0() {
    // Encoding: 0xA580E3C0
    // Test LDNT1D_Z.P.BI_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Zt=0, Pg=0, Rn=30
    let encoding: u32 = 0xA580E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_rn_31_max_e000_a580e3e0() {
    // Encoding: 0xA580E3E0
    // Test LDNT1D_Z.P.BI_Contiguous field Rn = 31 (Max)
    // Fields: Pg=0, Rn=31, Zt=0, imm4=0
    let encoding: u32 = 0xA580E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_zt_0_min_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field Zt = 0 (Min)
    // Fields: Rn=0, Pg=0, imm4=0, Zt=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_zt_1_poweroftwo_e000_a580e001() {
    // Encoding: 0xA580E001
    // Test LDNT1D_Z.P.BI_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Rn=0, Pg=0, imm4=0, Zt=1
    let encoding: u32 = 0xA580E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_zt_30_poweroftwominusone_e000_a580e01e() {
    // Encoding: 0xA580E01E
    // Test LDNT1D_Z.P.BI_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=30, Pg=0, Rn=0, imm4=0
    let encoding: u32 = 0xA580E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ldnt1d_z_p_bi_contiguous_field_zt_31_max_e000_a580e01f() {
    // Encoding: 0xA580E01F
    // Test LDNT1D_Z.P.BI_Contiguous field Zt = 31 (Max)
    // Fields: Pg=0, imm4=0, Zt=31, Rn=0
    let encoding: u32 = 0xA580E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=0 (immediate value 0)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_0_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=1 (immediate value 1)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_1_e000_a581e000() {
    // Encoding: 0xA581E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, imm4=1, Rn=0
    let encoding: u32 = 0xA581E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=3 (2^2 - 1 = 3)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_2_e000_a583e000() {
    // Encoding: 0xA583E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=3, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=3, Zt=0, Pg=0
    let encoding: u32 = 0xA583E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=4 (power of 2 (2^2 = 4))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_3_e000_a584e000() {
    // Encoding: 0xA584E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=4, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, imm4=4, Pg=0, Rn=0
    let encoding: u32 = 0xA584E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=7 (immediate midpoint (7))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_4_e000_a587e000() {
    // Encoding: 0xA587E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=7, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Pg=0, imm4=7
    let encoding: u32 = 0xA587E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=8 (power of 2 (2^3 = 8))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_5_e000_a588e000() {
    // Encoding: 0xA588E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=8, Pg=0, Rn=0, Zt=0
    // Fields: imm4=8, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA588E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=15 (maximum immediate (15))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_6_e000_a58fe000() {
    // Encoding: 0xA58FE000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=15, Pg=0, Rn=0, Zt=0
    // Fields: imm4=15, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA58FE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_7_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, imm4=0, Zt=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_8_e000_a580e400() {
    // Encoding: 0xA580E400
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=1, Rn=0, Zt=0
    // Fields: Zt=0, Pg=1, Rn=0, imm4=0
    let encoding: u32 = 0xA580E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_9_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_10_e000_a580e020() {
    // Encoding: 0xA580E020
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Rn=1, Zt=0, imm4=0
    let encoding: u32 = 0xA580E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_11_e000_a580e3c0() {
    // Encoding: 0xA580E3C0
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=30, Zt=0
    // Fields: imm4=0, Rn=30, Zt=0, Pg=0
    let encoding: u32 = 0xA580E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_12_e000_a580e3e0() {
    // Encoding: 0xA580E3E0
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=31, Zt=0
    // Fields: Pg=0, imm4=0, Rn=31, Zt=0
    let encoding: u32 = 0xA580E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_13_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_14_e000_a580e001() {
    // Encoding: 0xA580E001
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, imm4=0, Zt=1, Rn=0
    let encoding: u32 = 0xA580E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_15_e000_a580e01e() {
    // Encoding: 0xA580E01E
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=30
    // Fields: imm4=0, Zt=30, Pg=0, Rn=0
    let encoding: u32 = 0xA580E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_16_e000_a580e01f() {
    // Encoding: 0xA580E01F
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=31
    // Fields: Rn=0, Zt=31, imm4=0, Pg=0
    let encoding: u32 = 0xA580E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_17_e000_a580e420() {
    // Encoding: 0xA580E420
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=1, Rn=1, Zt=0
    // Fields: Rn=1, imm4=0, Zt=0, Pg=1
    let encoding: u32 = 0xA580E420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ldnt1d_z_p_bi_contiguous_combo_18_e000_a580ffe0() {
    // Encoding: 0xA580FFE0
    // Test LDNT1D_Z.P.BI_Contiguous field combination: imm4=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, imm4=0, Rn=31, Zt=0
    let encoding: u32 = 0xA580FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ldnt1d_z_p_bi_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_57344_a581e3e0() {
    // Encoding: 0xA581E3E0
    // Test LDNT1D_Z.P.BI_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Pg=0, imm4=1, Zt=0
    let encoding: u32 = 0xA581E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ldnt1d_z_p_bi_contiguous_invalid_0_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: imm4=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LDNT1D_Z.P.BI_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ldnt1d_z_p_bi_contiguous_invalid_1_e000_a580e000() {
    // Encoding: 0xA580E000
    // Test LDNT1D_Z.P.BI_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Rn=0, imm4=0, Zt=0
    let encoding: u32 = 0xA580E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD4H_Z.P.BI_Contiguous Tests
// ============================================================================

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_ld4h_z_p_bi_contiguous_field_imm4_0_zero_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field imm4 = 0 (Zero)
    // Fields: Pg=0, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_ld4h_z_p_bi_contiguous_field_imm4_1_poweroftwo_e000_a4e1e000() {
    // Encoding: 0xA4E1E000
    // Test LD4H_Z.P.BI_Contiguous field imm4 = 1 (PowerOfTwo)
    // Fields: Rn=0, imm4=1, Pg=0, Zt=0
    let encoding: u32 = 0xA4E1E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_ld4h_z_p_bi_contiguous_field_imm4_3_poweroftwominusone_e000_a4e3e000() {
    // Encoding: 0xA4E3E000
    // Test LD4H_Z.P.BI_Contiguous field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Pg=0, imm4=3, Zt=0
    let encoding: u32 = 0xA4E3E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_imm4_4_poweroftwo_e000_a4e4e000() {
    // Encoding: 0xA4E4E000
    // Test LD4H_Z.P.BI_Contiguous field imm4 = 4 (PowerOfTwo)
    // Fields: imm4=4, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA4E4E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_imm4_7_poweroftwominusone_e000_a4e7e000() {
    // Encoding: 0xA4E7E000
    // Test LD4H_Z.P.BI_Contiguous field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: Rn=0, imm4=7, Pg=0, Zt=0
    let encoding: u32 = 0xA4E7E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_imm4_8_poweroftwo_e000_a4e8e000() {
    // Encoding: 0xA4E8E000
    // Test LD4H_Z.P.BI_Contiguous field imm4 = 8 (PowerOfTwo)
    // Fields: imm4=8, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA4E8E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field imm4 16 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_imm4_15_max_e000_a4efe000() {
    // Encoding: 0xA4EFE000
    // Test LD4H_Z.P.BI_Contiguous field imm4 = 15 (Max)
    // Fields: Zt=0, imm4=15, Pg=0, Rn=0
    let encoding: u32 = 0xA4EFE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_pg_0_min_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field Pg = 0 (Min)
    // Fields: Pg=0, Rn=0, Zt=0, imm4=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_pg_1_poweroftwo_e000_a4e0e400() {
    // Encoding: 0xA4E0E400
    // Test LD4H_Z.P.BI_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, imm4=0, Rn=0, Zt=0
    let encoding: u32 = 0xA4E0E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_rn_0_min_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field Rn = 0 (Min)
    // Fields: Pg=0, Rn=0, imm4=0, Zt=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_rn_1_poweroftwo_e000_a4e0e020() {
    // Encoding: 0xA4E0E020
    // Test LD4H_Z.P.BI_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: imm4=0, Pg=0, Zt=0, Rn=1
    let encoding: u32 = 0xA4E0E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_rn_30_poweroftwominusone_e000_a4e0e3c0() {
    // Encoding: 0xA4E0E3C0
    // Test LD4H_Z.P.BI_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Pg=0, Rn=30, Zt=0
    let encoding: u32 = 0xA4E0E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld4h_z_p_bi_contiguous_field_rn_31_max_e000_a4e0e3e0() {
    // Encoding: 0xA4E0E3E0
    // Test LD4H_Z.P.BI_Contiguous field Rn = 31 (Max)
    // Fields: imm4=0, Zt=0, Rn=31, Pg=0
    let encoding: u32 = 0xA4E0E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld4h_z_p_bi_contiguous_field_zt_0_min_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field Zt = 0 (Min)
    // Fields: Pg=0, Zt=0, imm4=0, Rn=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld4h_z_p_bi_contiguous_field_zt_1_poweroftwo_e000_a4e0e001() {
    // Encoding: 0xA4E0E001
    // Test LD4H_Z.P.BI_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Pg=0, Zt=1, Rn=0, imm4=0
    let encoding: u32 = 0xA4E0E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld4h_z_p_bi_contiguous_field_zt_30_poweroftwominusone_e000_a4e0e01e() {
    // Encoding: 0xA4E0E01E
    // Test LD4H_Z.P.BI_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, imm4=0, Zt=30, Rn=0
    let encoding: u32 = 0xA4E0E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld4h_z_p_bi_contiguous_field_zt_31_max_e000_a4e0e01f() {
    // Encoding: 0xA4E0E01F
    // Test LD4H_Z.P.BI_Contiguous field Zt = 31 (Max)
    // Fields: Pg=0, imm4=0, Rn=0, Zt=31
    let encoding: u32 = 0xA4E0E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=0 (immediate value 0)
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_0_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, imm4=0, Pg=0, Zt=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=1 (immediate value 1)
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_1_e000_a4e1e000() {
    // Encoding: 0xA4E1E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=1, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, imm4=1, Zt=0
    let encoding: u32 = 0xA4E1E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=3 (2^2 - 1 = 3)
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_2_e000_a4e3e000() {
    // Encoding: 0xA4E3E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=3, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rn=0, imm4=3
    let encoding: u32 = 0xA4E3E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=4 (power of 2 (2^2 = 4))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_3_e000_a4e4e000() {
    // Encoding: 0xA4E4E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=4, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, imm4=4, Pg=0
    let encoding: u32 = 0xA4E4E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=7 (immediate midpoint (7))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_4_e000_a4e7e000() {
    // Encoding: 0xA4E7E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=7, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, imm4=7, Zt=0
    let encoding: u32 = 0xA4E7E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=8 (power of 2 (2^3 = 8))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_5_e000_a4e8e000() {
    // Encoding: 0xA4E8E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=8, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Rn=0, imm4=8
    let encoding: u32 = 0xA4E8E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm4=15 (maximum immediate (15))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_6_e000_a4efe000() {
    // Encoding: 0xA4EFE000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=15, Pg=0, Rn=0, Zt=0
    // Fields: imm4=15, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA4EFE000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_7_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Rn=0, imm4=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_8_e000_a4e0e400() {
    // Encoding: 0xA4E0E400
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=1, Rn=0, Zt=0
    // Fields: Pg=1, Rn=0, imm4=0, Zt=0
    let encoding: u32 = 0xA4E0E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_9_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_10_e000_a4e0e020() {
    // Encoding: 0xA4E0E020
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=1, Zt=0
    // Fields: Zt=0, imm4=0, Pg=0, Rn=1
    let encoding: u32 = 0xA4E0E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_11_e000_a4e0e3c0() {
    // Encoding: 0xA4E0E3C0
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=30, Zt=0
    // Fields: imm4=0, Pg=0, Rn=30, Zt=0
    let encoding: u32 = 0xA4E0E3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_12_e000_a4e0e3e0() {
    // Encoding: 0xA4E0E3E0
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=31, Zt=0
    // Fields: imm4=0, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xA4E0E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_13_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=0
    // Fields: imm4=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_14_e000_a4e0e001() {
    // Encoding: 0xA4E0E001
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=1
    // Fields: Rn=0, Zt=1, imm4=0, Pg=0
    let encoding: u32 = 0xA4E0E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_15_e000_a4e0e01e() {
    // Encoding: 0xA4E0E01E
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=30
    // Fields: Pg=0, Rn=0, Zt=30, imm4=0
    let encoding: u32 = 0xA4E0E01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_16_e000_a4e0e01f() {
    // Encoding: 0xA4E0E01F
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=0, Rn=0, Zt=31
    // Fields: Zt=31, Rn=0, Pg=0, imm4=0
    let encoding: u32 = 0xA4E0E01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_17_e000_a4e0e420() {
    // Encoding: 0xA4E0E420
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=1, Rn=1, Zt=0
    // Fields: Pg=1, Rn=1, imm4=0, Zt=0
    let encoding: u32 = 0xA4E0E420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld4h_z_p_bi_contiguous_combo_18_e000_a4e0ffe0() {
    // Encoding: 0xA4E0FFE0
    // Test LD4H_Z.P.BI_Contiguous field combination: imm4=0, Pg=31, Rn=31, Zt=0
    // Fields: imm4=0, Zt=0, Rn=31, Pg=31
    let encoding: u32 = 0xA4E0FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld4h_z_p_bi_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_57344_a4e1e3e0() {
    // Encoding: 0xA4E1E3E0
    // Test LD4H_Z.P.BI_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Pg=0, imm4=1, Zt=0
    let encoding: u32 = 0xA4E1E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld4h_z_p_bi_contiguous_invalid_0_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zt=0, imm4=0, Rn=0, Pg=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD4H_Z.P.BI_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld4h_z_p_bi_contiguous_invalid_1_e000_a4e0e000() {
    // Encoding: 0xA4E0E000
    // Test LD4H_Z.P.BI_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Zt=0, imm4=0, Pg=0
    let encoding: u32 = 0xA4E0E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD3H_Z.P.BR_Contiguous Tests
// ============================================================================

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rm_0_min_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field Rm = 0 (Min)
    // Fields: Rn=0, Rm=0, Zt=0, Pg=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rm_1_poweroftwo_c000_a4c1c000() {
    // Encoding: 0xA4C1C000
    // Test LD3H_Z.P.BR_Contiguous field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA4C1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rm_30_poweroftwominusone_c000_a4dec000() {
    // Encoding: 0xA4DEC000
    // Test LD3H_Z.P.BR_Contiguous field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rm=30, Zt=0, Pg=0
    let encoding: u32 = 0xA4DEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rm_31_max_c000_a4dfc000() {
    // Encoding: 0xA4DFC000
    // Test LD3H_Z.P.BR_Contiguous field Rm = 31 (Max)
    // Fields: Rn=0, Zt=0, Pg=0, Rm=31
    let encoding: u32 = 0xA4DFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld3h_z_p_br_contiguous_field_pg_0_min_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field Pg = 0 (Min)
    // Fields: Rm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld3h_z_p_br_contiguous_field_pg_1_poweroftwo_c000_a4c0c400() {
    // Encoding: 0xA4C0C400
    // Test LD3H_Z.P.BR_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Zt=0, Rn=0, Rm=0
    let encoding: u32 = 0xA4C0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rn_0_min_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field Rn = 0 (Min)
    // Fields: Rm=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rn_1_poweroftwo_c000_a4c0c020() {
    // Encoding: 0xA4C0C020
    // Test LD3H_Z.P.BR_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: Zt=0, Rm=0, Rn=1, Pg=0
    let encoding: u32 = 0xA4C0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rn_30_poweroftwominusone_c000_a4c0c3c0() {
    // Encoding: 0xA4C0C3C0
    // Test LD3H_Z.P.BR_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Rm=0, Pg=0, Rn=30
    let encoding: u32 = 0xA4C0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld3h_z_p_br_contiguous_field_rn_31_max_c000_a4c0c3e0() {
    // Encoding: 0xA4C0C3E0
    // Test LD3H_Z.P.BR_Contiguous field Rn = 31 (Max)
    // Fields: Zt=0, Rn=31, Rm=0, Pg=0
    let encoding: u32 = 0xA4C0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld3h_z_p_br_contiguous_field_zt_0_min_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field Zt = 0 (Min)
    // Fields: Pg=0, Rn=0, Zt=0, Rm=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld3h_z_p_br_contiguous_field_zt_1_poweroftwo_c000_a4c0c001() {
    // Encoding: 0xA4C0C001
    // Test LD3H_Z.P.BR_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Rn=0, Pg=0, Rm=0, Zt=1
    let encoding: u32 = 0xA4C0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld3h_z_p_br_contiguous_field_zt_30_poweroftwominusone_c000_a4c0c01e() {
    // Encoding: 0xA4C0C01E
    // Test LD3H_Z.P.BR_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zt=30, Rn=0, Rm=0
    let encoding: u32 = 0xA4C0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld3h_z_p_br_contiguous_field_zt_31_max_c000_a4c0c01f() {
    // Encoding: 0xA4C0C01F
    // Test LD3H_Z.P.BR_Contiguous field Zt = 31 (Max)
    // Fields: Pg=0, Rm=0, Zt=31, Rn=0
    let encoding: u32 = 0xA4C0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_0_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (register index 1 (second register))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_1_c000_a4c1c000() {
    // Encoding: 0xA4C1C000
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Rm=1, Zt=0, Pg=0
    let encoding: u32 = 0xA4C1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_2_c000_a4dec000() {
    // Encoding: 0xA4DEC000
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=30, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rm=30, Rn=0
    let encoding: u32 = 0xA4DEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (register index 31 (special))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_3_c000_a4dfc000() {
    // Encoding: 0xA4DFC000
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Rm=31, Pg=0
    let encoding: u32 = 0xA4DFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_4_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rm=0, Rn=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_5_c000_a4c0c400() {
    // Encoding: 0xA4C0C400
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rm=0, Rn=0, Zt=0, Pg=1
    let encoding: u32 = 0xA4C0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_6_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rm=0, Rn=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_7_c000_a4c0c020() {
    // Encoding: 0xA4C0C020
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=1, Zt=0
    // Fields: Zt=0, Rm=0, Pg=0, Rn=1
    let encoding: u32 = 0xA4C0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_8_c000_a4c0c3c0() {
    // Encoding: 0xA4C0C3C0
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rm=0, Zt=0, Pg=0, Rn=30
    let encoding: u32 = 0xA4C0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_9_c000_a4c0c3e0() {
    // Encoding: 0xA4C0C3E0
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=31, Zt=0
    // Fields: Rm=0, Pg=0, Rn=31, Zt=0
    let encoding: u32 = 0xA4C0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld3h_z_p_br_contiguous_combo_10_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Pg=0, Rm=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld3h_z_p_br_contiguous_combo_11_c000_a4c0c001() {
    // Encoding: 0xA4C0C001
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=1
    // Fields: Rm=0, Rn=0, Zt=1, Pg=0
    let encoding: u32 = 0xA4C0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld3h_z_p_br_contiguous_combo_12_c000_a4c0c01e() {
    // Encoding: 0xA4C0C01E
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=30
    // Fields: Rm=0, Rn=0, Pg=0, Zt=30
    let encoding: u32 = 0xA4C0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld3h_z_p_br_contiguous_combo_13_c000_a4c0c01f() {
    // Encoding: 0xA4C0C01F
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zt=31, Pg=0, Rm=0, Rn=0
    let encoding: u32 = 0xA4C0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Pg=1 (same register test (reg=1))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_14_c000_a4c1c400() {
    // Encoding: 0xA4C1C400
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=1, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Rm=1, Pg=1
    let encoding: u32 = 0xA4C1C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Pg=31 (same register test (reg=31))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_15_c000_a4dfdc00() {
    // Encoding: 0xA4DFDC00
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=31, Pg=31, Rn=0, Zt=0
    // Fields: Rm=31, Pg=31, Rn=0, Zt=0
    let encoding: u32 = 0xA4DFDC00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_16_c000_a4c1c020() {
    // Encoding: 0xA4C1C020
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=1, Zt=0
    // Fields: Rm=1, Pg=0, Zt=0, Rn=1
    let encoding: u32 = 0xA4C1C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_17_c000_a4dfc3e0() {
    // Encoding: 0xA4DFC3E0
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=31, Zt=0
    // Fields: Rn=31, Zt=0, Rm=31, Pg=0
    let encoding: u32 = 0xA4DFC3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_18_c000_a4c0c420() {
    // Encoding: 0xA4C0C420
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=1, Zt=0
    // Fields: Zt=0, Pg=1, Rn=1, Rm=0
    let encoding: u32 = 0xA4C0C420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field combination 19`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld3h_z_p_br_contiguous_combo_19_c000_a4c0dfe0() {
    // Encoding: 0xA4C0DFE0
    // Test LD3H_Z.P.BR_Contiguous field combination: Rm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Rm=0, Zt=0, Rn=31
    let encoding: u32 = 0xA4C0DFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld3h_z_p_br_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_49152_a4c0c3e0() {
    // Encoding: 0xA4C0C3E0
    // Test LD3H_Z.P.BR_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xA4C0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld3h_z_p_br_contiguous_invalid_0_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zt=0, Rm=0, Pg=0, Rn=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld3h_z_p_br_contiguous_invalid_1_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, Rm=0, Rn=0, Pg=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Rm\" }), rhs: LitBits([true, true, true, true, true]) }" }
/// triggers Undefined
#[test]
fn test_ld3h_z_p_br_contiguous_invalid_2_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous invalid encoding: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }
    // Fields: Pg=0, Zt=0, Rm=0, Rn=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD3H_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld3h_z_p_br_contiguous_invalid_3_c000_a4c0c000() {
    // Encoding: 0xA4C0C000
    // Test LD3H_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD1SH_Z.P.BZ_S.x32.scaled Tests
// ============================================================================

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_xs_0_min_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field xs = 0 (Min)
    // Fields: Rn=0, Pg=0, Zt=0, xs=0, Zm=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_xs_1_max_0_84e00000() {
    // Encoding: 0x84E00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field xs = 1 (Max)
    // Fields: Pg=0, xs=1, Rn=0, Zt=0, Zm=0
    let encoding: u32 = 0x84E00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zm_0_min_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zm = 0 (Min)
    // Fields: xs=0, Pg=0, Zt=0, Zm=0, Rn=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zm_1_poweroftwo_0_84a10000() {
    // Encoding: 0x84A10000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zm = 1 (PowerOfTwo)
    // Fields: xs=0, Zt=0, Pg=0, Zm=1, Rn=0
    let encoding: u32 = 0x84A10000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zm_30_poweroftwominusone_0_84be0000() {
    // Encoding: 0x84BE0000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zt=0, Rn=0, xs=0, Zm=30
    let encoding: u32 = 0x84BE0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zm_31_max_0_84bf0000() {
    // Encoding: 0x84BF0000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zm = 31 (Max)
    // Fields: Zt=0, xs=0, Rn=0, Zm=31, Pg=0
    let encoding: u32 = 0x84BF0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_pg_0_min_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Pg = 0 (Min)
    // Fields: Zm=0, Pg=0, xs=0, Rn=0, Zt=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_pg_1_poweroftwo_0_84a00400() {
    // Encoding: 0x84A00400
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, xs=0, Rn=0, Zt=0, Zm=0
    let encoding: u32 = 0x84A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_rn_0_min_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Rn = 0 (Min)
    // Fields: Zt=0, Zm=0, Pg=0, xs=0, Rn=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_rn_1_poweroftwo_0_84a00020() {
    // Encoding: 0x84A00020
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Rn = 1 (PowerOfTwo)
    // Fields: Zm=0, Pg=0, Rn=1, xs=0, Zt=0
    let encoding: u32 = 0x84A00020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_rn_30_poweroftwominusone_0_84a003c0() {
    // Encoding: 0x84A003C0
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Pg=0, xs=0, Zm=0, Zt=0
    let encoding: u32 = 0x84A003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_rn_31_max_0_84a003e0() {
    // Encoding: 0x84A003E0
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Rn = 31 (Max)
    // Fields: xs=0, Zt=0, Zm=0, Pg=0, Rn=31
    let encoding: u32 = 0x84A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zt_0_min_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zt = 0 (Min)
    // Fields: Rn=0, Zt=0, Pg=0, Zm=0, xs=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zt_1_poweroftwo_0_84a00001() {
    // Encoding: 0x84A00001
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zt = 1 (PowerOfTwo)
    // Fields: Pg=0, Zm=0, Zt=1, Rn=0, xs=0
    let encoding: u32 = 0x84A00001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zt_30_poweroftwominusone_0_84a0001e() {
    // Encoding: 0x84A0001E
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Pg=0, Zm=0, xs=0, Zt=30
    let encoding: u32 = 0x84A0001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_field_zt_31_max_0_84a0001f() {
    // Encoding: 0x84A0001F
    // Test LD1SH_Z.P.BZ_S.x32.scaled field Zt = 31 (Max)
    // Fields: Pg=0, Rn=0, Zt=31, Zm=0, xs=0
    let encoding: u32 = 0x84A0001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_0_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=0, Zt=0, Rn=0, xs=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_1_0_84e00000() {
    // Encoding: 0x84E00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0x84E00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_2_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Zm=0, Pg=0, xs=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_3_0_84a10000() {
    // Encoding: 0x84A10000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, xs=0, Zm=1, Rn=0
    let encoding: u32 = 0x84A10000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_4_0_84be0000() {
    // Encoding: 0x84BE0000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, xs=0, Zm=30, Pg=0, Zt=0
    let encoding: u32 = 0x84BE0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_5_0_84bf0000() {
    // Encoding: 0x84BF0000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=31, xs=0, Rn=0, Pg=0
    let encoding: u32 = 0x84BF0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_6_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, xs=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_7_0_84a00400() {
    // Encoding: 0x84A00400
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Pg=1, Zm=0, xs=0
    let encoding: u32 = 0x84A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_8_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Pg=0, Rn=0, Zt=0, xs=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_9_0_84a00020() {
    // Encoding: 0x84A00020
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Rn=1, Zt=0, Zm=0, xs=0
    let encoding: u32 = 0x84A00020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_10_0_84a003c0() {
    // Encoding: 0x84A003C0
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zt=0, Pg=0, Zm=0, xs=0, Rn=30
    let encoding: u32 = 0x84A003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_11_0_84a003e0() {
    // Encoding: 0x84A003E0
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zm=0, Pg=0, Rn=31, xs=0, Zt=0
    let encoding: u32 = 0x84A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_12_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Pg=0, xs=0, Zt=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_13_0_84a00001() {
    // Encoding: 0x84A00001
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Zm=0, xs=0, Rn=0, Zt=1
    let encoding: u32 = 0x84A00001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_14_0_84a0001e() {
    // Encoding: 0x84A0001E
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zm=0, Zt=30, xs=0, Pg=0, Rn=0
    let encoding: u32 = 0x84A0001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_15_0_84a0001f() {
    // Encoding: 0x84A0001F
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, Zt=31, xs=0, Zm=0, Rn=0
    let encoding: u32 = 0x84A0001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_16_0_84a00420() {
    // Encoding: 0x84A00420
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: xs=0, Zm=0, Rn=1, Pg=1, Zt=0
    let encoding: u32 = 0x84A00420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_combo_17_0_84a01fe0() {
    // Encoding: 0x84A01FE0
    // Test LD1SH_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Rn=31, xs=0, Zt=0, Pg=31, Zm=0
    let encoding: u32 = 0x84A01FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_special_rn_31_stack_pointer_sp_may_require_alignment_0_84a003e0() {
    // Encoding: 0x84A003E0
    // Test LD1SH_Z.P.BZ_S.x32.scaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Pg=0, xs=0, Zm=0, Rn=31, Zt=0
    let encoding: u32 = 0x84A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_invalid_0_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: xs=0, Rn=0, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.scaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_s_x32_scaled_invalid_1_0_84a00000() {
    // Encoding: 0x84A00000
    // Test LD1SH_Z.P.BZ_S.x32.scaled invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, xs=0, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0x84A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_xs_0_min_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field xs = 0 (Min)
    // Fields: Zt=0, Zm=0, xs=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_xs_1_max_0_c4e00000() {
    // Encoding: 0xC4E00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field xs = 1 (Max)
    // Fields: Zt=0, Zm=0, xs=1, Pg=0, Rn=0
    let encoding: u32 = 0xC4E00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zm_0_min_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zm = 0 (Min)
    // Fields: Rn=0, Pg=0, xs=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zm_1_poweroftwo_0_c4a10000() {
    // Encoding: 0xC4A10000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zm = 1 (PowerOfTwo)
    // Fields: xs=0, Rn=0, Zt=0, Zm=1, Pg=0
    let encoding: u32 = 0xC4A10000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zm_30_poweroftwominusone_0_c4be0000() {
    // Encoding: 0xC4BE0000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, xs=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4BE0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zm_31_max_0_c4bf0000() {
    // Encoding: 0xC4BF0000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zm = 31 (Max)
    // Fields: Zm=31, Rn=0, Pg=0, Zt=0, xs=0
    let encoding: u32 = 0xC4BF0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_pg_0_min_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Pg = 0 (Min)
    // Fields: Rn=0, Pg=0, Zm=0, Zt=0, xs=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_pg_1_poweroftwo_0_c4a00400() {
    // Encoding: 0xC4A00400
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Rn=0, xs=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_rn_0_min_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Rn = 0 (Min)
    // Fields: Pg=0, Rn=0, Zt=0, xs=0, Zm=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_rn_1_poweroftwo_0_c4a00020() {
    // Encoding: 0xC4A00020
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Rn = 1 (PowerOfTwo)
    // Fields: xs=0, Pg=0, Rn=1, Zm=0, Zt=0
    let encoding: u32 = 0xC4A00020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_rn_30_poweroftwominusone_0_c4a003c0() {
    // Encoding: 0xC4A003C0
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Pg=0, Zt=0, Rn=30, xs=0
    let encoding: u32 = 0xC4A003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_rn_31_max_0_c4a003e0() {
    // Encoding: 0xC4A003E0
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Rn = 31 (Max)
    // Fields: Pg=0, xs=0, Zm=0, Zt=0, Rn=31
    let encoding: u32 = 0xC4A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zt_0_min_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zt = 0 (Min)
    // Fields: Rn=0, xs=0, Zt=0, Pg=0, Zm=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zt_1_poweroftwo_0_c4a00001() {
    // Encoding: 0xC4A00001
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zt = 1 (PowerOfTwo)
    // Fields: Zm=0, xs=0, Pg=0, Rn=0, Zt=1
    let encoding: u32 = 0xC4A00001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zt_30_poweroftwominusone_0_c4a0001e() {
    // Encoding: 0xC4A0001E
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Zm=0, xs=0, Pg=0, Zt=30
    let encoding: u32 = 0xC4A0001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_field_zt_31_max_0_c4a0001f() {
    // Encoding: 0xC4A0001F
    // Test LD1SH_Z.P.BZ_D.x32.scaled field Zt = 31 (Max)
    // Fields: Pg=0, Rn=0, Zt=31, Zm=0, xs=0
    let encoding: u32 = 0xC4A0001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_0_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, xs=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_1_0_c4e00000() {
    // Encoding: 0xC4E00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=0, Rn=0, xs=1, Pg=0
    let encoding: u32 = 0xC4E00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_2_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_3_0_c4a10000() {
    // Encoding: 0xC4A10000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Rn=0, xs=0, Zm=1
    let encoding: u32 = 0xC4A10000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_4_0_c4be0000() {
    // Encoding: 0xC4BE0000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Zm=30, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4BE0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_5_0_c4bf0000() {
    // Encoding: 0xC4BF0000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, xs=0, Zm=31, Pg=0, Zt=0
    let encoding: u32 = 0xC4BF0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_6_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, xs=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_7_0_c4a00400() {
    // Encoding: 0xC4A00400
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, xs=0, Zm=0, Pg=1, Zt=0
    let encoding: u32 = 0xC4A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_8_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_9_0_c4a00020() {
    // Encoding: 0xC4A00020
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Zm=0, xs=0, Zt=0, Pg=0, Rn=1
    let encoding: u32 = 0xC4A00020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_10_0_c4a003c0() {
    // Encoding: 0xC4A003C0
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: xs=0, Pg=0, Zm=0, Rn=30, Zt=0
    let encoding: u32 = 0xC4A003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_11_0_c4a003e0() {
    // Encoding: 0xC4A003E0
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zm=0, Rn=31, Pg=0, xs=0, Zt=0
    let encoding: u32 = 0xC4A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_12_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=0, Rn=0, Zt=0, xs=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_13_0_c4a00001() {
    // Encoding: 0xC4A00001
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Zt=1, xs=0, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4A00001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_14_0_c4a0001e() {
    // Encoding: 0xC4A0001E
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    let encoding: u32 = 0xC4A0001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_15_0_c4a0001f() {
    // Encoding: 0xC4A0001F
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, Rn=0, Zt=31, xs=0, Zm=0
    let encoding: u32 = 0xC4A0001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_16_0_c4a00420() {
    // Encoding: 0xC4A00420
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: xs=0, Pg=1, Zt=0, Rn=1, Zm=0
    let encoding: u32 = 0xC4A00420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_combo_17_0_c4a01fe0() {
    // Encoding: 0xC4A01FE0
    // Test LD1SH_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Zt=0, Zm=0, Pg=31, Rn=31, xs=0
    let encoding: u32 = 0xC4A01FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_special_rn_31_stack_pointer_sp_may_require_alignment_0_c4a003e0() {
    // Encoding: 0xC4A003E0
    // Test LD1SH_Z.P.BZ_D.x32.scaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Pg=0, xs=0, Zm=0, Rn=31, Zt=0
    let encoding: u32 = 0xC4A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_invalid_0_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, xs=0, Zm=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.scaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_x32_scaled_invalid_1_0_c4a00000() {
    // Encoding: 0xC4A00000
    // Test LD1SH_Z.P.BZ_D.x32.scaled invalid encoding: Unconditional UNDEFINED
    // Fields: xs=0, Pg=0, Zm=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_xs_0_min_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field xs = 0 (Min)
    // Fields: Pg=0, Zm=0, xs=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_xs_1_max_0_c4c00000() {
    // Encoding: 0xC4C00000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field xs = 1 (Max)
    // Fields: xs=1, Zm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4C00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zm_0_min_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zm = 0 (Min)
    // Fields: Zm=0, Pg=0, Rn=0, xs=0, Zt=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zm_1_poweroftwo_0_c4810000() {
    // Encoding: 0xC4810000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Pg=0, Zt=0, Rn=0, xs=0, Zm=1
    let encoding: u32 = 0xC4810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zm_30_poweroftwominusone_0_c49e0000() {
    // Encoding: 0xC49E0000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, xs=0, Rn=0, Zm=30, Zt=0
    let encoding: u32 = 0xC49E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zm_31_max_0_c49f0000() {
    // Encoding: 0xC49F0000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zm = 31 (Max)
    // Fields: Rn=0, xs=0, Zt=0, Zm=31, Pg=0
    let encoding: u32 = 0xC49F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_pg_0_min_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Pg = 0 (Min)
    // Fields: Rn=0, Zt=0, Pg=0, xs=0, Zm=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_pg_1_poweroftwo_0_c4800400() {
    // Encoding: 0xC4800400
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: Rn=0, Zm=0, xs=0, Pg=1, Zt=0
    let encoding: u32 = 0xC4800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_rn_0_min_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Rn = 0 (Min)
    // Fields: Zt=0, xs=0, Zm=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_rn_1_poweroftwo_0_c4800020() {
    // Encoding: 0xC4800020
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: Zt=0, Zm=0, Rn=1, Pg=0, xs=0
    let encoding: u32 = 0xC4800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_rn_30_poweroftwominusone_0_c48003c0() {
    // Encoding: 0xC48003C0
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Pg=0, xs=0, Zm=0, Rn=30
    let encoding: u32 = 0xC48003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_rn_31_max_0_c48003e0() {
    // Encoding: 0xC48003E0
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Rn = 31 (Max)
    // Fields: Zm=0, Pg=0, Rn=31, Zt=0, xs=0
    let encoding: u32 = 0xC48003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zt_0_min_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zt = 0 (Min)
    // Fields: Zt=0, xs=0, Zm=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zt_1_poweroftwo_0_c4800001() {
    // Encoding: 0xC4800001
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    let encoding: u32 = 0xC4800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zt_30_poweroftwominusone_0_c480001e() {
    // Encoding: 0xC480001E
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Rn=0, xs=0, Zt=30, Pg=0
    let encoding: u32 = 0xC480001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_field_zt_31_max_0_c480001f() {
    // Encoding: 0xC480001F
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field Zt = 31 (Max)
    // Fields: Zt=31, Rn=0, xs=0, Pg=0, Zm=0
    let encoding: u32 = 0xC480001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_0_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_1_0_c4c00000() {
    // Encoding: 0xC4C00000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, xs=1, Zm=0, Rn=0
    let encoding: u32 = 0xC4C00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_2_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Pg=0, Zm=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_3_0_c4810000() {
    // Encoding: 0xC4810000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, xs=0, Zm=1, Pg=0, Zt=0
    let encoding: u32 = 0xC4810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_4_0_c49e0000() {
    // Encoding: 0xC49E0000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, xs=0, Zm=30, Rn=0, Zt=0
    let encoding: u32 = 0xC49E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_5_0_c49f0000() {
    // Encoding: 0xC49F0000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, xs=0, Rn=0, Zm=31, Zt=0
    let encoding: u32 = 0xC49F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_6_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, xs=0, Rn=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_7_0_c4800400() {
    // Encoding: 0xC4800400
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Zm=0, xs=0, Pg=1
    let encoding: u32 = 0xC4800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_8_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=0, Rn=0, Pg=0, xs=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_9_0_c4800020() {
    // Encoding: 0xC4800020
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Zm=0, Rn=1, Pg=0, xs=0, Zt=0
    let encoding: u32 = 0xC4800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_10_0_c48003c0() {
    // Encoding: 0xC48003C0
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: xs=0, Rn=30, Zt=0, Zm=0, Pg=0
    let encoding: u32 = 0xC48003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_11_0_c48003e0() {
    // Encoding: 0xC48003E0
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Rn=31, Zt=0, Zm=0, Pg=0, xs=0
    let encoding: u32 = 0xC48003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_12_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Zm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_13_0_c4800001() {
    // Encoding: 0xC4800001
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Rn=0, Zt=1, xs=0, Zm=0
    let encoding: u32 = 0xC4800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_14_0_c480001e() {
    // Encoding: 0xC480001E
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Rn=0, Zt=30, Zm=0, Pg=0, xs=0
    let encoding: u32 = 0xC480001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_15_0_c480001f() {
    // Encoding: 0xC480001F
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Rn=0, Zt=31, xs=0, Zm=0, Pg=0
    let encoding: u32 = 0xC480001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_16_0_c4800420() {
    // Encoding: 0xC4800420
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: xs=0, Zm=0, Pg=1, Zt=0, Rn=1
    let encoding: u32 = 0xC4800420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_combo_17_0_c4801fe0() {
    // Encoding: 0xC4801FE0
    // Test LD1SH_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Zt=0, Pg=31, Rn=31, Zm=0, xs=0
    let encoding: u32 = 0xC4801FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_0_c48003e0() {
    // Encoding: 0xC48003E0
    // Test LD1SH_Z.P.BZ_D.x32.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Pg=0, Rn=31, Zt=0, xs=0, Zm=0
    let encoding: u32 = 0xC48003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_invalid_0_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zm=0, Pg=0, Zt=0, xs=0, Rn=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.x32.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_x32_unscaled_invalid_1_0_c4800000() {
    // Encoding: 0xC4800000
    // Test LD1SH_Z.P.BZ_D.x32.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: xs=0, Rn=0, Pg=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_xs_0_min_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field xs = 0 (Min)
    // Fields: Zm=0, xs=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_xs_1_max_0_84c00000() {
    // Encoding: 0x84C00000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field xs = 1 (Max)
    // Fields: Zt=0, Pg=0, xs=1, Zm=0, Rn=0
    let encoding: u32 = 0x84C00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zm_0_min_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zm = 0 (Min)
    // Fields: Pg=0, Zm=0, xs=0, Rn=0, Zt=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zm_1_poweroftwo_0_84810000() {
    // Encoding: 0x84810000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Pg=0, Zt=0, Rn=0, xs=0, Zm=1
    let encoding: u32 = 0x84810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zm_30_poweroftwominusone_0_849e0000() {
    // Encoding: 0x849E0000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zm=30, xs=0, Zt=0, Rn=0
    let encoding: u32 = 0x849E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zm_31_max_0_849f0000() {
    // Encoding: 0x849F0000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zm = 31 (Max)
    // Fields: Pg=0, xs=0, Rn=0, Zt=0, Zm=31
    let encoding: u32 = 0x849F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_pg_0_min_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Pg = 0 (Min)
    // Fields: Zt=0, Pg=0, xs=0, Zm=0, Rn=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_pg_1_poweroftwo_0_84800400() {
    // Encoding: 0x84800400
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: xs=0, Rn=0, Zm=0, Pg=1, Zt=0
    let encoding: u32 = 0x84800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_rn_0_min_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Rn = 0 (Min)
    // Fields: Zm=0, Pg=0, xs=0, Zt=0, Rn=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_rn_1_poweroftwo_0_84800020() {
    // Encoding: 0x84800020
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Zt=0, xs=0, Zm=0, Pg=0
    let encoding: u32 = 0x84800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_rn_30_poweroftwominusone_0_848003c0() {
    // Encoding: 0x848003C0
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Pg=0, Rn=30, Zm=0, Zt=0
    let encoding: u32 = 0x848003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_rn_31_max_0_848003e0() {
    // Encoding: 0x848003E0
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Rn = 31 (Max)
    // Fields: Rn=31, Zt=0, xs=0, Zm=0, Pg=0
    let encoding: u32 = 0x848003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zt_0_min_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zt = 0 (Min)
    // Fields: Zm=0, Zt=0, Rn=0, xs=0, Pg=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zt_1_poweroftwo_0_84800001() {
    // Encoding: 0x84800001
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: Pg=0, Rn=0, Zt=1, xs=0, Zm=0
    let encoding: u32 = 0x84800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zt_30_poweroftwominusone_0_8480001e() {
    // Encoding: 0x8480001E
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zm=0, xs=0, Rn=0, Zt=30
    let encoding: u32 = 0x8480001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_field_zt_31_max_0_8480001f() {
    // Encoding: 0x8480001F
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field Zt = 31 (Max)
    // Fields: Rn=0, xs=0, Zt=31, Zm=0, Pg=0
    let encoding: u32 = 0x8480001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_0_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, xs=0, Zm=0, Pg=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_1_0_84c00000() {
    // Encoding: 0x84C00000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=0, xs=1, Rn=0, Pg=0
    let encoding: u32 = 0x84C00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_2_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Zm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_3_0_84810000() {
    // Encoding: 0x84810000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=1, xs=0, Rn=0, Zt=0
    let encoding: u32 = 0x84810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_4_0_849e0000() {
    // Encoding: 0x849E0000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=30, Pg=0, xs=0, Rn=0
    let encoding: u32 = 0x849E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_5_0_849f0000() {
    // Encoding: 0x849F0000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Rn=0, Zt=0, Zm=31, Pg=0
    let encoding: u32 = 0x849F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_6_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Rn=0, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_7_0_84800400() {
    // Encoding: 0x84800400
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, Zm=0, Pg=1, xs=0, Zt=0
    let encoding: u32 = 0x84800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_8_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, xs=0, Pg=0, Rn=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_9_0_84800020() {
    // Encoding: 0x84800020
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Zm=0, Pg=0, Rn=1, Zt=0, xs=0
    let encoding: u32 = 0x84800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_10_0_848003c0() {
    // Encoding: 0x848003C0
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zm=0, xs=0, Rn=30, Pg=0, Zt=0
    let encoding: u32 = 0x848003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_11_0_848003e0() {
    // Encoding: 0x848003E0
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zt=0, Rn=31, Pg=0, xs=0, Zm=0
    let encoding: u32 = 0x848003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_12_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, xs=0, Pg=0, Zm=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_13_0_84800001() {
    // Encoding: 0x84800001
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Rn=0, Zt=1, Pg=0, xs=0, Zm=0
    let encoding: u32 = 0x84800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_14_0_8480001e() {
    // Encoding: 0x8480001E
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zm=0, Zt=30, xs=0, Pg=0, Rn=0
    let encoding: u32 = 0x8480001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_15_0_8480001f() {
    // Encoding: 0x8480001F
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, Zt=31, Zm=0, xs=0, Rn=0
    let encoding: u32 = 0x8480001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_16_0_84800420() {
    // Encoding: 0x84800420
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: xs=0, Zt=0, Pg=1, Rn=1, Zm=0
    let encoding: u32 = 0x84800420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_combo_17_0_84801fe0() {
    // Encoding: 0x84801FE0
    // Test LD1SH_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: xs=0, Zm=0, Rn=31, Pg=31, Zt=0
    let encoding: u32 = 0x84801FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_0_848003e0() {
    // Encoding: 0x848003E0
    // Test LD1SH_Z.P.BZ_S.x32.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zm=0, xs=0, Rn=31, Pg=0, Zt=0
    let encoding: u32 = 0x848003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_invalid_0_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Zm=0, Rn=0, xs=0, Zt=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_S.x32.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_s_x32_unscaled_invalid_1_0_84800000() {
    // Encoding: 0x84800000
    // Test LD1SH_Z.P.BZ_S.x32.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, xs=0, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0x84800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zm_0_min_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zm = 0 (Min)
    // Fields: Pg=0, Zt=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zm_1_poweroftwo_8000_c4e18000() {
    // Encoding: 0xC4E18000
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zm = 1 (PowerOfTwo)
    // Fields: Zt=0, Rn=0, Pg=0, Zm=1
    let encoding: u32 = 0xC4E18000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zm_30_poweroftwominusone_8000_c4fe8000() {
    // Encoding: 0xC4FE8000
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xC4FE8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zm_31_max_8000_c4ff8000() {
    // Encoding: 0xC4FF8000
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zm = 31 (Max)
    // Fields: Pg=0, Rn=0, Zt=0, Zm=31
    let encoding: u32 = 0xC4FF8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_pg_0_min_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field Pg = 0 (Min)
    // Fields: Zm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_pg_1_poweroftwo_8000_c4e08400() {
    // Encoding: 0xC4E08400
    // Test LD1SH_Z.P.BZ_D.64.scaled field Pg = 1 (PowerOfTwo)
    // Fields: Rn=0, Pg=1, Zt=0, Zm=0
    let encoding: u32 = 0xC4E08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_rn_0_min_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field Rn = 0 (Min)
    // Fields: Pg=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_rn_1_poweroftwo_8000_c4e08020() {
    // Encoding: 0xC4E08020
    // Test LD1SH_Z.P.BZ_D.64.scaled field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zm=0, Rn=1, Zt=0
    let encoding: u32 = 0xC4E08020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_rn_30_poweroftwominusone_8000_c4e083c0() {
    // Encoding: 0xC4E083C0
    // Test LD1SH_Z.P.BZ_D.64.scaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Pg=0, Zm=0, Rn=30
    let encoding: u32 = 0xC4E083C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_rn_31_max_8000_c4e083e0() {
    // Encoding: 0xC4E083E0
    // Test LD1SH_Z.P.BZ_D.64.scaled field Rn = 31 (Max)
    // Fields: Zm=0, Zt=0, Pg=0, Rn=31
    let encoding: u32 = 0xC4E083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zt_0_min_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zt = 0 (Min)
    // Fields: Zm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zt_1_poweroftwo_8000_c4e08001() {
    // Encoding: 0xC4E08001
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zt = 1 (PowerOfTwo)
    // Fields: Zm=0, Pg=0, Zt=1, Rn=0
    let encoding: u32 = 0xC4E08001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zt_30_poweroftwominusone_8000_c4e0801e() {
    // Encoding: 0xC4E0801E
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Zm=0, Pg=0, Zt=30
    let encoding: u32 = 0xC4E0801E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_field_zt_31_max_8000_c4e0801f() {
    // Encoding: 0xC4E0801F
    // Test LD1SH_Z.P.BZ_D.64.scaled field Zt = 31 (Max)
    // Fields: Zm=0, Zt=31, Pg=0, Rn=0
    let encoding: u32 = 0xC4E0801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_0_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_1_8000_c4e18000() {
    // Encoding: 0xC4E18000
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=1, Rn=0, Zt=0
    let encoding: u32 = 0xC4E18000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_2_8000_c4fe8000() {
    // Encoding: 0xC4FE8000
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, Zm=30, Zt=0
    let encoding: u32 = 0xC4FE8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_3_8000_c4ff8000() {
    // Encoding: 0xC4FF8000
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zm=31, Zt=0
    let encoding: u32 = 0xC4FF8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_4_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zm=0, Pg=0, Zt=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_5_8000_c4e08400() {
    // Encoding: 0xC4E08400
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Pg=1, Rn=0
    let encoding: u32 = 0xC4E08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_6_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Zm=0, Pg=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_7_8000_c4e08020() {
    // Encoding: 0xC4E08020
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Zt=0, Pg=0, Zm=0
    let encoding: u32 = 0xC4E08020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_8_8000_c4e083c0() {
    // Encoding: 0xC4E083C0
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zt=0, Pg=0, Zm=0, Rn=30
    let encoding: u32 = 0xC4E083C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_9_8000_c4e083e0() {
    // Encoding: 0xC4E083E0
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Pg=0, Rn=31, Zt=0, Zm=0
    let encoding: u32 = 0xC4E083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_10_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_11_8000_c4e08001() {
    // Encoding: 0xC4E08001
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Zm=0, Rn=0, Zt=1
    let encoding: u32 = 0xC4E08001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_12_8000_c4e0801e() {
    // Encoding: 0xC4E0801E
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zt=30, Zm=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4E0801E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_13_8000_c4e0801f() {
    // Encoding: 0xC4E0801F
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zm=0, Rn=0, Zt=31, Pg=0
    let encoding: u32 = 0xC4E0801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_14_8000_c4e08420() {
    // Encoding: 0xC4E08420
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Rn=1, Pg=1, Zm=0, Zt=0
    let encoding: u32 = 0xC4E08420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_combo_15_8000_c4e09fe0() {
    // Encoding: 0xC4E09FE0
    // Test LD1SH_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Zm=0, Zt=0, Rn=31
    let encoding: u32 = 0xC4E09FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_special_rn_31_stack_pointer_sp_may_require_alignment_32768_c4e083e0() {
    // Encoding: 0xC4E083E0
    // Test LD1SH_Z.P.BZ_D.64.scaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zm=0, Pg=0, Zt=0, Rn=31
    let encoding: u32 = 0xC4E083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_invalid_0_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Zm=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.scaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_64_scaled_invalid_1_8000_c4e08000() {
    // Encoding: 0xC4E08000
    // Test LD1SH_Z.P.BZ_D.64.scaled invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Rn=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4E08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zm_0_min_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zm = 0 (Min)
    // Fields: Pg=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zm_1_poweroftwo_8000_c4c18000() {
    // Encoding: 0xC4C18000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Zt=0, Rn=0, Pg=0, Zm=1
    let encoding: u32 = 0xC4C18000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zm_30_poweroftwominusone_8000_c4de8000() {
    // Encoding: 0xC4DE8000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Rn=0, Pg=0, Zm=30
    let encoding: u32 = 0xC4DE8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zm_31_max_8000_c4df8000() {
    // Encoding: 0xC4DF8000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zm = 31 (Max)
    // Fields: Zt=0, Zm=31, Rn=0, Pg=0
    let encoding: u32 = 0xC4DF8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_pg_0_min_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Pg = 0 (Min)
    // Fields: Zt=0, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_pg_1_poweroftwo_8000_c4c08400() {
    // Encoding: 0xC4C08400
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: Zm=0, Zt=0, Rn=0, Pg=1
    let encoding: u32 = 0xC4C08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_rn_0_min_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Rn = 0 (Min)
    // Fields: Rn=0, Pg=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_rn_1_poweroftwo_8000_c4c08020() {
    // Encoding: 0xC4C08020
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zm=0, Zt=0, Rn=1
    let encoding: u32 = 0xC4C08020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_rn_30_poweroftwominusone_8000_c4c083c0() {
    // Encoding: 0xC4C083C0
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Zt=0, Zm=0, Pg=0
    let encoding: u32 = 0xC4C083C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_rn_31_max_8000_c4c083e0() {
    // Encoding: 0xC4C083E0
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Rn = 31 (Max)
    // Fields: Zm=0, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xC4C083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zt_0_min_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zt = 0 (Min)
    // Fields: Rn=0, Zt=0, Zm=0, Pg=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zt_1_poweroftwo_8000_c4c08001() {
    // Encoding: 0xC4C08001
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: Zm=0, Zt=1, Pg=0, Rn=0
    let encoding: u32 = 0xC4C08001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zt_30_poweroftwominusone_8000_c4c0801e() {
    // Encoding: 0xC4C0801E
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zm=0, Zt=30, Rn=0
    let encoding: u32 = 0xC4C0801E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_field_zt_31_max_8000_c4c0801f() {
    // Encoding: 0xC4C0801F
    // Test LD1SH_Z.P.BZ_D.64.unscaled field Zt = 31 (Max)
    // Fields: Zt=31, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4C0801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_0_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_1_8000_c4c18000() {
    // Encoding: 0xC4C18000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zm=1, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xC4C18000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_2_8000_c4de8000() {
    // Encoding: 0xC4DE8000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Zm=30, Rn=0
    let encoding: u32 = 0xC4DE8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_3_8000_c4df8000() {
    // Encoding: 0xC4DF8000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zm=31, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xC4DF8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_4_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_5_8000_c4c08400() {
    // Encoding: 0xC4C08400
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Zm=0, Pg=1, Rn=0, Zt=0
    let encoding: u32 = 0xC4C08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_6_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Zm=0, Pg=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_7_8000_c4c08020() {
    // Encoding: 0xC4C08020
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Zm=0, Zt=0, Rn=1
    let encoding: u32 = 0xC4C08020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_8_8000_c4c083c0() {
    // Encoding: 0xC4C083C0
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zm=0, Rn=30, Pg=0, Zt=0
    let encoding: u32 = 0xC4C083C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_9_8000_c4c083e0() {
    // Encoding: 0xC4C083E0
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zm=0, Pg=0, Zt=0, Rn=31
    let encoding: u32 = 0xC4C083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_10_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_11_8000_c4c08001() {
    // Encoding: 0xC4C08001
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Zt=1, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4C08001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_12_8000_c4c0801e() {
    // Encoding: 0xC4C0801E
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zm=0, Rn=0, Zt=30, Pg=0
    let encoding: u32 = 0xC4C0801E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_13_8000_c4c0801f() {
    // Encoding: 0xC4C0801F
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zt=31, Zm=0, Rn=0, Pg=0
    let encoding: u32 = 0xC4C0801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_14_8000_c4c08420() {
    // Encoding: 0xC4C08420
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Zm=0, Pg=1, Zt=0, Rn=1
    let encoding: u32 = 0xC4C08420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_combo_15_8000_c4c09fe0() {
    // Encoding: 0xC4C09FE0
    // Test LD1SH_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Zt=0, Zm=0, Pg=31, Rn=31
    let encoding: u32 = 0xC4C09FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_32768_c4c083e0() {
    // Encoding: 0xC4C083E0
    // Test LD1SH_Z.P.BZ_D.64.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zt=0, Rn=31, Pg=0, Zm=0
    let encoding: u32 = 0xC4C083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_invalid_0_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Rn=0, Zt=0, Pg=0, Zm=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1SH_Z.P.BZ_D.64.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1sh_z_p_bz_d_64_unscaled_invalid_1_8000_c4c08000() {
    // Encoding: 0xC4C08000
    // Test LD1SH_Z.P.BZ_D.64.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Zm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4C08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD1RQD_Z.P.BR_Contiguous Tests
// ============================================================================

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rm_0_min_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field Rm = 0 (Min)
    // Fields: Rm=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rm_1_poweroftwo_0_a5810000() {
    // Encoding: 0xA5810000
    // Test LD1RQD_Z.P.BR_Contiguous field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rm_30_poweroftwominusone_0_a59e0000() {
    // Encoding: 0xA59E0000
    // Test LD1RQD_Z.P.BR_Contiguous field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Rm=30, Rn=0, Pg=0
    let encoding: u32 = 0xA59E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rm_31_max_0_a59f0000() {
    // Encoding: 0xA59F0000
    // Test LD1RQD_Z.P.BR_Contiguous field Rm = 31 (Max)
    // Fields: Zt=0, Rn=0, Pg=0, Rm=31
    let encoding: u32 = 0xA59F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_pg_0_min_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field Pg = 0 (Min)
    // Fields: Rn=0, Rm=0, Pg=0, Zt=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_pg_1_poweroftwo_0_a5800400() {
    // Encoding: 0xA5800400
    // Test LD1RQD_Z.P.BR_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Zt=0, Pg=1, Rm=0, Rn=0
    let encoding: u32 = 0xA5800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rn_0_min_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field Rn = 0 (Min)
    // Fields: Rn=0, Pg=0, Rm=0, Zt=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rn_1_poweroftwo_0_a5800020() {
    // Encoding: 0xA5800020
    // Test LD1RQD_Z.P.BR_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rm=0, Zt=0, Pg=0
    let encoding: u32 = 0xA5800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rn_30_poweroftwominusone_0_a58003c0() {
    // Encoding: 0xA58003C0
    // Test LD1RQD_Z.P.BR_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Pg=0, Rn=30, Zt=0
    let encoding: u32 = 0xA58003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_rn_31_max_0_a58003e0() {
    // Encoding: 0xA58003E0
    // Test LD1RQD_Z.P.BR_Contiguous field Rn = 31 (Max)
    // Fields: Zt=0, Rm=0, Pg=0, Rn=31
    let encoding: u32 = 0xA58003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_zt_0_min_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field Zt = 0 (Min)
    // Fields: Pg=0, Rm=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_zt_1_poweroftwo_0_a5800001() {
    // Encoding: 0xA5800001
    // Test LD1RQD_Z.P.BR_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Zt=1, Pg=0, Rm=0, Rn=0
    let encoding: u32 = 0xA5800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_zt_30_poweroftwominusone_0_a580001e() {
    // Encoding: 0xA580001E
    // Test LD1RQD_Z.P.BR_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Pg=0, Zt=30, Rn=0
    let encoding: u32 = 0xA580001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1rqd_z_p_br_contiguous_field_zt_31_max_0_a580001f() {
    // Encoding: 0xA580001F
    // Test LD1RQD_Z.P.BR_Contiguous field Zt = 31 (Max)
    // Fields: Rm=0, Rn=0, Zt=31, Pg=0
    let encoding: u32 = 0xA580001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_0_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (register index 1 (second register))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_1_0_a5810000() {
    // Encoding: 0xA5810000
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=0, Zt=0
    // Fields: Rm=1, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_2_0_a59e0000() {
    // Encoding: 0xA59E0000
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rm=30, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA59E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (register index 31 (special))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_3_0_a59f0000() {
    // Encoding: 0xA59F0000
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=0, Zt=0
    // Fields: Rm=31, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA59F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_4_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Rm=0, Rn=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_5_0_a5800400() {
    // Encoding: 0xA5800400
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, Pg=1, Rm=0, Zt=0
    let encoding: u32 = 0xA5800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_6_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Rm=0, Zt=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_7_0_a5800020() {
    // Encoding: 0xA5800020
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rm=0, Rn=1, Zt=0, Pg=0
    let encoding: u32 = 0xA5800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_8_0_a58003c0() {
    // Encoding: 0xA58003C0
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rm=0, Pg=0, Rn=30, Zt=0
    let encoding: u32 = 0xA58003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_9_0_a58003e0() {
    // Encoding: 0xA58003E0
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=31, Zt=0
    // Fields: Rn=31, Zt=0, Rm=0, Pg=0
    let encoding: u32 = 0xA58003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_10_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_11_0_a5800001() {
    // Encoding: 0xA5800001
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=1
    // Fields: Rn=0, Rm=0, Pg=0, Zt=1
    let encoding: u32 = 0xA5800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_12_0_a580001e() {
    // Encoding: 0xA580001E
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=30
    // Fields: Pg=0, Rm=0, Rn=0, Zt=30
    let encoding: u32 = 0xA580001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_13_0_a580001f() {
    // Encoding: 0xA580001F
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=31
    // Fields: Rm=0, Zt=31, Rn=0, Pg=0
    let encoding: u32 = 0xA580001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Pg=1 (same register test (reg=1))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_14_0_a5810400() {
    // Encoding: 0xA5810400
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=1, Pg=1, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Rm=1, Pg=1
    let encoding: u32 = 0xA5810400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Pg=31 (same register test (reg=31))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_15_0_a59f1c00() {
    // Encoding: 0xA59F1C00
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=31, Pg=31, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Rm=31, Pg=31
    let encoding: u32 = 0xA59F1C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_16_0_a5810020() {
    // Encoding: 0xA5810020
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=1, Zt=0
    // Fields: Rm=1, Zt=0, Pg=0, Rn=1
    let encoding: u32 = 0xA5810020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_17_0_a59f03e0() {
    // Encoding: 0xA59F03E0
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=31, Zt=0
    // Fields: Rm=31, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xA59F03E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_18_0_a5800420() {
    // Encoding: 0xA5800420
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=1, Zt=0
    // Fields: Rm=0, Pg=1, Rn=1, Zt=0
    let encoding: u32 = 0xA5800420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field combination 19`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1rqd_z_p_br_contiguous_combo_19_0_a5801fe0() {
    // Encoding: 0xA5801FE0
    // Test LD1RQD_Z.P.BR_Contiguous field combination: Rm=0, Pg=31, Rn=31, Zt=0
    // Fields: Rn=31, Rm=0, Pg=31, Zt=0
    let encoding: u32 = 0xA5801FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1rqd_z_p_br_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_0_a58003e0() {
    // Encoding: 0xA58003E0
    // Test LD1RQD_Z.P.BR_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Pg=0, Rm=0, Zt=0
    let encoding: u32 = 0xA58003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1rqd_z_p_br_contiguous_invalid_0_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Zt=0, Rm=0, Rn=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1rqd_z_p_br_contiguous_invalid_1_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, Rm=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Rm\" }), rhs: LitBits([true, true, true, true, true]) }" }
/// triggers Undefined
#[test]
fn test_ld1rqd_z_p_br_contiguous_invalid_2_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous invalid encoding: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }
    // Fields: Rm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1RQD_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1rqd_z_p_br_contiguous_invalid_3_0_a5800000() {
    // Encoding: 0xA5800000
    // Test LD1RQD_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xA5800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD2D_Z.P.BR_Contiguous Tests
// ============================================================================

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rm_0_min_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field Rm = 0 (Min)
    // Fields: Rm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rm_1_poweroftwo_c000_a5a1c000() {
    // Encoding: 0xA5A1C000
    // Test LD2D_Z.P.BR_Contiguous field Rm = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=1, Pg=0, Zt=0
    let encoding: u32 = 0xA5A1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rm_30_poweroftwominusone_c000_a5bec000() {
    // Encoding: 0xA5BEC000
    // Test LD2D_Z.P.BR_Contiguous field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Pg=0, Rm=30, Rn=0
    let encoding: u32 = 0xA5BEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rm_31_max_c000_a5bfc000() {
    // Encoding: 0xA5BFC000
    // Test LD2D_Z.P.BR_Contiguous field Rm = 31 (Max)
    // Fields: Zt=0, Pg=0, Rn=0, Rm=31
    let encoding: u32 = 0xA5BFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld2d_z_p_br_contiguous_field_pg_0_min_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field Pg = 0 (Min)
    // Fields: Rm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld2d_z_p_br_contiguous_field_pg_1_poweroftwo_c000_a5a0c400() {
    // Encoding: 0xA5A0C400
    // Test LD2D_Z.P.BR_Contiguous field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Rn=0, Rm=0, Zt=0
    let encoding: u32 = 0xA5A0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rn_0_min_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field Rn = 0 (Min)
    // Fields: Pg=0, Rm=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rn_1_poweroftwo_c000_a5a0c020() {
    // Encoding: 0xA5A0C020
    // Test LD2D_Z.P.BR_Contiguous field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Pg=0, Rn=1, Zt=0
    let encoding: u32 = 0xA5A0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rn_30_poweroftwominusone_c000_a5a0c3c0() {
    // Encoding: 0xA5A0C3C0
    // Test LD2D_Z.P.BR_Contiguous field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zt=0, Rn=30, Rm=0
    let encoding: u32 = 0xA5A0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld2d_z_p_br_contiguous_field_rn_31_max_c000_a5a0c3e0() {
    // Encoding: 0xA5A0C3E0
    // Test LD2D_Z.P.BR_Contiguous field Rn = 31 (Max)
    // Fields: Zt=0, Rm=0, Rn=31, Pg=0
    let encoding: u32 = 0xA5A0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld2d_z_p_br_contiguous_field_zt_0_min_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field Zt = 0 (Min)
    // Fields: Rn=0, Pg=0, Rm=0, Zt=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld2d_z_p_br_contiguous_field_zt_1_poweroftwo_c000_a5a0c001() {
    // Encoding: 0xA5A0C001
    // Test LD2D_Z.P.BR_Contiguous field Zt = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, Zt=1, Pg=0
    let encoding: u32 = 0xA5A0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld2d_z_p_br_contiguous_field_zt_30_poweroftwominusone_c000_a5a0c01e() {
    // Encoding: 0xA5A0C01E
    // Test LD2D_Z.P.BR_Contiguous field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Pg=0, Zt=30, Rn=0
    let encoding: u32 = 0xA5A0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld2d_z_p_br_contiguous_field_zt_31_max_c000_a5a0c01f() {
    // Encoding: 0xA5A0C01F
    // Test LD2D_Z.P.BR_Contiguous field Zt = 31 (Max)
    // Fields: Pg=0, Zt=31, Rm=0, Rn=0
    let encoding: u32 = 0xA5A0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_0_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Pg=0, Rm=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (register index 1 (second register))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_1_c000_a5a1c000() {
    // Encoding: 0xA5A1C000
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rm=1, Pg=0, Rn=0
    let encoding: u32 = 0xA5A1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_2_c000_a5bec000() {
    // Encoding: 0xA5BEC000
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Rm=30, Pg=0, Zt=0
    let encoding: u32 = 0xA5BEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (register index 31 (special))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_3_c000_a5bfc000() {
    // Encoding: 0xA5BFC000
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=0, Zt=0
    // Fields: Rm=31, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5BFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_4_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_5_c000_a5a0c400() {
    // Encoding: 0xA5A0C400
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Rm=0, Pg=1
    let encoding: u32 = 0xA5A0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_6_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Rm=0, Zt=0, Pg=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_7_c000_a5a0c020() {
    // Encoding: 0xA5A0C020
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Zt=0, Rm=0, Rn=1
    let encoding: u32 = 0xA5A0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_8_c000_a5a0c3c0() {
    // Encoding: 0xA5A0C3C0
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rm=0, Pg=0, Zt=0, Rn=30
    let encoding: u32 = 0xA5A0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_9_c000_a5a0c3e0() {
    // Encoding: 0xA5A0C3E0
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=31, Zt=0
    // Fields: Rn=31, Rm=0, Zt=0, Pg=0
    let encoding: u32 = 0xA5A0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld2d_z_p_br_contiguous_combo_10_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld2d_z_p_br_contiguous_combo_11_c000_a5a0c001() {
    // Encoding: 0xA5A0C001
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Rn=0, Rm=0, Zt=1
    let encoding: u32 = 0xA5A0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld2d_z_p_br_contiguous_combo_12_c000_a5a0c01e() {
    // Encoding: 0xA5A0C01E
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zt=30, Rm=0, Pg=0, Rn=0
    let encoding: u32 = 0xA5A0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld2d_z_p_br_contiguous_combo_13_c000_a5a0c01f() {
    // Encoding: 0xA5A0C01F
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=0, Rn=0, Zt=31
    // Fields: Rm=0, Zt=31, Rn=0, Pg=0
    let encoding: u32 = 0xA5A0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Pg=1 (same register test (reg=1))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_14_c000_a5a1c400() {
    // Encoding: 0xA5A1C400
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=1, Pg=1, Rn=0, Zt=0
    // Fields: Zt=0, Rm=1, Pg=1, Rn=0
    let encoding: u32 = 0xA5A1C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Pg=31 (same register test (reg=31))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_15_c000_a5bfdc00() {
    // Encoding: 0xA5BFDC00
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=31, Pg=31, Rn=0, Zt=0
    // Fields: Pg=31, Rm=31, Rn=0, Zt=0
    let encoding: u32 = 0xA5BFDC00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_16_c000_a5a1c020() {
    // Encoding: 0xA5A1C020
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=1, Pg=0, Rn=1, Zt=0
    // Fields: Rm=1, Rn=1, Pg=0, Zt=0
    let encoding: u32 = 0xA5A1C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_17_c000_a5bfc3e0() {
    // Encoding: 0xA5BFC3E0
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=31, Pg=0, Rn=31, Zt=0
    // Fields: Pg=0, Rn=31, Zt=0, Rm=31
    let encoding: u32 = 0xA5BFC3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(matches!(exit, Ok(CpuExit::Undefined(_))) || matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected unallocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 18`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_18_c000_a5a0c420() {
    // Encoding: 0xA5A0C420
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=1, Rn=1, Zt=0
    // Fields: Rm=0, Rn=1, Pg=1, Zt=0
    let encoding: u32 = 0xA5A0C420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field combination 19`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld2d_z_p_br_contiguous_combo_19_c000_a5a0dfe0() {
    // Encoding: 0xA5A0DFE0
    // Test LD2D_Z.P.BR_Contiguous field combination: Rm=0, Pg=31, Rn=31, Zt=0
    // Fields: Zt=0, Rn=31, Pg=31, Rm=0
    let encoding: u32 = 0xA5A0DFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld2d_z_p_br_contiguous_special_rn_31_stack_pointer_sp_may_require_alignment_49152_a5a0c3e0() {
    // Encoding: 0xA5A0C3E0
    // Test LD2D_Z.P.BR_Contiguous special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zt=0, Pg=0, Rm=0, Rn=31
    let encoding: u32 = 0xA5A0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld2d_z_p_br_contiguous_invalid_0_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Rm=0, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld2d_z_p_br_contiguous_invalid_1_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Rm\" }), rhs: LitBits([true, true, true, true, true]) }" }
/// triggers Undefined
#[test]
fn test_ld2d_z_p_br_contiguous_invalid_2_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous invalid encoding: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Rm" }), rhs: LitBits([true, true, true, true, true]) }
    // Fields: Rn=0, Zt=0, Rm=0, Pg=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD2D_Z.P.BR_Contiguous
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld2d_z_p_br_contiguous_invalid_3_c000_a5a0c000() {
    // Encoding: 0xA5A0C000
    // Test LD2D_Z.P.BR_Contiguous invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, Pg=0, Rn=0, Rm=0
    let encoding: u32 = 0xA5A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

// ============================================================================
// LD1H_Z.P.BZ_S.x32.scaled Tests
// ============================================================================

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_xs_0_min_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field xs = 0 (Min)
    // Fields: Zm=0, Rn=0, Pg=0, Zt=0, xs=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_xs_1_max_4000_84e04000() {
    // Encoding: 0x84E04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field xs = 1 (Max)
    // Fields: Zt=0, Zm=0, Rn=0, Pg=0, xs=1
    let encoding: u32 = 0x84E04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zm_0_min_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zm = 0 (Min)
    // Fields: Zt=0, Zm=0, Rn=0, xs=0, Pg=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zm_1_poweroftwo_4000_84a14000() {
    // Encoding: 0x84A14000
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zm = 1 (PowerOfTwo)
    // Fields: Pg=0, xs=0, Rn=0, Zt=0, Zm=1
    let encoding: u32 = 0x84A14000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zm_30_poweroftwominusone_4000_84be4000() {
    // Encoding: 0x84BE4000
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Pg=0, xs=0, Rn=0, Zt=0
    let encoding: u32 = 0x84BE4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zm_31_max_4000_84bf4000() {
    // Encoding: 0x84BF4000
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zm = 31 (Max)
    // Fields: xs=0, Pg=0, Zm=31, Rn=0, Zt=0
    let encoding: u32 = 0x84BF4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_pg_0_min_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field Pg = 0 (Min)
    // Fields: Rn=0, Zm=0, Pg=0, xs=0, Zt=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_pg_1_poweroftwo_4000_84a04400() {
    // Encoding: 0x84A04400
    // Test LD1H_Z.P.BZ_S.x32.scaled field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Zm=0, xs=0, Zt=0, Rn=0
    let encoding: u32 = 0x84A04400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_rn_0_min_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field Rn = 0 (Min)
    // Fields: xs=0, Zt=0, Rn=0, Zm=0, Pg=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_rn_1_poweroftwo_4000_84a04020() {
    // Encoding: 0x84A04020
    // Test LD1H_Z.P.BZ_S.x32.scaled field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zm=0, xs=0, Rn=1, Zt=0
    let encoding: u32 = 0x84A04020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_rn_30_poweroftwominusone_4000_84a043c0() {
    // Encoding: 0x84A043C0
    // Test LD1H_Z.P.BZ_S.x32.scaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Zm=0, Rn=30, Zt=0, Pg=0
    let encoding: u32 = 0x84A043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_rn_31_max_4000_84a043e0() {
    // Encoding: 0x84A043E0
    // Test LD1H_Z.P.BZ_S.x32.scaled field Rn = 31 (Max)
    // Fields: xs=0, Zm=0, Zt=0, Rn=31, Pg=0
    let encoding: u32 = 0x84A043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zt_0_min_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zt = 0 (Min)
    // Fields: Zm=0, Zt=0, xs=0, Rn=0, Pg=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zt_1_poweroftwo_4000_84a04001() {
    // Encoding: 0x84A04001
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zt = 1 (PowerOfTwo)
    // Fields: xs=0, Pg=0, Zt=1, Zm=0, Rn=0
    let encoding: u32 = 0x84A04001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zt_30_poweroftwominusone_4000_84a0401e() {
    // Encoding: 0x84A0401E
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Zm=0, Zt=30, Pg=0, Rn=0
    let encoding: u32 = 0x84A0401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_field_zt_31_max_4000_84a0401f() {
    // Encoding: 0x84A0401F
    // Test LD1H_Z.P.BZ_S.x32.scaled field Zt = 31 (Max)
    // Fields: Zm=0, Pg=0, Rn=0, xs=0, Zt=31
    let encoding: u32 = 0x84A0401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_0_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Zm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_1_4000_84e04000() {
    // Encoding: 0x84E04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=1, Pg=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0x84E04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_2_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, xs=0, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_3_4000_84a14000() {
    // Encoding: 0x84A14000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Pg=0, Zt=0, Zm=1, Rn=0
    let encoding: u32 = 0x84A14000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_4_4000_84be4000() {
    // Encoding: 0x84BE4000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, xs=0, Pg=0, Zm=30
    let encoding: u32 = 0x84BE4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_5_4000_84bf4000() {
    // Encoding: 0x84BF4000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Rn=0, Zm=31, Zt=0, Pg=0
    let encoding: u32 = 0x84BF4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_6_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Rn=0, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_7_4000_84a04400() {
    // Encoding: 0x84A04400
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Zm=0, Pg=1, Zt=0, Rn=0, xs=0
    let encoding: u32 = 0x84A04400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_8_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Pg=0, Rn=0, Zt=0, xs=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_9_4000_84a04020() {
    // Encoding: 0x84A04020
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: xs=0, Pg=0, Zm=0, Zt=0, Rn=1
    let encoding: u32 = 0x84A04020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_10_4000_84a043c0() {
    // Encoding: 0x84A043C0
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zm=0, Zt=0, xs=0, Rn=30, Pg=0
    let encoding: u32 = 0x84A043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_11_4000_84a043e0() {
    // Encoding: 0x84A043E0
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: xs=0, Zm=0, Rn=31, Pg=0, Zt=0
    let encoding: u32 = 0x84A043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_12_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Zt=0, Zm=0, Rn=0, Pg=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_13_4000_84a04001() {
    // Encoding: 0x84A04001
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: xs=0, Zt=1, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0x84A04001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_14_4000_84a0401e() {
    // Encoding: 0x84A0401E
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zm=0, xs=0, Zt=30, Pg=0, Rn=0
    let encoding: u32 = 0x84A0401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_15_4000_84a0401f() {
    // Encoding: 0x84A0401F
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zm=0, Pg=0, Zt=31, Rn=0, xs=0
    let encoding: u32 = 0x84A0401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_16_4000_84a04420() {
    // Encoding: 0x84A04420
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Rn=1, Zt=0, xs=0, Pg=1, Zm=0
    let encoding: u32 = 0x84A04420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_combo_17_4000_84a05fe0() {
    // Encoding: 0x84A05FE0
    // Test LD1H_Z.P.BZ_S.x32.scaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Zm=0, Rn=31, xs=0, Pg=31, Zt=0
    let encoding: u32 = 0x84A05FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_special_rn_31_stack_pointer_sp_may_require_alignment_16384_84a043e0() {
    // Encoding: 0x84A043E0
    // Test LD1H_Z.P.BZ_S.x32.scaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zt=0, Zm=0, Pg=0, Rn=31, xs=0
    let encoding: u32 = 0x84A043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_invalid_0_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Zt=0, xs=0, Rn=0, Zm=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.scaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_s_x32_scaled_invalid_1_4000_84a04000() {
    // Encoding: 0x84A04000
    // Test LD1H_Z.P.BZ_S.x32.scaled invalid encoding: Unconditional UNDEFINED
    // Fields: Zm=0, Pg=0, xs=0, Rn=0, Zt=0
    let encoding: u32 = 0x84A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_xs_0_min_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field xs = 0 (Min)
    // Fields: xs=0, Pg=0, Zt=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_xs_1_max_4000_c4e04000() {
    // Encoding: 0xC4E04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field xs = 1 (Max)
    // Fields: Pg=0, Zm=0, Rn=0, xs=1, Zt=0
    let encoding: u32 = 0xC4E04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zm_0_min_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zm = 0 (Min)
    // Fields: Rn=0, Zt=0, xs=0, Pg=0, Zm=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zm_1_poweroftwo_4000_c4a14000() {
    // Encoding: 0xC4A14000
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zm = 1 (PowerOfTwo)
    // Fields: Rn=0, Zt=0, xs=0, Zm=1, Pg=0
    let encoding: u32 = 0xC4A14000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zm_30_poweroftwominusone_4000_c4be4000() {
    // Encoding: 0xC4BE4000
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Zm=30, xs=0, Rn=0, Pg=0
    let encoding: u32 = 0xC4BE4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zm_31_max_4000_c4bf4000() {
    // Encoding: 0xC4BF4000
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zm = 31 (Max)
    // Fields: xs=0, Pg=0, Zm=31, Rn=0, Zt=0
    let encoding: u32 = 0xC4BF4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_pg_0_min_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field Pg = 0 (Min)
    // Fields: Rn=0, Zt=0, xs=0, Pg=0, Zm=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_pg_1_poweroftwo_4000_c4a04400() {
    // Encoding: 0xC4A04400
    // Test LD1H_Z.P.BZ_D.x32.scaled field Pg = 1 (PowerOfTwo)
    // Fields: Zm=0, Zt=0, xs=0, Pg=1, Rn=0
    let encoding: u32 = 0xC4A04400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_rn_0_min_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field Rn = 0 (Min)
    // Fields: Zm=0, Pg=0, Rn=0, Zt=0, xs=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_rn_1_poweroftwo_4000_c4a04020() {
    // Encoding: 0xC4A04020
    // Test LD1H_Z.P.BZ_D.x32.scaled field Rn = 1 (PowerOfTwo)
    // Fields: xs=0, Zt=0, Rn=1, Zm=0, Pg=0
    let encoding: u32 = 0xC4A04020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_rn_30_poweroftwominusone_4000_c4a043c0() {
    // Encoding: 0xC4A043C0
    // Test LD1H_Z.P.BZ_D.x32.scaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Pg=0, Zt=0, xs=0, Zm=0
    let encoding: u32 = 0xC4A043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_rn_31_max_4000_c4a043e0() {
    // Encoding: 0xC4A043E0
    // Test LD1H_Z.P.BZ_D.x32.scaled field Rn = 31 (Max)
    // Fields: Zt=0, Pg=0, Rn=31, xs=0, Zm=0
    let encoding: u32 = 0xC4A043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zt_0_min_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zt = 0 (Min)
    // Fields: Zm=0, Pg=0, Zt=0, xs=0, Rn=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zt_1_poweroftwo_4000_c4a04001() {
    // Encoding: 0xC4A04001
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zt = 1 (PowerOfTwo)
    // Fields: Zt=1, xs=0, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4A04001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zt_30_poweroftwominusone_4000_c4a0401e() {
    // Encoding: 0xC4A0401E
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: xs=0, Zm=0, Rn=0, Zt=30, Pg=0
    let encoding: u32 = 0xC4A0401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_field_zt_31_max_4000_c4a0401f() {
    // Encoding: 0xC4A0401F
    // Test LD1H_Z.P.BZ_D.x32.scaled field Zt = 31 (Max)
    // Fields: Rn=0, xs=0, Pg=0, Zm=0, Zt=31
    let encoding: u32 = 0xC4A0401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_0_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Rn=0, Pg=0, xs=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_1_4000_c4e04000() {
    // Encoding: 0xC4E04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=1, Rn=0, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4E04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_2_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, xs=0, Rn=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_3_4000_c4a14000() {
    // Encoding: 0xC4A14000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zt=0, Zm=1, xs=0
    let encoding: u32 = 0xC4A14000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_4_4000_c4be4000() {
    // Encoding: 0xC4BE4000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=30, Rn=0, Zt=0, xs=0
    let encoding: u32 = 0xC4BE4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_5_4000_c4bf4000() {
    // Encoding: 0xC4BF4000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Zm=31, Zt=0, Rn=0, Pg=0
    let encoding: u32 = 0xC4BF4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_6_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Zm=0, Rn=0, xs=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_7_4000_c4a04400() {
    // Encoding: 0xC4A04400
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Zt=0, xs=0, Zm=0, Rn=0, Pg=1
    let encoding: u32 = 0xC4A04400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_8_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: xs=0, Pg=0, Zt=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_9_4000_c4a04020() {
    // Encoding: 0xC4A04020
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Pg=0, Zt=0, Rn=1, xs=0, Zm=0
    let encoding: u32 = 0xC4A04020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_10_4000_c4a043c0() {
    // Encoding: 0xC4A043C0
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Zm=0, Zt=0, Pg=0, Rn=30, xs=0
    let encoding: u32 = 0xC4A043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_11_4000_c4a043e0() {
    // Encoding: 0xC4A043E0
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zm=0, Pg=0, Rn=31, Zt=0, xs=0
    let encoding: u32 = 0xC4A043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_12_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, xs=0, Zm=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_13_4000_c4a04001() {
    // Encoding: 0xC4A04001
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Zm=0, Pg=0, Rn=0, xs=0, Zt=1
    let encoding: u32 = 0xC4A04001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_14_4000_c4a0401e() {
    // Encoding: 0xC4A0401E
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zm=0, Pg=0, xs=0, Zt=30, Rn=0
    let encoding: u32 = 0xC4A0401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_15_4000_c4a0401f() {
    // Encoding: 0xC4A0401F
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zm=0, Pg=0, xs=0, Rn=0, Zt=31
    let encoding: u32 = 0xC4A0401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_16_4000_c4a04420() {
    // Encoding: 0xC4A04420
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: xs=0, Pg=1, Rn=1, Zt=0, Zm=0
    let encoding: u32 = 0xC4A04420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_combo_17_4000_c4a05fe0() {
    // Encoding: 0xC4A05FE0
    // Test LD1H_Z.P.BZ_D.x32.scaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Rn=31, xs=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4A05FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_special_rn_31_stack_pointer_sp_may_require_alignment_16384_c4a043e0() {
    // Encoding: 0xC4A043E0
    // Test LD1H_Z.P.BZ_D.x32.scaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    let encoding: u32 = 0xC4A043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_invalid_0_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Rn=0, Zm=0, xs=0, Pg=0, Zt=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.scaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_x32_scaled_invalid_1_4000_c4a04000() {
    // Encoding: 0xC4A04000
    // Test LD1H_Z.P.BZ_D.x32.scaled invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, xs=0, Zm=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_xs_0_min_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field xs = 0 (Min)
    // Fields: Zt=0, Zm=0, Pg=0, xs=0, Rn=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_xs_1_max_4000_c4c04000() {
    // Encoding: 0xC4C04000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field xs = 1 (Max)
    // Fields: Rn=0, xs=1, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zm_0_min_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zm = 0 (Min)
    // Fields: Pg=0, xs=0, Zt=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zm_1_poweroftwo_4000_c4814000() {
    // Encoding: 0xC4814000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4814000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zm_30_poweroftwominusone_4000_c49e4000() {
    // Encoding: 0xC49E4000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0xC49E4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zm_31_max_4000_c49f4000() {
    // Encoding: 0xC49F4000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zm = 31 (Max)
    // Fields: xs=0, Zt=0, Pg=0, Rn=0, Zm=31
    let encoding: u32 = 0xC49F4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_pg_0_min_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Pg = 0 (Min)
    // Fields: Zt=0, Pg=0, xs=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_pg_1_poweroftwo_4000_c4804400() {
    // Encoding: 0xC4804400
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: Zm=0, Pg=1, xs=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4804400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_rn_0_min_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Rn = 0 (Min)
    // Fields: xs=0, Zt=0, Rn=0, Zm=0, Pg=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_rn_1_poweroftwo_4000_c4804020() {
    // Encoding: 0xC4804020
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: xs=0, Rn=1, Zt=0, Pg=0, Zm=0
    let encoding: u32 = 0xC4804020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_rn_30_poweroftwominusone_4000_c48043c0() {
    // Encoding: 0xC48043C0
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Zm=0, Zt=0, Pg=0, xs=0
    let encoding: u32 = 0xC48043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_rn_31_max_4000_c48043e0() {
    // Encoding: 0xC48043E0
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Rn = 31 (Max)
    // Fields: Zm=0, xs=0, Zt=0, Rn=31, Pg=0
    let encoding: u32 = 0xC48043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zt_0_min_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zt = 0 (Min)
    // Fields: xs=0, Zt=0, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zt_1_poweroftwo_4000_c4804001() {
    // Encoding: 0xC4804001
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: Pg=0, xs=0, Rn=0, Zt=1, Zm=0
    let encoding: u32 = 0xC4804001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zt_30_poweroftwominusone_4000_c480401e() {
    // Encoding: 0xC480401E
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Zt=30, Rn=0, Pg=0, xs=0
    let encoding: u32 = 0xC480401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_field_zt_31_max_4000_c480401f() {
    // Encoding: 0xC480401F
    // Test LD1H_Z.P.BZ_D.x32.unscaled field Zt = 31 (Max)
    // Fields: Pg=0, Rn=0, xs=0, Zm=0, Zt=31
    let encoding: u32 = 0xC480401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_0_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, xs=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_1_4000_c4c04000() {
    // Encoding: 0xC4C04000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, xs=1, Zm=0, Rn=0
    let encoding: u32 = 0xC4C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_2_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, xs=0, Zm=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_3_4000_c4814000() {
    // Encoding: 0xC4814000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Pg=0, Zm=1, xs=0
    let encoding: u32 = 0xC4814000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_4_4000_c49e4000() {
    // Encoding: 0xC49E4000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Rn=0, Zm=30, Zt=0, xs=0
    let encoding: u32 = 0xC49E4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_5_4000_c49f4000() {
    // Encoding: 0xC49F4000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zm=31, xs=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC49F4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_6_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Zm=0, Pg=0, xs=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_7_4000_c4804400() {
    // Encoding: 0xC4804400
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, xs=0, Pg=1, Zm=0
    let encoding: u32 = 0xC4804400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_8_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, xs=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_9_4000_c4804020() {
    // Encoding: 0xC4804020
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Zm=0, Pg=0, Zt=0, xs=0
    let encoding: u32 = 0xC4804020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_10_4000_c48043c0() {
    // Encoding: 0xC48043C0
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rn=30, Zm=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0xC48043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_11_4000_c48043e0() {
    // Encoding: 0xC48043E0
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zm=0, Pg=0, xs=0, Zt=0, Rn=31
    let encoding: u32 = 0xC48043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_12_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_13_4000_c4804001() {
    // Encoding: 0xC4804001
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, xs=0, Zt=1, Rn=0, Zm=0
    let encoding: u32 = 0xC4804001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_14_4000_c480401e() {
    // Encoding: 0xC480401E
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Rn=0, xs=0, Zm=0, Pg=0, Zt=30
    let encoding: u32 = 0xC480401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_15_4000_c480401f() {
    // Encoding: 0xC480401F
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zm=0, Zt=31, Pg=0, xs=0, Rn=0
    let encoding: u32 = 0xC480401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_16_4000_c4804420() {
    // Encoding: 0xC4804420
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: xs=0, Zm=0, Rn=1, Pg=1, Zt=0
    let encoding: u32 = 0xC4804420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_combo_17_4000_c4805fe0() {
    // Encoding: 0xC4805FE0
    // Test LD1H_Z.P.BZ_D.x32.unscaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Zm=0, xs=0, Rn=31, Zt=0
    let encoding: u32 = 0xC4805FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_16384_c48043e0() {
    // Encoding: 0xC48043E0
    // Test LD1H_Z.P.BZ_D.x32.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zm=0, Rn=31, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0xC48043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_invalid_0_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.x32.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_x32_unscaled_invalid_1_4000_c4804000() {
    // Encoding: 0xC4804000
    // Test LD1H_Z.P.BZ_D.x32.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Zm=0, Pg=0, Zt=0, xs=0, Rn=0
    let encoding: u32 = 0xC4804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_xs_0_min_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field xs = 0 (Min)
    // Fields: Pg=0, Zm=0, Rn=0, Zt=0, xs=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field xs 22 +: 1`
/// Requirement: FieldBoundary { field: "xs", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_xs_1_max_4000_84c04000() {
    // Encoding: 0x84C04000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field xs = 1 (Max)
    // Fields: Zm=0, xs=1, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0x84C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zm_0_min_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zm = 0 (Min)
    // Fields: Zm=0, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zm_1_poweroftwo_4000_84814000() {
    // Encoding: 0x84814000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Rn=0, Zm=1, Zt=0, Pg=0, xs=0
    let encoding: u32 = 0x84814000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zm_30_poweroftwominusone_4000_849e4000() {
    // Encoding: 0x849E4000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Zm=30, Zt=0, xs=0, Rn=0
    let encoding: u32 = 0x849E4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zm_31_max_4000_849f4000() {
    // Encoding: 0x849F4000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zm = 31 (Max)
    // Fields: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0x849F4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_pg_0_min_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Pg = 0 (Min)
    // Fields: Pg=0, xs=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_pg_1_poweroftwo_4000_84804400() {
    // Encoding: 0x84804400
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: Zm=0, Pg=1, Zt=0, Rn=0, xs=0
    let encoding: u32 = 0x84804400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_rn_0_min_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Rn = 0 (Min)
    // Fields: Rn=0, Pg=0, Zm=0, xs=0, Zt=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_rn_1_poweroftwo_4000_84804020() {
    // Encoding: 0x84804020
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Rn=1, Zm=0, Zt=0, xs=0
    let encoding: u32 = 0x84804020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_rn_30_poweroftwominusone_4000_848043c0() {
    // Encoding: 0x848043C0
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=0, Zm=0, Rn=30, Pg=0, xs=0
    let encoding: u32 = 0x848043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_rn_31_max_4000_848043e0() {
    // Encoding: 0x848043E0
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Rn = 31 (Max)
    // Fields: Rn=31, Zt=0, Pg=0, xs=0, Zm=0
    let encoding: u32 = 0x848043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zt_0_min_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zt = 0 (Min)
    // Fields: Zm=0, Pg=0, Zt=0, Rn=0, xs=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zt_1_poweroftwo_4000_84804001() {
    // Encoding: 0x84804001
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: xs=0, Rn=0, Zt=1, Zm=0, Pg=0
    let encoding: u32 = 0x84804001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zt_30_poweroftwominusone_4000_8480401e() {
    // Encoding: 0x8480401E
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=30, Pg=0, xs=0, Zm=0, Rn=0
    let encoding: u32 = 0x8480401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_field_zt_31_max_4000_8480401f() {
    // Encoding: 0x8480401F
    // Test LD1H_Z.P.BZ_S.x32.unscaled field Zt = 31 (Max)
    // Fields: xs=0, Zm=0, Rn=0, Pg=0, Zt=31
    let encoding: u32 = 0x8480401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=0 (minimum value)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_0_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// xs=1 (maximum value (1))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_1_4000_84c04000() {
    // Encoding: 0x84C04000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=1, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Rn=0, xs=1, Zm=0
    let encoding: u32 = 0x84C04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_2_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, xs=0, Rn=0, Pg=0, Zt=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_3_4000_84814000() {
    // Encoding: 0x84814000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=1, xs=0, Zt=0, Rn=0
    let encoding: u32 = 0x84814000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_4_4000_849e4000() {
    // Encoding: 0x849E4000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zt=0, Zm=30, Rn=0, xs=0
    let encoding: u32 = 0x849E4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_5_4000_849f4000() {
    // Encoding: 0x849F4000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=31, xs=0, Pg=0, Rn=0
    let encoding: u32 = 0x849F4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_6_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Rn=0, Pg=0, xs=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_7_4000_84804400() {
    // Encoding: 0x84804400
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Pg=1, Zm=0, Zt=0, xs=0, Rn=0
    let encoding: u32 = 0x84804400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_8_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Zt=0, xs=0, Pg=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_9_4000_84804020() {
    // Encoding: 0x84804020
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Zt=0, Pg=0, Zm=0, xs=0
    let encoding: u32 = 0x84804020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_10_4000_848043c0() {
    // Encoding: 0x848043C0
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Pg=0, xs=0, Zm=0, Rn=30, Zt=0
    let encoding: u32 = 0x848043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_11_4000_848043e0() {
    // Encoding: 0x848043E0
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Rn=31, xs=0, Zt=0, Pg=0, Zm=0
    let encoding: u32 = 0x848043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_12_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Pg=0, Zm=0, Rn=0, xs=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_13_4000_84804001() {
    // Encoding: 0x84804001
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Zt=1, xs=0, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0x84804001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_14_4000_8480401e() {
    // Encoding: 0x8480401E
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Pg=0, Rn=0, xs=0, Zm=0, Zt=30
    let encoding: u32 = 0x8480401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_15_4000_8480401f() {
    // Encoding: 0x8480401F
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Rn=0, Zt=31, Zm=0, xs=0, Pg=0
    let encoding: u32 = 0x8480401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 16`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_16_4000_84804420() {
    // Encoding: 0x84804420
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Pg=1, xs=0, Rn=1, Zm=0, Zt=0
    let encoding: u32 = 0x84804420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field combination 17`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_combo_17_4000_84805fe0() {
    // Encoding: 0x84805FE0
    // Test LD1H_Z.P.BZ_S.x32.unscaled field combination: xs=0, Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Pg=31, Rn=31, Zm=0, Zt=0, xs=0
    let encoding: u32 = 0x84805FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_16384_848043e0() {
    // Encoding: 0x848043E0
    // Test LD1H_Z.P.BZ_S.x32.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: xs=0, Zt=0, Zm=0, Rn=31, Pg=0
    let encoding: u32 = 0x848043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_invalid_0_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zt=0, xs=0, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_S.x32.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_s_x32_unscaled_invalid_1_4000_84804000() {
    // Encoding: 0x84804000
    // Test LD1H_Z.P.BZ_S.x32.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Zt=0, Zm=0, xs=0, Pg=0
    let encoding: u32 = 0x84804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zm_0_min_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field Zm = 0 (Min)
    // Fields: Pg=0, Zt=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zm_1_poweroftwo_c000_c4e1c000() {
    // Encoding: 0xC4E1C000
    // Test LD1H_Z.P.BZ_D.64.scaled field Zm = 1 (PowerOfTwo)
    // Fields: Pg=0, Zm=1, Zt=0, Rn=0
    let encoding: u32 = 0xC4E1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zm_30_poweroftwominusone_c000_c4fec000() {
    // Encoding: 0xC4FEC000
    // Test LD1H_Z.P.BZ_D.64.scaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Zt=0, Zm=30, Pg=0
    let encoding: u32 = 0xC4FEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zm_31_max_c000_c4ffc000() {
    // Encoding: 0xC4FFC000
    // Test LD1H_Z.P.BZ_D.64.scaled field Zm = 31 (Max)
    // Fields: Rn=0, Zm=31, Pg=0, Zt=0
    let encoding: u32 = 0xC4FFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_pg_0_min_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field Pg = 0 (Min)
    // Fields: Zm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_pg_1_poweroftwo_c000_c4e0c400() {
    // Encoding: 0xC4E0C400
    // Test LD1H_Z.P.BZ_D.64.scaled field Pg = 1 (PowerOfTwo)
    // Fields: Zm=0, Zt=0, Pg=1, Rn=0
    let encoding: u32 = 0xC4E0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_rn_0_min_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field Rn = 0 (Min)
    // Fields: Rn=0, Pg=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_rn_1_poweroftwo_c000_c4e0c020() {
    // Encoding: 0xC4E0C020
    // Test LD1H_Z.P.BZ_D.64.scaled field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zt=0, Rn=1, Zm=0
    let encoding: u32 = 0xC4E0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_rn_30_poweroftwominusone_c000_c4e0c3c0() {
    // Encoding: 0xC4E0C3C0
    // Test LD1H_Z.P.BZ_D.64.scaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Pg=0, Zt=0, Rn=30
    let encoding: u32 = 0xC4E0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_rn_31_max_c000_c4e0c3e0() {
    // Encoding: 0xC4E0C3E0
    // Test LD1H_Z.P.BZ_D.64.scaled field Rn = 31 (Max)
    // Fields: Zt=0, Zm=0, Pg=0, Rn=31
    let encoding: u32 = 0xC4E0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zt_0_min_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field Zt = 0 (Min)
    // Fields: Zm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zt_1_poweroftwo_c000_c4e0c001() {
    // Encoding: 0xC4E0C001
    // Test LD1H_Z.P.BZ_D.64.scaled field Zt = 1 (PowerOfTwo)
    // Fields: Zt=1, Zm=0, Rn=0, Pg=0
    let encoding: u32 = 0xC4E0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zt_30_poweroftwominusone_c000_c4e0c01e() {
    // Encoding: 0xC4E0C01E
    // Test LD1H_Z.P.BZ_D.64.scaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zt=30, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4E0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_field_zt_31_max_c000_c4e0c01f() {
    // Encoding: 0xC4E0C01F
    // Test LD1H_Z.P.BZ_D.64.scaled field Zt = 31 (Max)
    // Fields: Rn=0, Zm=0, Pg=0, Zt=31
    let encoding: u32 = 0xC4E0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_0_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Pg=0, Zm=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_1_c000_c4e1c000() {
    // Encoding: 0xC4E1C000
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zm=1, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4E1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_2_c000_c4fec000() {
    // Encoding: 0xC4FEC000
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zt=0, Pg=0, Zm=30
    let encoding: u32 = 0xC4FEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_3_c000_c4ffc000() {
    // Encoding: 0xC4FFC000
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zm=31, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4FFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_4_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_5_c000_c4e0c400() {
    // Encoding: 0xC4E0C400
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Zt=0, Pg=1, Rn=0, Zm=0
    let encoding: u32 = 0xC4E0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_6_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Pg=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_7_c000_c4e0c020() {
    // Encoding: 0xC4E0C020
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Pg=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4E0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_8_c000_c4e0c3c0() {
    // Encoding: 0xC4E0C3C0
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Pg=0, Zt=0, Rn=30, Zm=0
    let encoding: u32 = 0xC4E0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_9_c000_c4e0c3e0() {
    // Encoding: 0xC4E0C3E0
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Zm=0, Rn=31, Zt=0, Pg=0
    let encoding: u32 = 0xC4E0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_10_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Zm=0, Rn=0, Pg=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_11_c000_c4e0c001() {
    // Encoding: 0xC4E0C001
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Pg=0, Zt=1, Zm=0, Rn=0
    let encoding: u32 = 0xC4E0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_12_c000_c4e0c01e() {
    // Encoding: 0xC4E0C01E
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Zm=0, Pg=0, Zt=30, Rn=0
    let encoding: u32 = 0xC4E0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_13_c000_c4e0c01f() {
    // Encoding: 0xC4E0C01F
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Pg=0, Zt=31, Zm=0, Rn=0
    let encoding: u32 = 0xC4E0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_14_c000_c4e0c420() {
    // Encoding: 0xC4E0C420
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Zm=0, Zt=0, Pg=1, Rn=1
    let encoding: u32 = 0xC4E0C420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_combo_15_c000_c4e0dfe0() {
    // Encoding: 0xC4E0DFE0
    // Test LD1H_Z.P.BZ_D.64.scaled field combination: Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Zm=0, Pg=31, Zt=0, Rn=31
    let encoding: u32 = 0xC4E0DFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_special_rn_31_stack_pointer_sp_may_require_alignment_49152_c4e0c3e0() {
    // Encoding: 0xC4E0C3E0
    // Test LD1H_Z.P.BZ_D.64.scaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zt=0, Zm=0, Rn=31, Pg=0
    let encoding: u32 = 0xC4E0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_invalid_0_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zm=0, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.scaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_64_scaled_invalid_1_c000_c4e0c000() {
    // Encoding: 0xC4E0C000
    // Test LD1H_Z.P.BZ_D.64.scaled invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4E0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zm_0_min_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zm = 0 (Min)
    // Fields: Pg=0, Zm=0, Rn=0, Zt=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zm_1_poweroftwo_c000_c4c1c000() {
    // Encoding: 0xC4C1C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zm = 1 (PowerOfTwo)
    // Fields: Rn=0, Pg=0, Zm=1, Zt=0
    let encoding: u32 = 0xC4C1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zm_30_poweroftwominusone_c000_c4dec000() {
    // Encoding: 0xC4DEC000
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zm = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=30, Pg=0, Zt=0, Rn=0
    let encoding: u32 = 0xC4DEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zm 16 +: 5`
/// Requirement: FieldBoundary { field: "Zm", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zm_31_max_c000_c4dfc000() {
    // Encoding: 0xC4DFC000
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zm = 31 (Max)
    // Fields: Zt=0, Pg=0, Zm=31, Rn=0
    let encoding: u32 = 0xC4DFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_pg_0_min_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field Pg = 0 (Min)
    // Fields: Pg=0, Rn=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Pg 10 +: 3`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_pg_1_poweroftwo_c000_c4c0c400() {
    // Encoding: 0xC4C0C400
    // Test LD1H_Z.P.BZ_D.64.unscaled field Pg = 1 (PowerOfTwo)
    // Fields: Rn=0, Zm=0, Pg=1, Zt=0
    let encoding: u32 = 0xC4C0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_rn_0_min_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field Rn = 0 (Min)
    // Fields: Zt=0, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_rn_1_poweroftwo_c000_c4c0c020() {
    // Encoding: 0xC4C0C020
    // Test LD1H_Z.P.BZ_D.64.unscaled field Rn = 1 (PowerOfTwo)
    // Fields: Pg=0, Zm=0, Zt=0, Rn=1
    let encoding: u32 = 0xC4C0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_rn_30_poweroftwominusone_c000_c4c0c3c0() {
    // Encoding: 0xC4C0C3C0
    // Test LD1H_Z.P.BZ_D.64.unscaled field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Pg=0, Rn=30, Zt=0
    let encoding: u32 = 0xC4C0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_rn_31_max_c000_c4c0c3e0() {
    // Encoding: 0xC4C0C3E0
    // Test LD1H_Z.P.BZ_D.64.unscaled field Rn = 31 (Max)
    // Fields: Rn=31, Zm=0, Pg=0, Zt=0
    let encoding: u32 = 0xC4C0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 0, boundary: Min }
/// SIMD register V0
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zt_0_min_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zt = 0 (Min)
    // Fields: Rn=0, Pg=0, Zt=0, Zm=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 1, boundary: PowerOfTwo }
/// SIMD register V1
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zt_1_poweroftwo_c000_c4c0c001() {
    // Encoding: 0xC4C0C001
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zt = 1 (PowerOfTwo)
    // Fields: Zt=1, Pg=0, Zm=0, Rn=0
    let encoding: u32 = 0xC4C0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 30, boundary: PowerOfTwoMinusOne }
/// SIMD register V30
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zt_30_poweroftwominusone_c000_c4c0c01e() {
    // Encoding: 0xC4C0C01E
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zt = 30 (PowerOfTwoMinusOne)
    // Fields: Zm=0, Pg=0, Rn=0, Zt=30
    let encoding: u32 = 0xC4C0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Zt 0 +: 5`
/// Requirement: FieldBoundary { field: "Zt", value: 31, boundary: Max }
/// SIMD register V31
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_field_zt_31_max_c000_c4c0c01f() {
    // Encoding: 0xC4C0C01F
    // Test LD1H_Z.P.BZ_D.64.unscaled field Zt = 31 (Max)
    // Fields: Pg=0, Zt=31, Zm=0, Rn=0
    let encoding: u32 = 0xC4C0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_0_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_1_c000_c4c1c000() {
    // Encoding: 0xC4C1C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=1, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Zm=1, Pg=0
    let encoding: u32 = 0xC4C1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_2_c000_c4dec000() {
    // Encoding: 0xC4DEC000
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=30, Pg=0, Rn=0, Zt=0
    // Fields: Rn=0, Pg=0, Zm=30, Zt=0
    let encoding: u32 = 0xC4DEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zm=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_3_c000_c4dfc000() {
    // Encoding: 0xC4DFC000
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=31, Pg=0, Rn=0, Zt=0
    // Fields: Zt=0, Rn=0, Zm=31, Pg=0
    let encoding: u32 = 0xC4DFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_4_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_5_c000_c4c0c400() {
    // Encoding: 0xC4C0C400
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=1, Rn=0, Zt=0
    // Fields: Rn=0, Zm=0, Pg=1, Zt=0
    let encoding: u32 = 0xC4C0C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_6_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Rn=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=1 (register index 1 (second register))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_7_c000_c4c0c020() {
    // Encoding: 0xC4C0C020
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=1, Zt=0
    // Fields: Rn=1, Zm=0, Zt=0, Pg=0
    let encoding: u32 = 0xC4C0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=30 (register index 30 (LR in some contexts))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_8_c000_c4c0c3c0() {
    // Encoding: 0xC4C0C3C0
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=30, Zt=0
    // Fields: Rn=30, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4C0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=31 (register index 31 (SP - stack pointer))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_9_c000_c4c0c3e0() {
    // Encoding: 0xC4C0C3E0
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=31, Zt=0
    // Fields: Rn=31, Pg=0, Zm=0, Zt=0
    let encoding: u32 = 0xC4C0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 10`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=0 (SIMD register V0)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_10_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=0
    // Fields: Zm=0, Zt=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 11`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=1 (SIMD register V1)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_11_c000_c4c0c001() {
    // Encoding: 0xC4C0C001
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=1
    // Fields: Rn=0, Zm=0, Zt=1, Pg=0
    let encoding: u32 = 0xC4C0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 12`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=30 (SIMD register V30)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_12_c000_c4c0c01e() {
    // Encoding: 0xC4C0C01E
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=30
    // Fields: Rn=0, Zt=30, Zm=0, Pg=0
    let encoding: u32 = 0xC4C0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 13`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Zt=31 (SIMD register V31)
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_13_c000_c4c0c01f() {
    // Encoding: 0xC4C0C01F
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=0, Rn=0, Zt=31
    // Fields: Zm=0, Rn=0, Pg=0, Zt=31
    let encoding: u32 = 0xC4C0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 14`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=1 (same register test (reg=1)), Rn=1 (same register test (reg=1))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_14_c000_c4c0c420() {
    // Encoding: 0xC4C0C420
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=1, Rn=1, Zt=0
    // Fields: Zm=0, Zt=0, Rn=1, Pg=1
    let encoding: u32 = 0xC4C0C420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field combination 15`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=31 (same register test (reg=31)), Rn=31 (same register test (reg=31))
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_combo_15_c000_c4c0dfe0() {
    // Encoding: 0xC4C0DFE0
    // Test LD1H_Z.P.BZ_D.64.unscaled field combination: Zm=0, Pg=31, Rn=31, Zt=0
    // Fields: Rn=31, Pg=31, Zm=0, Zt=0
    let encoding: u32 = 0xC4C0DFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_special_rn_31_stack_pointer_sp_may_require_alignment_49152_c4c0c3e0() {
    // Encoding: 0xC4C0C3E0
    // Test LD1H_Z.P.BZ_D.64.unscaled special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Zm=0, Pg=0, Zt=0, Rn=31
    let encoding: u32 = 0xC4C0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_invalid_0_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Zt=0, Pg=0, Rn=0, Zm=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

/// Provenance: LD1H_Z.P.BZ_D.64.unscaled
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ld1h_z_p_bz_d_64_unscaled_invalid_1_c000_c4c0c000() {
    // Encoding: 0xC4C0C000
    // Test LD1H_Z.P.BZ_D.64.unscaled invalid encoding: Unconditional UNDEFINED
    // Fields: Zt=0, Zm=0, Pg=0, Rn=0
    let encoding: u32 = 0xC4C0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(!matches!(exit, Ok(CpuExit::Undefined(_))) && !matches!(exit, Err(ArmError::UndefinedInstruction(_))), "expected allocated encoding for 0x{:08X}: {:?}", encoding, exit);
}

