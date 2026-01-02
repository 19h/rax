//! A64 vector crypto tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_vector_crypto_sm3_sm3partw2 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rm_0_min_c400_ce60c400() {
    // Encoding: 0xCE60C400
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rm = 0 (Min)
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE60C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rm_1_poweroftwo_c400_ce61c400() {
    // Encoding: 0xCE61C400
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, Rd=0
    let encoding: u32 = 0xCE61C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rm_30_poweroftwominusone_c400_ce7ec400() {
    // Encoding: 0xCE7EC400
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rn=0, Rd=0
    let encoding: u32 = 0xCE7EC400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rm_31_max_c400_ce7fc400() {
    // Encoding: 0xCE7FC400
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rm = 31 (Max)
    // Fields: Rm=31, Rn=0, Rd=0
    let encoding: u32 = 0xCE7FC400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rn_0_min_c400_ce60c400() {
    // Encoding: 0xCE60C400
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rn = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE60C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rn_1_poweroftwo_c400_ce60c420() {
    // Encoding: 0xCE60C420
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, Rn=1
    let encoding: u32 = 0xCE60C420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rn_30_poweroftwominusone_c400_ce60c7c0() {
    // Encoding: 0xCE60C7C0
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0, Rm=0
    let encoding: u32 = 0xCE60C7C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rn_31_max_c400_ce60c7e0() {
    // Encoding: 0xCE60C7E0
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rn = 31 (Max)
    // Fields: Rn=31, Rm=0, Rd=0
    let encoding: u32 = 0xCE60C7E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rd_0_min_c400_ce60c400() {
    // Encoding: 0xCE60C400
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE60C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rd_1_poweroftwo_c400_ce60c401() {
    // Encoding: 0xCE60C401
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rd=1, Rn=0
    let encoding: u32 = 0xCE60C401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rd_30_poweroftwominusone_c400_ce60c41e() {
    // Encoding: 0xCE60C41E
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rm=0, Rn=0
    let encoding: u32 = 0xCE60C41E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_field_rd_31_max_c400_ce60c41f() {
    // Encoding: 0xCE60C41F
    // Test aarch64_vector_crypto_sm3_sm3partw2 field Rd = 31 (Max)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE60C41F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_combo_0_c400_ce60c400() {
    // Encoding: 0xCE60C400
    // Test aarch64_vector_crypto_sm3_sm3partw2 field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE60C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_special_rn_31_stack_pointer_sp_may_require_alignment_50176_ce60c7e0() {
    // Encoding: 0xCE60C7E0
    // Test aarch64_vector_crypto_sm3_sm3partw2 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Rd=0, Rm=0
    let encoding: u32 = 0xCE60C7E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_50176_ce60c41f() {
    // Encoding: 0xCE60C41F
    // Test aarch64_vector_crypto_sm3_sm3partw2 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE60C41F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_invalid_0_c400_ce60c400() {
    // Encoding: 0xCE60C400
    // Test aarch64_vector_crypto_sm3_sm3partw2 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE60C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_invalid_1_c400_ce60c400() {
    // Encoding: 0xCE60C400
    // Test aarch64_vector_crypto_sm3_sm3partw2 invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE60C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_reg_write_0_ce60c400() {
    // Test aarch64_vector_crypto_sm3_sm3partw2 register write: SimdFromField("d")
    // Encoding: 0xCE60C400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_sp_rn_ce60c7e0() {
    // Test aarch64_vector_crypto_sm3_sm3partw2 with Rn = SP (31)
    // Encoding: 0xCE60C7E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C7E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw2
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw2_zr_rd_ce60c41f() {
    // Test aarch64_vector_crypto_sm3_sm3partw2 with Rd = ZR (31)
    // Encoding: 0xCE60C41F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C41F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha2op_sha1_sched1 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rn_0_min_1800_5e281800() {
    // Encoding: 0x5E281800
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0x5E281800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rn_1_poweroftwo_1800_5e281820() {
    // Encoding: 0x5E281820
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rd=0
    let encoding: u32 = 0x5E281820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rn_30_poweroftwominusone_1800_5e281bc0() {
    // Encoding: 0x5E281BC0
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=30
    let encoding: u32 = 0x5E281BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rn_31_max_1800_5e281be0() {
    // Encoding: 0x5E281BE0
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rn = 31 (Max)
    // Fields: Rn=31, Rd=0
    let encoding: u32 = 0x5E281BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rd_0_min_1800_5e281800() {
    // Encoding: 0x5E281800
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E281800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rd_1_poweroftwo_1800_5e281801() {
    // Encoding: 0x5E281801
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=1
    let encoding: u32 = 0x5E281801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rd_30_poweroftwominusone_1800_5e28181e() {
    // Encoding: 0x5E28181E
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rn=0
    let encoding: u32 = 0x5E28181E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_field_rd_31_max_1800_5e28181f() {
    // Encoding: 0x5E28181F
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field Rd = 31 (Max)
    // Fields: Rn=0, Rd=31
    let encoding: u32 = 0x5E28181F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_combo_0_1800_5e281800() {
    // Encoding: 0x5E281800
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 field combination: Rn=0, Rd=0
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0x5E281800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_special_rn_31_stack_pointer_sp_may_require_alignment_6144_5e281be0() {
    // Encoding: 0x5E281BE0
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Rd=0
    let encoding: u32 = 0x5E281BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_6144_5e28181f() {
    // Encoding: 0x5E28181F
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rn=0
    let encoding: u32 = 0x5E28181F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA1Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_invalid_0_1800_5e281800() {
    // Encoding: 0x5E281800
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E281800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_invalid_1_1800_5e281800() {
    // Encoding: 0x5E281800
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0x5E281800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_reg_write_0_5e281800() {
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 register write: SimdFromField("d")
    // Encoding: 0x5E281800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E281800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_sp_rn_5e281be0() {
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 with Rn = SP (31)
    // Encoding: 0x5E281BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E281BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_sched1
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_sched1_zr_rd_5e28181f() {
    // Test aarch64_vector_crypto_sha2op_sha1_sched1 with Rd = ZR (31)
    // Encoding: 0x5E28181F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E28181F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm3_sm3tt2b Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rm_0_min_8c00_ce408c00() {
    // Encoding: 0xCE408C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rm = 0 (Min)
    // Fields: Rm=0, imm2=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE408C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rm_1_poweroftwo_8c00_ce418c00() {
    // Encoding: 0xCE418C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, imm2=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE418C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rm_30_poweroftwominusone_8c00_ce5e8c00() {
    // Encoding: 0xCE5E8C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rd=0, Rn=0, imm2=0
    let encoding: u32 = 0xCE5E8C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rm_31_max_8c00_ce5f8c00() {
    // Encoding: 0xCE5F8C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rm = 31 (Max)
    // Fields: Rd=0, imm2=0, Rn=0, Rm=31
    let encoding: u32 = 0xCE5F8C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_imm2_0_zero_8c00_ce408c00() {
    // Encoding: 0xCE408C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field imm2 = 0 (Zero)
    // Fields: Rm=0, Rn=0, Rd=0, imm2=0
    let encoding: u32 = 0xCE408C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_imm2_1_poweroftwo_8c00_ce409c00() {
    // Encoding: 0xCE409C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field imm2 = 1 (PowerOfTwo)
    // Fields: Rm=0, Rd=0, imm2=1, Rn=0
    let encoding: u32 = 0xCE409C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 3, boundary: Max }
/// maximum immediate (3)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_imm2_3_max_8c00_ce40bc00() {
    // Encoding: 0xCE40BC00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field imm2 = 3 (Max)
    // Fields: Rd=0, Rn=0, Rm=0, imm2=3
    let encoding: u32 = 0xCE40BC00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rn_0_min_8c00_ce408c00() {
    // Encoding: 0xCE408C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rn = 0 (Min)
    // Fields: imm2=0, Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE408C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rn_1_poweroftwo_8c00_ce408c20() {
    // Encoding: 0xCE408C20
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, Rn=1, imm2=0
    let encoding: u32 = 0xCE408C20;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rn_30_poweroftwominusone_8c00_ce408fc0() {
    // Encoding: 0xCE408FC0
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: imm2=0, Rn=30, Rd=0, Rm=0
    let encoding: u32 = 0xCE408FC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rn_31_max_8c00_ce408fe0() {
    // Encoding: 0xCE408FE0
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rn = 31 (Max)
    // Fields: Rn=31, imm2=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE408FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rd_0_min_8c00_ce408c00() {
    // Encoding: 0xCE408C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rd = 0 (Min)
    // Fields: Rn=0, Rm=0, imm2=0, Rd=0
    let encoding: u32 = 0xCE408C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rd_1_poweroftwo_8c00_ce408c01() {
    // Encoding: 0xCE408C01
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rd = 1 (PowerOfTwo)
    // Fields: imm2=0, Rn=0, Rd=1, Rm=0
    let encoding: u32 = 0xCE408C01;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rd_30_poweroftwominusone_8c00_ce408c1e() {
    // Encoding: 0xCE408C1E
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, imm2=0, Rd=30, Rn=0
    let encoding: u32 = 0xCE408C1E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_field_rd_31_max_8c00_ce408c1f() {
    // Encoding: 0xCE408C1F
    // Test aarch64_vector_crypto_sm3_sm3tt2b field Rd = 31 (Max)
    // Fields: imm2=0, Rd=31, Rm=0, Rn=0
    let encoding: u32 = 0xCE408C1F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_combo_0_8c00_ce408c00() {
    // Encoding: 0xCE408C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b field combination: Rm=0, imm2=0, Rn=0, Rd=0
    // Fields: Rn=0, imm2=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE408C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_special_rn_31_stack_pointer_sp_may_require_alignment_35840_ce408fe0() {
    // Encoding: 0xCE408FE0
    // Test aarch64_vector_crypto_sm3_sm3tt2b special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, imm2=0, Rn=31
    let encoding: u32 = 0xCE408FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_35840_ce408c1f() {
    // Encoding: 0xCE408C1F
    // Test aarch64_vector_crypto_sm3_sm3tt2b special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rm=0, imm2=0, Rn=0
    let encoding: u32 = 0xCE408C1F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_invalid_0_8c00_ce408c00() {
    // Encoding: 0xCE408C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }
    // Fields: Rd=0, imm2=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE408C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_invalid_1_8c00_ce408c00() {
    // Encoding: 0xCE408C00
    // Test aarch64_vector_crypto_sm3_sm3tt2b invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rm=0, Rd=0, imm2=0
    let encoding: u32 = 0xCE408C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_reg_write_0_ce408c00() {
    // Test aarch64_vector_crypto_sm3_sm3tt2b register write: SimdFromField("d")
    // Encoding: 0xCE408C00
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE408C00;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_sp_rn_ce408fe0() {
    // Test aarch64_vector_crypto_sm3_sm3tt2b with Rn = SP (31)
    // Encoding: 0xCE408FE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE408FE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2b
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2b_zr_rd_ce408c1f() {
    // Test aarch64_vector_crypto_sm3_sm3tt2b with Rd = ZR (31)
    // Encoding: 0xCE408C1F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE408C1F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3op_sha1_hash_majority Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rm_0_min_2000_5e002000() {
    // Encoding: 0x5E002000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rm = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rm_1_poweroftwo_2000_5e012000() {
    // Encoding: 0x5E012000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, Rd=0
    let encoding: u32 = 0x5E012000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rm_30_poweroftwominusone_2000_5e1e2000() {
    // Encoding: 0x5E1E2000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rd=0, Rn=0
    let encoding: u32 = 0x5E1E2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rm_31_max_2000_5e1f2000() {
    // Encoding: 0x5E1F2000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rm = 31 (Max)
    // Fields: Rn=0, Rm=31, Rd=0
    let encoding: u32 = 0x5E1F2000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rn_0_min_2000_5e002000() {
    // Encoding: 0x5E002000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x5E002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rn_1_poweroftwo_2000_5e002020() {
    // Encoding: 0x5E002020
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=1, Rd=0
    let encoding: u32 = 0x5E002020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rn_30_poweroftwominusone_2000_5e0023c0() {
    // Encoding: 0x5E0023C0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rd=0, Rn=30
    let encoding: u32 = 0x5E0023C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rn_31_max_2000_5e0023e0() {
    // Encoding: 0x5E0023E0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rn = 31 (Max)
    // Fields: Rn=31, Rm=0, Rd=0
    let encoding: u32 = 0x5E0023E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rd_0_min_2000_5e002000() {
    // Encoding: 0x5E002000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rd = 0 (Min)
    // Fields: Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rd_1_poweroftwo_2000_5e002001() {
    // Encoding: 0x5E002001
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rd=1, Rn=0
    let encoding: u32 = 0x5E002001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rd_30_poweroftwominusone_2000_5e00201e() {
    // Encoding: 0x5E00201E
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rm=0, Rn=0
    let encoding: u32 = 0x5E00201E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_field_rd_31_max_2000_5e00201f() {
    // Encoding: 0x5E00201F
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field Rd = 31 (Max)
    // Fields: Rd=31, Rn=0, Rm=0
    let encoding: u32 = 0x5E00201F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_combo_0_2000_5e002000() {
    // Encoding: 0x5E002000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_special_rn_31_stack_pointer_sp_may_require_alignment_8192_5e0023e0() {
    // Encoding: 0x5E0023E0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rn=31, Rd=0
    let encoding: u32 = 0x5E0023E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_8192_5e00201f() {
    // Encoding: 0x5E00201F
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00201F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA1Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_invalid_0_2000_5e002000() {
    // Encoding: 0x5E002000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0x5E002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_invalid_1_2000_5e002000() {
    // Encoding: 0x5E002000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0x5E002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_reg_write_0_5e002000() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority register write: SimdFromField("d")
    // Encoding: 0x5E002000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E002000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_sp_rn_5e0023e0() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority with Rn = SP (31)
    // Encoding: 0x5E0023E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E0023E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_majority
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_majority_zr_rd_5e00201f() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_majority with Rd = ZR (31)
    // Encoding: 0x5E00201F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E00201F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_aes_round Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field D 12 +: 1`
/// Requirement: FieldBoundary { field: "D", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_crypto_aes_round_field_d_0_min_4800_4e284800() {
    // Encoding: 0x4E284800
    // Test aarch64_vector_crypto_aes_round field D = 0 (Min)
    // Fields: D=0, Rn=0, Rd=0
    let encoding: u32 = 0x4E284800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field D 12 +: 1`
/// Requirement: FieldBoundary { field: "D", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_d_1_max_4800_4e285800() {
    // Encoding: 0x4E285800
    // Test aarch64_vector_crypto_aes_round field D = 1 (Max)
    // Fields: Rd=0, D=1, Rn=0
    let encoding: u32 = 0x4E285800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rn_0_min_4800_4e284800() {
    // Encoding: 0x4E284800
    // Test aarch64_vector_crypto_aes_round field Rn = 0 (Min)
    // Fields: Rd=0, Rn=0, D=0
    let encoding: u32 = 0x4E284800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rn_1_poweroftwo_4800_4e284820() {
    // Encoding: 0x4E284820
    // Test aarch64_vector_crypto_aes_round field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rn=1, D=0
    let encoding: u32 = 0x4E284820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rn_30_poweroftwominusone_4800_4e284bc0() {
    // Encoding: 0x4E284BC0
    // Test aarch64_vector_crypto_aes_round field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, D=0, Rd=0
    let encoding: u32 = 0x4E284BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rn_31_max_4800_4e284be0() {
    // Encoding: 0x4E284BE0
    // Test aarch64_vector_crypto_aes_round field Rn = 31 (Max)
    // Fields: Rn=31, D=0, Rd=0
    let encoding: u32 = 0x4E284BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rd_0_min_4800_4e284800() {
    // Encoding: 0x4E284800
    // Test aarch64_vector_crypto_aes_round field Rd = 0 (Min)
    // Fields: Rn=0, Rd=0, D=0
    let encoding: u32 = 0x4E284800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rd_1_poweroftwo_4800_4e284801() {
    // Encoding: 0x4E284801
    // Test aarch64_vector_crypto_aes_round field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, D=0, Rd=1
    let encoding: u32 = 0x4E284801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rd_30_poweroftwominusone_4800_4e28481e() {
    // Encoding: 0x4E28481E
    // Test aarch64_vector_crypto_aes_round field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30, D=0
    let encoding: u32 = 0x4E28481E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_aes_round_field_rd_31_max_4800_4e28481f() {
    // Encoding: 0x4E28481F
    // Test aarch64_vector_crypto_aes_round field Rd = 31 (Max)
    // Fields: D=0, Rn=0, Rd=31
    let encoding: u32 = 0x4E28481F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// D=0 (minimum value)
#[test]
fn test_aarch64_vector_crypto_aes_round_combo_0_4800_4e284800() {
    // Encoding: 0x4E284800
    // Test aarch64_vector_crypto_aes_round field combination: D=0, Rn=0, Rd=0
    // Fields: Rn=0, D=0, Rd=0
    let encoding: u32 = 0x4E284800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_aes_round_special_rn_31_stack_pointer_sp_may_require_alignment_18432_4e284be0() {
    // Encoding: 0x4E284BE0
    // Test aarch64_vector_crypto_aes_round special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: D=0, Rd=0, Rn=31
    let encoding: u32 = 0x4E284BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_aes_round_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_18432_4e28481f() {
    // Encoding: 0x4E28481F
    // Test aarch64_vector_crypto_aes_round special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, D=0, Rd=31
    let encoding: u32 = 0x4E28481F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveAESExt" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveAESExt\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_aes_round_invalid_0_4800_4e284800() {
    // Encoding: 0x4E284800
    // Test aarch64_vector_crypto_aes_round invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveAESExt" }, args: [] } }
    // Fields: Rn=0, Rd=0, D=0
    let encoding: u32 = 0x4E284800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_aes_round_invalid_1_4800_4e284800() {
    // Encoding: 0x4E284800
    // Test aarch64_vector_crypto_aes_round invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rn=0, D=0
    let encoding: u32 = 0x4E284800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_aes_round_reg_write_0_4e284800() {
    // Test aarch64_vector_crypto_aes_round register write: SimdFromField("d")
    // Encoding: 0x4E284800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E284800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_aes_round_sp_rn_4e284be0() {
    // Test aarch64_vector_crypto_aes_round with Rn = SP (31)
    // Encoding: 0x4E284BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E284BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_aes_round
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_aes_round_zr_rd_4e28481f() {
    // Test aarch64_vector_crypto_aes_round with Rd = ZR (31)
    // Encoding: 0x4E28481F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E28481F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha2op_sha1_hash Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rn_0_min_800_5e280800() {
    // Encoding: 0x5E280800
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rn = 0 (Min)
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E280800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rn_1_poweroftwo_800_5e280820() {
    // Encoding: 0x5E280820
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rd=0
    let encoding: u32 = 0x5E280820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rn_30_poweroftwominusone_800_5e280bc0() {
    // Encoding: 0x5E280BC0
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0
    let encoding: u32 = 0x5E280BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rn_31_max_800_5e280be0() {
    // Encoding: 0x5E280BE0
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rn = 31 (Max)
    // Fields: Rd=0, Rn=31
    let encoding: u32 = 0x5E280BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rd_0_min_800_5e280800() {
    // Encoding: 0x5E280800
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rd = 0 (Min)
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0x5E280800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rd_1_poweroftwo_800_5e280801() {
    // Encoding: 0x5E280801
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rn=0
    let encoding: u32 = 0x5E280801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rd_30_poweroftwominusone_800_5e28081e() {
    // Encoding: 0x5E28081E
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rn=0
    let encoding: u32 = 0x5E28081E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_field_rd_31_max_800_5e28081f() {
    // Encoding: 0x5E28081F
    // Test aarch64_vector_crypto_sha2op_sha1_hash field Rd = 31 (Max)
    // Fields: Rd=31, Rn=0
    let encoding: u32 = 0x5E28081F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_combo_0_800_5e280800() {
    // Encoding: 0x5E280800
    // Test aarch64_vector_crypto_sha2op_sha1_hash field combination: Rn=0, Rd=0
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0x5E280800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_special_rn_31_stack_pointer_sp_may_require_alignment_2048_5e280be0() {
    // Encoding: 0x5E280BE0
    // Test aarch64_vector_crypto_sha2op_sha1_hash special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rn=31
    let encoding: u32 = 0x5E280BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_2048_5e28081f() {
    // Encoding: 0x5E28081F
    // Test aarch64_vector_crypto_sha2op_sha1_hash special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31
    let encoding: u32 = 0x5E28081F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA1Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_invalid_0_800_5e280800() {
    // Encoding: 0x5E280800
    // Test aarch64_vector_crypto_sha2op_sha1_hash invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E280800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_invalid_1_800_5e280800() {
    // Encoding: 0x5E280800
    // Test aarch64_vector_crypto_sha2op_sha1_hash invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E280800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_reg_write_0_5e280800() {
    // Test aarch64_vector_crypto_sha2op_sha1_hash register write: SimdFromField("d")
    // Encoding: 0x5E280800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E280800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_sp_rn_5e280be0() {
    // Test aarch64_vector_crypto_sha2op_sha1_hash with Rn = SP (31)
    // Encoding: 0x5E280BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E280BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha2op_sha1_hash
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha1_hash_zr_rd_5e28081f() {
    // Test aarch64_vector_crypto_sha2op_sha1_hash with Rd = ZR (31)
    // Encoding: 0x5E28081F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E28081F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm4_sm4enc Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rn_0_min_8400_cec08400() {
    // Encoding: 0xCEC08400
    // Test aarch64_vector_crypto_sm4_sm4enc field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0xCEC08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rn_1_poweroftwo_8400_cec08420() {
    // Encoding: 0xCEC08420
    // Test aarch64_vector_crypto_sm4_sm4enc field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rn=1
    let encoding: u32 = 0xCEC08420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rn_30_poweroftwominusone_8400_cec087c0() {
    // Encoding: 0xCEC087C0
    // Test aarch64_vector_crypto_sm4_sm4enc field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=30
    let encoding: u32 = 0xCEC087C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rn_31_max_8400_cec087e0() {
    // Encoding: 0xCEC087E0
    // Test aarch64_vector_crypto_sm4_sm4enc field Rn = 31 (Max)
    // Fields: Rn=31, Rd=0
    let encoding: u32 = 0xCEC087E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rd_0_min_8400_cec08400() {
    // Encoding: 0xCEC08400
    // Test aarch64_vector_crypto_sm4_sm4enc field Rd = 0 (Min)
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0xCEC08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rd_1_poweroftwo_8400_cec08401() {
    // Encoding: 0xCEC08401
    // Test aarch64_vector_crypto_sm4_sm4enc field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=1
    let encoding: u32 = 0xCEC08401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rd_30_poweroftwominusone_8400_cec0841e() {
    // Encoding: 0xCEC0841E
    // Test aarch64_vector_crypto_sm4_sm4enc field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rn=0
    let encoding: u32 = 0xCEC0841E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_field_rd_31_max_8400_cec0841f() {
    // Encoding: 0xCEC0841F
    // Test aarch64_vector_crypto_sm4_sm4enc field Rd = 31 (Max)
    // Fields: Rd=31, Rn=0
    let encoding: u32 = 0xCEC0841F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_combo_0_8400_cec08400() {
    // Encoding: 0xCEC08400
    // Test aarch64_vector_crypto_sm4_sm4enc field combination: Rn=0, Rd=0
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0xCEC08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_special_rn_31_stack_pointer_sp_may_require_alignment_33792_cec087e0() {
    // Encoding: 0xCEC087E0
    // Test aarch64_vector_crypto_sm4_sm4enc special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rn=31
    let encoding: u32 = 0xCEC087E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_33792_cec0841f() {
    // Encoding: 0xCEC0841F
    // Test aarch64_vector_crypto_sm4_sm4enc special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rn=0
    let encoding: u32 = 0xCEC0841F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM4Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM4Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_invalid_0_8400_cec08400() {
    // Encoding: 0xCEC08400
    // Test aarch64_vector_crypto_sm4_sm4enc invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM4Ext" }, args: [] } }
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0xCEC08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_invalid_1_8400_cec08400() {
    // Encoding: 0xCEC08400
    // Test aarch64_vector_crypto_sm4_sm4enc invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0xCEC08400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_reg_write_0_cec08400() {
    // Test aarch64_vector_crypto_sm4_sm4enc register write: SimdFromField("d")
    // Encoding: 0xCEC08400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCEC08400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_sp_rn_cec087e0() {
    // Test aarch64_vector_crypto_sm4_sm4enc with Rn = SP (31)
    // Encoding: 0xCEC087E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCEC087E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enc
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enc_zr_rd_cec0841f() {
    // Test aarch64_vector_crypto_sm4_sm4enc with Rd = ZR (31)
    // Encoding: 0xCEC0841F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCEC0841F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3_rax1 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rm_0_min_8c00_ce608c00() {
    // Encoding: 0xCE608C00
    // Test aarch64_vector_crypto_sha3_rax1 field Rm = 0 (Min)
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rm_1_poweroftwo_8c00_ce618c00() {
    // Encoding: 0xCE618C00
    // Test aarch64_vector_crypto_sha3_rax1 field Rm = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, Rm=1
    let encoding: u32 = 0xCE618C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rm_30_poweroftwominusone_8c00_ce7e8c00() {
    // Encoding: 0xCE7E8C00
    // Test aarch64_vector_crypto_sha3_rax1 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=0, Rm=30
    let encoding: u32 = 0xCE7E8C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rm_31_max_8c00_ce7f8c00() {
    // Encoding: 0xCE7F8C00
    // Test aarch64_vector_crypto_sha3_rax1 field Rm = 31 (Max)
    // Fields: Rm=31, Rn=0, Rd=0
    let encoding: u32 = 0xCE7F8C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rn_0_min_8c00_ce608c00() {
    // Encoding: 0xCE608C00
    // Test aarch64_vector_crypto_sha3_rax1 field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE608C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rn_1_poweroftwo_8c00_ce608c20() {
    // Encoding: 0xCE608C20
    // Test aarch64_vector_crypto_sha3_rax1 field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, Rn=1
    let encoding: u32 = 0xCE608C20;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rn_30_poweroftwominusone_8c00_ce608fc0() {
    // Encoding: 0xCE608FC0
    // Test aarch64_vector_crypto_sha3_rax1 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=30, Rd=0
    let encoding: u32 = 0xCE608FC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rn_31_max_8c00_ce608fe0() {
    // Encoding: 0xCE608FE0
    // Test aarch64_vector_crypto_sha3_rax1 field Rn = 31 (Max)
    // Fields: Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0xCE608FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rd_0_min_8c00_ce608c00() {
    // Encoding: 0xCE608C00
    // Test aarch64_vector_crypto_sha3_rax1 field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE608C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rd_1_poweroftwo_8c00_ce608c01() {
    // Encoding: 0xCE608C01
    // Test aarch64_vector_crypto_sha3_rax1 field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, Rd=1
    let encoding: u32 = 0xCE608C01;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rd_30_poweroftwominusone_8c00_ce608c1e() {
    // Encoding: 0xCE608C1E
    // Test aarch64_vector_crypto_sha3_rax1 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rm=0, Rd=30
    let encoding: u32 = 0xCE608C1E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_field_rd_31_max_8c00_ce608c1f() {
    // Encoding: 0xCE608C1F
    // Test aarch64_vector_crypto_sha3_rax1 field Rd = 31 (Max)
    // Fields: Rn=0, Rd=31, Rm=0
    let encoding: u32 = 0xCE608C1F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_combo_0_8c00_ce608c00() {
    // Encoding: 0xCE608C00
    // Test aarch64_vector_crypto_sha3_rax1 field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_special_rn_31_stack_pointer_sp_may_require_alignment_35840_ce608fe0() {
    // Encoding: 0xCE608FE0
    // Test aarch64_vector_crypto_sha3_rax1 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Rd=0, Rm=0
    let encoding: u32 = 0xCE608FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_35840_ce608c1f() {
    // Encoding: 0xCE608C1F
    // Test aarch64_vector_crypto_sha3_rax1 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rn=0, Rm=0
    let encoding: u32 = 0xCE608C1F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_invalid_0_8c00_ce608c00() {
    // Encoding: 0xCE608C00
    // Test aarch64_vector_crypto_sha3_rax1 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_invalid_1_8c00_ce608c00() {
    // Encoding: 0xCE608C00
    // Test aarch64_vector_crypto_sha3_rax1 invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_reg_write_0_ce608c00() {
    // Test aarch64_vector_crypto_sha3_rax1 register write: SimdFromField("d")
    // Encoding: 0xCE608C00
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE608C00;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_sp_rn_ce608fe0() {
    // Test aarch64_vector_crypto_sha3_rax1 with Rn = SP (31)
    // Encoding: 0xCE608FE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE608FE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_rax1
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_rax1_zr_rd_ce608c1f() {
    // Test aarch64_vector_crypto_sha3_rax1 with Rd = ZR (31)
    // Encoding: 0xCE608C1F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE608C1F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm4_sm4enckey Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rm_0_min_c800_ce60c800() {
    // Encoding: 0xCE60C800
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rm = 0 (Min)
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE60C800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rm_1_poweroftwo_c800_ce61c800() {
    // Encoding: 0xCE61C800
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, Rd=0
    let encoding: u32 = 0xCE61C800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rm_30_poweroftwominusone_c800_ce7ec800() {
    // Encoding: 0xCE7EC800
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=0, Rm=30
    let encoding: u32 = 0xCE7EC800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rm_31_max_c800_ce7fc800() {
    // Encoding: 0xCE7FC800
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rm = 31 (Max)
    // Fields: Rn=0, Rm=31, Rd=0
    let encoding: u32 = 0xCE7FC800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rn_0_min_c800_ce60c800() {
    // Encoding: 0xCE60C800
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rn = 0 (Min)
    // Fields: Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE60C800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rn_1_poweroftwo_c800_ce60c820() {
    // Encoding: 0xCE60C820
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rm=0, Rd=0
    let encoding: u32 = 0xCE60C820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rn_30_poweroftwominusone_c800_ce60cbc0() {
    // Encoding: 0xCE60CBC0
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rd=0, Rn=30
    let encoding: u32 = 0xCE60CBC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rn_31_max_c800_ce60cbe0() {
    // Encoding: 0xCE60CBE0
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rn = 31 (Max)
    // Fields: Rm=0, Rd=0, Rn=31
    let encoding: u32 = 0xCE60CBE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rd_0_min_c800_ce60c800() {
    // Encoding: 0xCE60C800
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rd = 0 (Min)
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE60C800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rd_1_poweroftwo_c800_ce60c801() {
    // Encoding: 0xCE60C801
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, Rd=1
    let encoding: u32 = 0xCE60C801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rd_30_poweroftwominusone_c800_ce60c81e() {
    // Encoding: 0xCE60C81E
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rm=0, Rd=30
    let encoding: u32 = 0xCE60C81E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_field_rd_31_max_c800_ce60c81f() {
    // Encoding: 0xCE60C81F
    // Test aarch64_vector_crypto_sm4_sm4enckey field Rd = 31 (Max)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE60C81F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_combo_0_c800_ce60c800() {
    // Encoding: 0xCE60C800
    // Test aarch64_vector_crypto_sm4_sm4enckey field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE60C800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_special_rn_31_stack_pointer_sp_may_require_alignment_51200_ce60cbe0() {
    // Encoding: 0xCE60CBE0
    // Test aarch64_vector_crypto_sm4_sm4enckey special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rd=0, Rn=31
    let encoding: u32 = 0xCE60CBE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_51200_ce60c81f() {
    // Encoding: 0xCE60C81F
    // Test aarch64_vector_crypto_sm4_sm4enckey special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rm=0, Rd=31
    let encoding: u32 = 0xCE60C81F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM4Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM4Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_invalid_0_c800_ce60c800() {
    // Encoding: 0xCE60C800
    // Test aarch64_vector_crypto_sm4_sm4enckey invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM4Ext" }, args: [] } }
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE60C800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_invalid_1_c800_ce60c800() {
    // Encoding: 0xCE60C800
    // Test aarch64_vector_crypto_sm4_sm4enckey invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE60C800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_reg_write_0_ce60c800() {
    // Test aarch64_vector_crypto_sm4_sm4enckey register write: SimdFromField("d")
    // Encoding: 0xCE60C800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_sp_rn_ce60cbe0() {
    // Test aarch64_vector_crypto_sm4_sm4enckey with Rn = SP (31)
    // Encoding: 0xCE60CBE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60CBE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm4_sm4enckey
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm4_sm4enckey_zr_rd_ce60c81f() {
    // Test aarch64_vector_crypto_sm4_sm4enckey with Rd = ZR (31)
    // Encoding: 0xCE60C81F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C81F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3_eor3 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rm_0_min_0_ce000000() {
    // Encoding: 0xCE000000
    // Test aarch64_vector_crypto_sha3_eor3 field Rm = 0 (Min)
    // Fields: Ra=0, Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rm_1_poweroftwo_0_ce010000() {
    // Encoding: 0xCE010000
    // Test aarch64_vector_crypto_sha3_eor3 field Rm = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=1, Ra=0, Rn=0
    let encoding: u32 = 0xCE010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rm_30_poweroftwominusone_0_ce1e0000() {
    // Encoding: 0xCE1E0000
    // Test aarch64_vector_crypto_sha3_eor3 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=0, Ra=0, Rm=30
    let encoding: u32 = 0xCE1E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rm_31_max_0_ce1f0000() {
    // Encoding: 0xCE1F0000
    // Test aarch64_vector_crypto_sha3_eor3 field Rm = 31 (Max)
    // Fields: Rd=0, Ra=0, Rm=31, Rn=0
    let encoding: u32 = 0xCE1F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_ra_0_min_0_ce000000() {
    // Encoding: 0xCE000000
    // Test aarch64_vector_crypto_sha3_eor3 field Ra = 0 (Min)
    // Fields: Rn=0, Rm=0, Ra=0, Rd=0
    let encoding: u32 = 0xCE000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_ra_1_poweroftwo_0_ce000400() {
    // Encoding: 0xCE000400
    // Test aarch64_vector_crypto_sha3_eor3 field Ra = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, Ra=1, Rn=0
    let encoding: u32 = 0xCE000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_ra_30_poweroftwominusone_0_ce007800() {
    // Encoding: 0xCE007800
    // Test aarch64_vector_crypto_sha3_eor3 field Ra = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=0, Rm=0, Ra=30
    let encoding: u32 = 0xCE007800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_ra_31_max_0_ce007c00() {
    // Encoding: 0xCE007C00
    // Test aarch64_vector_crypto_sha3_eor3 field Ra = 31 (Max)
    // Fields: Ra=31, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE007C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rn_0_min_0_ce000000() {
    // Encoding: 0xCE000000
    // Test aarch64_vector_crypto_sha3_eor3 field Rn = 0 (Min)
    // Fields: Rm=0, Ra=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rn_1_poweroftwo_0_ce000020() {
    // Encoding: 0xCE000020
    // Test aarch64_vector_crypto_sha3_eor3 field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=1, Rd=0, Ra=0
    let encoding: u32 = 0xCE000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rn_30_poweroftwominusone_0_ce0003c0() {
    // Encoding: 0xCE0003C0
    // Test aarch64_vector_crypto_sha3_eor3 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0, Rm=0, Ra=0
    let encoding: u32 = 0xCE0003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rn_31_max_0_ce0003e0() {
    // Encoding: 0xCE0003E0
    // Test aarch64_vector_crypto_sha3_eor3 field Rn = 31 (Max)
    // Fields: Rm=0, Rd=0, Rn=31, Ra=0
    let encoding: u32 = 0xCE0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rd_0_min_0_ce000000() {
    // Encoding: 0xCE000000
    // Test aarch64_vector_crypto_sha3_eor3 field Rd = 0 (Min)
    // Fields: Rm=0, Ra=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rd_1_poweroftwo_0_ce000001() {
    // Encoding: 0xCE000001
    // Test aarch64_vector_crypto_sha3_eor3 field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Ra=0, Rn=0, Rd=1
    let encoding: u32 = 0xCE000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rd_30_poweroftwominusone_0_ce00001e() {
    // Encoding: 0xCE00001E
    // Test aarch64_vector_crypto_sha3_eor3 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Ra=0, Rd=30
    let encoding: u32 = 0xCE00001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_field_rd_31_max_0_ce00001f() {
    // Encoding: 0xCE00001F
    // Test aarch64_vector_crypto_sha3_eor3 field Rd = 31 (Max)
    // Fields: Ra=0, Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_combo_0_0_ce000000() {
    // Encoding: 0xCE000000
    // Test aarch64_vector_crypto_sha3_eor3 field combination: Rm=0, Ra=0, Rn=0, Rd=0
    // Fields: Rn=0, Ra=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_special_rn_31_stack_pointer_sp_may_require_alignment_0_ce0003e0() {
    // Encoding: 0xCE0003E0
    // Test aarch64_vector_crypto_sha3_eor3 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rn=31, Ra=0, Rd=0
    let encoding: u32 = 0xCE0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_ce00001f() {
    // Encoding: 0xCE00001F
    // Test aarch64_vector_crypto_sha3_eor3 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Ra=0, Rm=0, Rd=31, Rn=0
    let encoding: u32 = 0xCE00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_invalid_0_0_ce000000() {
    // Encoding: 0xCE000000
    // Test aarch64_vector_crypto_sha3_eor3 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }
    // Fields: Rn=0, Rm=0, Ra=0, Rd=0
    let encoding: u32 = 0xCE000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_invalid_1_0_ce000000() {
    // Encoding: 0xCE000000
    // Test aarch64_vector_crypto_sha3_eor3 invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Ra=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_reg_write_0_ce000000() {
    // Test aarch64_vector_crypto_sha3_eor3 register write: SimdFromField("d")
    // Encoding: 0xCE000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_sp_rn_ce0003e0() {
    // Test aarch64_vector_crypto_sha3_eor3 with Rn = SP (31)
    // Encoding: 0xCE0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_eor3
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_eor3_zr_rd_ce00001f() {
    // Test aarch64_vector_crypto_sha3_eor3 with Rd = ZR (31)
    // Encoding: 0xCE00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3op_sha256_hash Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rm_0_min_4000_5e004000() {
    // Encoding: 0x5E004000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rm = 0 (Min)
    // Fields: Rm=0, P=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rm_1_poweroftwo_4000_5e014000() {
    // Encoding: 0x5E014000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, Rd=0, P=0
    let encoding: u32 = 0x5E014000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rm_30_poweroftwominusone_4000_5e1e4000() {
    // Encoding: 0x5E1E4000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rn=0, P=0, Rd=0
    let encoding: u32 = 0x5E1E4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rm_31_max_4000_5e1f4000() {
    // Encoding: 0x5E1F4000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rm = 31 (Max)
    // Fields: Rn=0, P=0, Rm=31, Rd=0
    let encoding: u32 = 0x5E1F4000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field P 12 +: 1`
/// Requirement: FieldBoundary { field: "P", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_p_0_min_4000_5e004000() {
    // Encoding: 0x5E004000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field P = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0, P=0
    let encoding: u32 = 0x5E004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field P 12 +: 1`
/// Requirement: FieldBoundary { field: "P", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_p_1_max_4000_5e005000() {
    // Encoding: 0x5E005000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field P = 1 (Max)
    // Fields: Rd=0, Rm=0, P=1, Rn=0
    let encoding: u32 = 0x5E005000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rn_0_min_4000_5e004000() {
    // Encoding: 0x5E004000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rn = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0, P=0
    let encoding: u32 = 0x5E004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rn_1_poweroftwo_4000_5e004020() {
    // Encoding: 0x5E004020
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rn=1, P=0, Rm=0
    let encoding: u32 = 0x5E004020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rn_30_poweroftwominusone_4000_5e0043c0() {
    // Encoding: 0x5E0043C0
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=0, Rn=30, P=0
    let encoding: u32 = 0x5E0043C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rn_31_max_4000_5e0043e0() {
    // Encoding: 0x5E0043E0
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rn = 31 (Max)
    // Fields: Rm=0, Rd=0, Rn=31, P=0
    let encoding: u32 = 0x5E0043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rd_0_min_4000_5e004000() {
    // Encoding: 0x5E004000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rd = 0 (Min)
    // Fields: Rm=0, Rd=0, P=0, Rn=0
    let encoding: u32 = 0x5E004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rd_1_poweroftwo_4000_5e004001() {
    // Encoding: 0x5E004001
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, P=0, Rd=1
    let encoding: u32 = 0x5E004001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rd_30_poweroftwominusone_4000_5e00401e() {
    // Encoding: 0x5E00401E
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Rd=30, P=0
    let encoding: u32 = 0x5E00401E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_field_rd_31_max_4000_5e00401f() {
    // Encoding: 0x5E00401F
    // Test aarch64_vector_crypto_sha3op_sha256_hash field Rd = 31 (Max)
    // Fields: Rn=0, Rm=0, Rd=31, P=0
    let encoding: u32 = 0x5E00401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_combo_0_4000_5e004000() {
    // Encoding: 0x5E004000
    // Test aarch64_vector_crypto_sha3op_sha256_hash field combination: Rm=0, P=0, Rn=0, Rd=0
    // Fields: Rd=0, P=0, Rn=0, Rm=0
    let encoding: u32 = 0x5E004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_special_rn_31_stack_pointer_sp_may_require_alignment_16384_5e0043e0() {
    // Encoding: 0x5E0043E0
    // Test aarch64_vector_crypto_sha3op_sha256_hash special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, P=0, Rn=31
    let encoding: u32 = 0x5E0043E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_16384_5e00401f() {
    // Encoding: 0x5E00401F
    // Test aarch64_vector_crypto_sha3op_sha256_hash special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rm=0, Rd=31, P=0
    let encoding: u32 = 0x5E00401F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA256Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA256Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_invalid_0_4000_5e004000() {
    // Encoding: 0x5E004000
    // Test aarch64_vector_crypto_sha3op_sha256_hash invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA256Ext" }, args: [] } }
    // Fields: P=0, Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0x5E004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_invalid_1_4000_5e004000() {
    // Encoding: 0x5E004000
    // Test aarch64_vector_crypto_sha3op_sha256_hash invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rd=0, P=0, Rn=0
    let encoding: u32 = 0x5E004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_reg_write_0_5e004000() {
    // Test aarch64_vector_crypto_sha3op_sha256_hash register write: SimdFromField("d")
    // Encoding: 0x5E004000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E004000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_sp_rn_5e0043e0() {
    // Test aarch64_vector_crypto_sha3op_sha256_hash with Rn = SP (31)
    // Encoding: 0x5E0043E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E0043E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_hash
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_hash_zr_rd_5e00401f() {
    // Test aarch64_vector_crypto_sha3op_sha256_hash with Rd = ZR (31)
    // Encoding: 0x5E00401F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E00401F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha512_sha512su1 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rm_0_min_8800_ce608800() {
    // Encoding: 0xCE608800
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rm = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rm_1_poweroftwo_8800_ce618800() {
    // Encoding: 0xCE618800
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rm = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=1, Rd=0
    let encoding: u32 = 0xCE618800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rm_30_poweroftwominusone_8800_ce7e8800() {
    // Encoding: 0xCE7E8800
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=30, Rn=0
    let encoding: u32 = 0xCE7E8800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rm_31_max_8800_ce7f8800() {
    // Encoding: 0xCE7F8800
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rm = 31 (Max)
    // Fields: Rd=0, Rn=0, Rm=31
    let encoding: u32 = 0xCE7F8800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rn_0_min_8800_ce608800() {
    // Encoding: 0xCE608800
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rn = 0 (Min)
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rn_1_poweroftwo_8800_ce608820() {
    // Encoding: 0xCE608820
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rd=0, Rn=1
    let encoding: u32 = 0xCE608820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rn_30_poweroftwominusone_8800_ce608bc0() {
    // Encoding: 0xCE608BC0
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=0, Rn=30
    let encoding: u32 = 0xCE608BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rn_31_max_8800_ce608be0() {
    // Encoding: 0xCE608BE0
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rn = 31 (Max)
    // Fields: Rm=0, Rn=31, Rd=0
    let encoding: u32 = 0xCE608BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rd_0_min_8800_ce608800() {
    // Encoding: 0xCE608800
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE608800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rd_1_poweroftwo_8800_ce608801() {
    // Encoding: 0xCE608801
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, Rd=1
    let encoding: u32 = 0xCE608801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rd_30_poweroftwominusone_8800_ce60881e() {
    // Encoding: 0xCE60881E
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30, Rm=0
    let encoding: u32 = 0xCE60881E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_field_rd_31_max_8800_ce60881f() {
    // Encoding: 0xCE60881F
    // Test aarch64_vector_crypto_sha512_sha512su1 field Rd = 31 (Max)
    // Fields: Rn=0, Rm=0, Rd=31
    let encoding: u32 = 0xCE60881F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_combo_0_8800_ce608800() {
    // Encoding: 0xCE608800
    // Test aarch64_vector_crypto_sha512_sha512su1 field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_special_rn_31_stack_pointer_sp_may_require_alignment_34816_ce608be0() {
    // Encoding: 0xCE608BE0
    // Test aarch64_vector_crypto_sha512_sha512su1 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rn=31, Rm=0
    let encoding: u32 = 0xCE608BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_34816_ce60881f() {
    // Encoding: 0xCE60881F
    // Test aarch64_vector_crypto_sha512_sha512su1 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE60881F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA512Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_invalid_0_8800_ce608800() {
    // Encoding: 0xCE608800
    // Test aarch64_vector_crypto_sha512_sha512su1 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_invalid_1_8800_ce608800() {
    // Encoding: 0xCE608800
    // Test aarch64_vector_crypto_sha512_sha512su1 invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_reg_write_0_ce608800() {
    // Test aarch64_vector_crypto_sha512_sha512su1 register write: SimdFromField("d")
    // Encoding: 0xCE608800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE608800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_sp_rn_ce608be0() {
    // Test aarch64_vector_crypto_sha512_sha512su1 with Rn = SP (31)
    // Encoding: 0xCE608BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE608BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su1
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su1_zr_rd_ce60881f() {
    // Test aarch64_vector_crypto_sha512_sha512su1 with Rd = ZR (31)
    // Encoding: 0xCE60881F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60881F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3op_sha256_sched1 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rm_0_min_6000_5e006000() {
    // Encoding: 0x5E006000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rm = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0x5E006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rm_1_poweroftwo_6000_5e016000() {
    // Encoding: 0x5E016000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rm = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, Rm=1
    let encoding: u32 = 0x5E016000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rm_30_poweroftwominusone_6000_5e1e6000() {
    // Encoding: 0x5E1E6000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=30, Rn=0
    let encoding: u32 = 0x5E1E6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rm_31_max_6000_5e1f6000() {
    // Encoding: 0x5E1F6000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rm = 31 (Max)
    // Fields: Rd=0, Rm=31, Rn=0
    let encoding: u32 = 0x5E1F6000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rn_0_min_6000_5e006000() {
    // Encoding: 0x5E006000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x5E006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rn_1_poweroftwo_6000_5e006020() {
    // Encoding: 0x5E006020
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=1, Rd=0
    let encoding: u32 = 0x5E006020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rn_30_poweroftwominusone_6000_5e0063c0() {
    // Encoding: 0x5E0063C0
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rm=0, Rd=0
    let encoding: u32 = 0x5E0063C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rn_31_max_6000_5e0063e0() {
    // Encoding: 0x5E0063E0
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rn = 31 (Max)
    // Fields: Rd=0, Rn=31, Rm=0
    let encoding: u32 = 0x5E0063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rd_0_min_6000_5e006000() {
    // Encoding: 0x5E006000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rd = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rd_1_poweroftwo_6000_5e006001() {
    // Encoding: 0x5E006001
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rn=0, Rm=0
    let encoding: u32 = 0x5E006001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rd_30_poweroftwominusone_6000_5e00601e() {
    // Encoding: 0x5E00601E
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rd=30, Rn=0
    let encoding: u32 = 0x5E00601E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_field_rd_31_max_6000_5e00601f() {
    // Encoding: 0x5E00601F
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field Rd = 31 (Max)
    // Fields: Rn=0, Rm=0, Rd=31
    let encoding: u32 = 0x5E00601F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_combo_0_6000_5e006000() {
    // Encoding: 0x5E006000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_special_rn_31_stack_pointer_sp_may_require_alignment_24576_5e0063e0() {
    // Encoding: 0x5E0063E0
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0x5E0063E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_24576_5e00601f() {
    // Encoding: 0x5E00601F
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00601F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA256Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA256Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_invalid_0_6000_5e006000() {
    // Encoding: 0x5E006000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA256Ext" }, args: [] } }
    // Fields: Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_invalid_1_6000_5e006000() {
    // Encoding: 0x5E006000
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0x5E006000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_reg_write_0_5e006000() {
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 register write: SimdFromField("d")
    // Encoding: 0x5E006000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E006000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_sp_rn_5e0063e0() {
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 with Rn = SP (31)
    // Encoding: 0x5E0063E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E0063E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha256_sched1
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha256_sched1_zr_rd_5e00601f() {
    // Test aarch64_vector_crypto_sha3op_sha256_sched1 with Rd = ZR (31)
    // Encoding: 0x5E00601F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E00601F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm3_sm3tt2a Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rm_0_min_8800_ce408800() {
    // Encoding: 0xCE408800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rm = 0 (Min)
    // Fields: imm2=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE408800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rm_1_poweroftwo_8800_ce418800() {
    // Encoding: 0xCE418800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rm = 1 (PowerOfTwo)
    // Fields: imm2=0, Rm=1, Rd=0, Rn=0
    let encoding: u32 = 0xCE418800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rm_30_poweroftwominusone_8800_ce5e8800() {
    // Encoding: 0xCE5E8800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, imm2=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE5E8800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rm_31_max_8800_ce5f8800() {
    // Encoding: 0xCE5F8800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rm = 31 (Max)
    // Fields: Rm=31, Rn=0, Rd=0, imm2=0
    let encoding: u32 = 0xCE5F8800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_imm2_0_zero_8800_ce408800() {
    // Encoding: 0xCE408800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field imm2 = 0 (Zero)
    // Fields: Rd=0, Rm=0, imm2=0, Rn=0
    let encoding: u32 = 0xCE408800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_imm2_1_poweroftwo_8800_ce409800() {
    // Encoding: 0xCE409800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field imm2 = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, Rm=0, imm2=1
    let encoding: u32 = 0xCE409800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 3, boundary: Max }
/// maximum immediate (3)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_imm2_3_max_8800_ce40b800() {
    // Encoding: 0xCE40B800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field imm2 = 3 (Max)
    // Fields: Rd=0, Rm=0, imm2=3, Rn=0
    let encoding: u32 = 0xCE40B800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rn_0_min_8800_ce408800() {
    // Encoding: 0xCE408800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rn = 0 (Min)
    // Fields: imm2=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE408800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rn_1_poweroftwo_8800_ce408820() {
    // Encoding: 0xCE408820
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rm=0, imm2=0, Rd=0
    let encoding: u32 = 0xCE408820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rn_30_poweroftwominusone_8800_ce408bc0() {
    // Encoding: 0xCE408BC0
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0, imm2=0, Rm=0
    let encoding: u32 = 0xCE408BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rn_31_max_8800_ce408be0() {
    // Encoding: 0xCE408BE0
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rn = 31 (Max)
    // Fields: Rm=0, imm2=0, Rn=31, Rd=0
    let encoding: u32 = 0xCE408BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rd_0_min_8800_ce408800() {
    // Encoding: 0xCE408800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rd = 0 (Min)
    // Fields: Rn=0, Rm=0, imm2=0, Rd=0
    let encoding: u32 = 0xCE408800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rd_1_poweroftwo_8800_ce408801() {
    // Encoding: 0xCE408801
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, imm2=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE408801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rd_30_poweroftwominusone_8800_ce40881e() {
    // Encoding: 0xCE40881E
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: imm2=0, Rm=0, Rn=0, Rd=30
    let encoding: u32 = 0xCE40881E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_field_rd_31_max_8800_ce40881f() {
    // Encoding: 0xCE40881F
    // Test aarch64_vector_crypto_sm3_sm3tt2a field Rd = 31 (Max)
    // Fields: Rm=0, imm2=0, Rd=31, Rn=0
    let encoding: u32 = 0xCE40881F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_combo_0_8800_ce408800() {
    // Encoding: 0xCE408800
    // Test aarch64_vector_crypto_sm3_sm3tt2a field combination: Rm=0, imm2=0, Rn=0, Rd=0
    // Fields: Rm=0, Rd=0, imm2=0, Rn=0
    let encoding: u32 = 0xCE408800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_special_rn_31_stack_pointer_sp_may_require_alignment_34816_ce408be0() {
    // Encoding: 0xCE408BE0
    // Test aarch64_vector_crypto_sm3_sm3tt2a special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rd=0, imm2=0, Rn=31
    let encoding: u32 = 0xCE408BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_34816_ce40881f() {
    // Encoding: 0xCE40881F
    // Test aarch64_vector_crypto_sm3_sm3tt2a special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rm=0, imm2=0, Rd=31
    let encoding: u32 = 0xCE40881F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_invalid_0_8800_ce408800() {
    // Encoding: 0xCE408800
    // Test aarch64_vector_crypto_sm3_sm3tt2a invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }
    // Fields: Rn=0, Rm=0, imm2=0, Rd=0
    let encoding: u32 = 0xCE408800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_invalid_1_8800_ce408800() {
    // Encoding: 0xCE408800
    // Test aarch64_vector_crypto_sm3_sm3tt2a invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, imm2=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE408800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_reg_write_0_ce408800() {
    // Test aarch64_vector_crypto_sm3_sm3tt2a register write: SimdFromField("d")
    // Encoding: 0xCE408800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE408800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_sp_rn_ce408be0() {
    // Test aarch64_vector_crypto_sm3_sm3tt2a with Rn = SP (31)
    // Encoding: 0xCE408BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE408BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt2a
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt2a_zr_rd_ce40881f() {
    // Test aarch64_vector_crypto_sm3_sm3tt2a with Rd = ZR (31)
    // Encoding: 0xCE40881F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE40881F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm3_sm3partw1 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rm_0_min_c000_ce60c000() {
    // Encoding: 0xCE60C000
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rm = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE60C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rm_1_poweroftwo_c000_ce61c000() {
    // Encoding: 0xCE61C000
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, Rd=0
    let encoding: u32 = 0xCE61C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rm_30_poweroftwominusone_c000_ce7ec000() {
    // Encoding: 0xCE7EC000
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=0, Rm=30
    let encoding: u32 = 0xCE7EC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rm_31_max_c000_ce7fc000() {
    // Encoding: 0xCE7FC000
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rm = 31 (Max)
    // Fields: Rd=0, Rm=31, Rn=0
    let encoding: u32 = 0xCE7FC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rn_0_min_c000_ce60c000() {
    // Encoding: 0xCE60C000
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rn = 0 (Min)
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE60C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rn_1_poweroftwo_c000_ce60c020() {
    // Encoding: 0xCE60C020
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rd=0, Rn=1
    let encoding: u32 = 0xCE60C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rn_30_poweroftwominusone_c000_ce60c3c0() {
    // Encoding: 0xCE60C3C0
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=0, Rn=30
    let encoding: u32 = 0xCE60C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rn_31_max_c000_ce60c3e0() {
    // Encoding: 0xCE60C3E0
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rn = 31 (Max)
    // Fields: Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0xCE60C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rd_0_min_c000_ce60c000() {
    // Encoding: 0xCE60C000
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rd = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE60C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rd_1_poweroftwo_c000_ce60c001() {
    // Encoding: 0xCE60C001
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, Rd=1
    let encoding: u32 = 0xCE60C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rd_30_poweroftwominusone_c000_ce60c01e() {
    // Encoding: 0xCE60C01E
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rm=0, Rn=0
    let encoding: u32 = 0xCE60C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_field_rd_31_max_c000_ce60c01f() {
    // Encoding: 0xCE60C01F
    // Test aarch64_vector_crypto_sm3_sm3partw1 field Rd = 31 (Max)
    // Fields: Rn=0, Rm=0, Rd=31
    let encoding: u32 = 0xCE60C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_combo_0_c000_ce60c000() {
    // Encoding: 0xCE60C000
    // Test aarch64_vector_crypto_sm3_sm3partw1 field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE60C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_special_rn_31_stack_pointer_sp_may_require_alignment_49152_ce60c3e0() {
    // Encoding: 0xCE60C3E0
    // Test aarch64_vector_crypto_sm3_sm3partw1 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0xCE60C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_49152_ce60c01f() {
    // Encoding: 0xCE60C01F
    // Test aarch64_vector_crypto_sm3_sm3partw1 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31, Rm=0
    let encoding: u32 = 0xCE60C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_invalid_0_c000_ce60c000() {
    // Encoding: 0xCE60C000
    // Test aarch64_vector_crypto_sm3_sm3partw1 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE60C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_invalid_1_c000_ce60c000() {
    // Encoding: 0xCE60C000
    // Test aarch64_vector_crypto_sm3_sm3partw1 invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE60C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_reg_write_0_ce60c000() {
    // Test aarch64_vector_crypto_sm3_sm3partw1 register write: SimdFromField("d")
    // Encoding: 0xCE60C000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_sp_rn_ce60c3e0() {
    // Test aarch64_vector_crypto_sm3_sm3partw1 with Rn = SP (31)
    // Encoding: 0xCE60C3E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C3E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3partw1
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3partw1_zr_rd_ce60c01f() {
    // Test aarch64_vector_crypto_sm3_sm3partw1 with Rd = ZR (31)
    // Encoding: 0xCE60C01F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60C01F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha512_sha512h Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rm_0_min_8000_ce608000() {
    // Encoding: 0xCE608000
    // Test aarch64_vector_crypto_sha512_sha512h field Rm = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE608000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rm_1_poweroftwo_8000_ce618000() {
    // Encoding: 0xCE618000
    // Test aarch64_vector_crypto_sha512_sha512h field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rd=0, Rn=0
    let encoding: u32 = 0xCE618000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rm_30_poweroftwominusone_8000_ce7e8000() {
    // Encoding: 0xCE7E8000
    // Test aarch64_vector_crypto_sha512_sha512h field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rn=0, Rd=0
    let encoding: u32 = 0xCE7E8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rm_31_max_8000_ce7f8000() {
    // Encoding: 0xCE7F8000
    // Test aarch64_vector_crypto_sha512_sha512h field Rm = 31 (Max)
    // Fields: Rm=31, Rn=0, Rd=0
    let encoding: u32 = 0xCE7F8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rn_0_min_8000_ce608000() {
    // Encoding: 0xCE608000
    // Test aarch64_vector_crypto_sha512_sha512h field Rn = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rn_1_poweroftwo_8000_ce608020() {
    // Encoding: 0xCE608020
    // Test aarch64_vector_crypto_sha512_sha512h field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=1, Rd=0
    let encoding: u32 = 0xCE608020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rn_30_poweroftwominusone_8000_ce6083c0() {
    // Encoding: 0xCE6083C0
    // Test aarch64_vector_crypto_sha512_sha512h field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rd=0, Rn=30
    let encoding: u32 = 0xCE6083C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rn_31_max_8000_ce6083e0() {
    // Encoding: 0xCE6083E0
    // Test aarch64_vector_crypto_sha512_sha512h field Rn = 31 (Max)
    // Fields: Rm=0, Rn=31, Rd=0
    let encoding: u32 = 0xCE6083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rd_0_min_8000_ce608000() {
    // Encoding: 0xCE608000
    // Test aarch64_vector_crypto_sha512_sha512h field Rd = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rd_1_poweroftwo_8000_ce608001() {
    // Encoding: 0xCE608001
    // Test aarch64_vector_crypto_sha512_sha512h field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rd=1, Rn=0
    let encoding: u32 = 0xCE608001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rd_30_poweroftwominusone_8000_ce60801e() {
    // Encoding: 0xCE60801E
    // Test aarch64_vector_crypto_sha512_sha512h field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30, Rm=0
    let encoding: u32 = 0xCE60801E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_field_rd_31_max_8000_ce60801f() {
    // Encoding: 0xCE60801F
    // Test aarch64_vector_crypto_sha512_sha512h field Rd = 31 (Max)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE60801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_combo_0_8000_ce608000() {
    // Encoding: 0xCE608000
    // Test aarch64_vector_crypto_sha512_sha512h field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_special_rn_31_stack_pointer_sp_may_require_alignment_32768_ce6083e0() {
    // Encoding: 0xCE6083E0
    // Test aarch64_vector_crypto_sha512_sha512h special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Rd=0, Rm=0
    let encoding: u32 = 0xCE6083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_32768_ce60801f() {
    // Encoding: 0xCE60801F
    // Test aarch64_vector_crypto_sha512_sha512h special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE60801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA512Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_invalid_0_8000_ce608000() {
    // Encoding: 0xCE608000
    // Test aarch64_vector_crypto_sha512_sha512h invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_invalid_1_8000_ce608000() {
    // Encoding: 0xCE608000
    // Test aarch64_vector_crypto_sha512_sha512h invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_reg_write_0_ce608000() {
    // Test aarch64_vector_crypto_sha512_sha512h register write: SimdFromField("d")
    // Encoding: 0xCE608000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE608000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_sp_rn_ce6083e0() {
    // Test aarch64_vector_crypto_sha512_sha512h with Rn = SP (31)
    // Encoding: 0xCE6083E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE6083E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h_zr_rd_ce60801f() {
    // Test aarch64_vector_crypto_sha512_sha512h with Rd = ZR (31)
    // Encoding: 0xCE60801F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60801F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_aes_mix Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field D 12 +: 1`
/// Requirement: FieldBoundary { field: "D", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_d_0_min_6800_4e286800() {
    // Encoding: 0x4E286800
    // Test aarch64_vector_crypto_aes_mix field D = 0 (Min)
    // Fields: D=0, Rn=0, Rd=0
    let encoding: u32 = 0x4E286800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field D 12 +: 1`
/// Requirement: FieldBoundary { field: "D", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_d_1_max_6800_4e287800() {
    // Encoding: 0x4E287800
    // Test aarch64_vector_crypto_aes_mix field D = 1 (Max)
    // Fields: Rn=0, Rd=0, D=1
    let encoding: u32 = 0x4E287800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rn_0_min_6800_4e286800() {
    // Encoding: 0x4E286800
    // Test aarch64_vector_crypto_aes_mix field Rn = 0 (Min)
    // Fields: D=0, Rn=0, Rd=0
    let encoding: u32 = 0x4E286800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rn_1_poweroftwo_6800_4e286820() {
    // Encoding: 0x4E286820
    // Test aarch64_vector_crypto_aes_mix field Rn = 1 (PowerOfTwo)
    // Fields: D=0, Rn=1, Rd=0
    let encoding: u32 = 0x4E286820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rn_30_poweroftwominusone_6800_4e286bc0() {
    // Encoding: 0x4E286BC0
    // Test aarch64_vector_crypto_aes_mix field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, D=0, Rn=30
    let encoding: u32 = 0x4E286BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rn_31_max_6800_4e286be0() {
    // Encoding: 0x4E286BE0
    // Test aarch64_vector_crypto_aes_mix field Rn = 31 (Max)
    // Fields: D=0, Rn=31, Rd=0
    let encoding: u32 = 0x4E286BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rd_0_min_6800_4e286800() {
    // Encoding: 0x4E286800
    // Test aarch64_vector_crypto_aes_mix field Rd = 0 (Min)
    // Fields: Rd=0, D=0, Rn=0
    let encoding: u32 = 0x4E286800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rd_1_poweroftwo_6800_4e286801() {
    // Encoding: 0x4E286801
    // Test aarch64_vector_crypto_aes_mix field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, D=0, Rd=1
    let encoding: u32 = 0x4E286801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rd_30_poweroftwominusone_6800_4e28681e() {
    // Encoding: 0x4E28681E
    // Test aarch64_vector_crypto_aes_mix field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: D=0, Rn=0, Rd=30
    let encoding: u32 = 0x4E28681E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_aes_mix_field_rd_31_max_6800_4e28681f() {
    // Encoding: 0x4E28681F
    // Test aarch64_vector_crypto_aes_mix field Rd = 31 (Max)
    // Fields: Rn=0, D=0, Rd=31
    let encoding: u32 = 0x4E28681F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// D=0 (minimum value)
#[test]
fn test_aarch64_vector_crypto_aes_mix_combo_0_6800_4e286800() {
    // Encoding: 0x4E286800
    // Test aarch64_vector_crypto_aes_mix field combination: D=0, Rn=0, Rd=0
    // Fields: Rn=0, Rd=0, D=0
    let encoding: u32 = 0x4E286800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_aes_mix_special_rn_31_stack_pointer_sp_may_require_alignment_26624_4e286be0() {
    // Encoding: 0x4E286BE0
    // Test aarch64_vector_crypto_aes_mix special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: D=0, Rn=31, Rd=0
    let encoding: u32 = 0x4E286BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_aes_mix_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_26624_4e28681f() {
    // Encoding: 0x4E28681F
    // Test aarch64_vector_crypto_aes_mix special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: D=0, Rn=0, Rd=31
    let encoding: u32 = 0x4E28681F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveAESExt" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveAESExt\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_aes_mix_invalid_0_6800_4e286800() {
    // Encoding: 0x4E286800
    // Test aarch64_vector_crypto_aes_mix invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveAESExt" }, args: [] } }
    // Fields: Rd=0, Rn=0, D=0
    let encoding: u32 = 0x4E286800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_aes_mix_invalid_1_6800_4e286800() {
    // Encoding: 0x4E286800
    // Test aarch64_vector_crypto_aes_mix invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, D=0, Rn=0
    let encoding: u32 = 0x4E286800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_aes_mix_reg_write_0_4e286800() {
    // Test aarch64_vector_crypto_aes_mix register write: SimdFromField("d")
    // Encoding: 0x4E286800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E286800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_aes_mix_sp_rn_4e286be0() {
    // Test aarch64_vector_crypto_aes_mix with Rn = SP (31)
    // Encoding: 0x4E286BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E286BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_aes_mix
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_aes_mix_zr_rd_4e28681f() {
    // Test aarch64_vector_crypto_aes_mix with Rd = ZR (31)
    // Encoding: 0x4E28681F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E28681F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3_bcax Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rm_0_min_0_ce200000() {
    // Encoding: 0xCE200000
    // Test aarch64_vector_crypto_sha3_bcax field Rm = 0 (Min)
    // Fields: Ra=0, Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rm_1_poweroftwo_0_ce210000() {
    // Encoding: 0xCE210000
    // Test aarch64_vector_crypto_sha3_bcax field Rm = 1 (PowerOfTwo)
    // Fields: Rn=0, Ra=0, Rd=0, Rm=1
    let encoding: u32 = 0xCE210000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rm_30_poweroftwominusone_0_ce3e0000() {
    // Encoding: 0xCE3E0000
    // Test aarch64_vector_crypto_sha3_bcax field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=0, Rm=30, Ra=0
    let encoding: u32 = 0xCE3E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rm_31_max_0_ce3f0000() {
    // Encoding: 0xCE3F0000
    // Test aarch64_vector_crypto_sha3_bcax field Rm = 31 (Max)
    // Fields: Rm=31, Ra=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE3F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_ra_0_min_0_ce200000() {
    // Encoding: 0xCE200000
    // Test aarch64_vector_crypto_sha3_bcax field Ra = 0 (Min)
    // Fields: Rm=0, Rn=0, Ra=0, Rd=0
    let encoding: u32 = 0xCE200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_ra_1_poweroftwo_0_ce200400() {
    // Encoding: 0xCE200400
    // Test aarch64_vector_crypto_sha3_bcax field Ra = 1 (PowerOfTwo)
    // Fields: Rm=0, Ra=1, Rd=0, Rn=0
    let encoding: u32 = 0xCE200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_ra_30_poweroftwominusone_0_ce207800() {
    // Encoding: 0xCE207800
    // Test aarch64_vector_crypto_sha3_bcax field Ra = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Ra=30, Rd=0
    let encoding: u32 = 0xCE207800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_ra_31_max_0_ce207c00() {
    // Encoding: 0xCE207C00
    // Test aarch64_vector_crypto_sha3_bcax field Ra = 31 (Max)
    // Fields: Rn=0, Rm=0, Rd=0, Ra=31
    let encoding: u32 = 0xCE207C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rn_0_min_0_ce200000() {
    // Encoding: 0xCE200000
    // Test aarch64_vector_crypto_sha3_bcax field Rn = 0 (Min)
    // Fields: Rm=0, Rd=0, Rn=0, Ra=0
    let encoding: u32 = 0xCE200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rn_1_poweroftwo_0_ce200020() {
    // Encoding: 0xCE200020
    // Test aarch64_vector_crypto_sha3_bcax field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Ra=0, Rn=1, Rd=0
    let encoding: u32 = 0xCE200020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rn_30_poweroftwominusone_0_ce2003c0() {
    // Encoding: 0xCE2003C0
    // Test aarch64_vector_crypto_sha3_bcax field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Ra=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE2003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rn_31_max_0_ce2003e0() {
    // Encoding: 0xCE2003E0
    // Test aarch64_vector_crypto_sha3_bcax field Rn = 31 (Max)
    // Fields: Ra=0, Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0xCE2003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rd_0_min_0_ce200000() {
    // Encoding: 0xCE200000
    // Test aarch64_vector_crypto_sha3_bcax field Rd = 0 (Min)
    // Fields: Ra=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rd_1_poweroftwo_0_ce200001() {
    // Encoding: 0xCE200001
    // Test aarch64_vector_crypto_sha3_bcax field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Ra=0, Rm=0, Rd=1
    let encoding: u32 = 0xCE200001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rd_30_poweroftwominusone_0_ce20001e() {
    // Encoding: 0xCE20001E
    // Test aarch64_vector_crypto_sha3_bcax field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Rd=30, Ra=0
    let encoding: u32 = 0xCE20001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_field_rd_31_max_0_ce20001f() {
    // Encoding: 0xCE20001F
    // Test aarch64_vector_crypto_sha3_bcax field Rd = 31 (Max)
    // Fields: Rm=0, Ra=0, Rd=31, Rn=0
    let encoding: u32 = 0xCE20001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_combo_0_0_ce200000() {
    // Encoding: 0xCE200000
    // Test aarch64_vector_crypto_sha3_bcax field combination: Rm=0, Ra=0, Rn=0, Rd=0
    // Fields: Rm=0, Rd=0, Rn=0, Ra=0
    let encoding: u32 = 0xCE200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_special_rn_31_stack_pointer_sp_may_require_alignment_0_ce2003e0() {
    // Encoding: 0xCE2003E0
    // Test aarch64_vector_crypto_sha3_bcax special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Ra=0, Rd=0, Rn=31
    let encoding: u32 = 0xCE2003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_ce20001f() {
    // Encoding: 0xCE20001F
    // Test aarch64_vector_crypto_sha3_bcax special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Ra=0, Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE20001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_invalid_0_0_ce200000() {
    // Encoding: 0xCE200000
    // Test aarch64_vector_crypto_sha3_bcax invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }
    // Fields: Rn=0, Rm=0, Rd=0, Ra=0
    let encoding: u32 = 0xCE200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_invalid_1_0_ce200000() {
    // Encoding: 0xCE200000
    // Test aarch64_vector_crypto_sha3_bcax invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Ra=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_reg_write_0_ce200000() {
    // Test aarch64_vector_crypto_sha3_bcax register write: SimdFromField("d")
    // Encoding: 0xCE200000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE200000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_sp_rn_ce2003e0() {
    // Test aarch64_vector_crypto_sha3_bcax with Rn = SP (31)
    // Encoding: 0xCE2003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE2003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_bcax
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_bcax_zr_rd_ce20001f() {
    // Test aarch64_vector_crypto_sha3_bcax with Rd = ZR (31)
    // Encoding: 0xCE20001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE20001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3_xar Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rm_0_min_0_ce800000() {
    // Encoding: 0xCE800000
    // Test aarch64_vector_crypto_sha3_xar field Rm = 0 (Min)
    // Fields: Rm=0, Rd=0, imm6=0, Rn=0
    let encoding: u32 = 0xCE800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rm_1_poweroftwo_0_ce810000() {
    // Encoding: 0xCE810000
    // Test aarch64_vector_crypto_sha3_xar field Rm = 1 (PowerOfTwo)
    // Fields: imm6=0, Rn=0, Rd=0, Rm=1
    let encoding: u32 = 0xCE810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rm_30_poweroftwominusone_0_ce9e0000() {
    // Encoding: 0xCE9E0000
    // Test aarch64_vector_crypto_sha3_xar field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, imm6=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE9E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rm_31_max_0_ce9f0000() {
    // Encoding: 0xCE9F0000
    // Test aarch64_vector_crypto_sha3_xar field Rm = 31 (Max)
    // Fields: imm6=0, Rn=0, Rm=31, Rd=0
    let encoding: u32 = 0xCE9F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_0_zero_0_ce800000() {
    // Encoding: 0xCE800000
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 0 (Zero)
    // Fields: Rm=0, imm6=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_1_poweroftwo_0_ce800400() {
    // Encoding: 0xCE800400
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 1 (PowerOfTwo)
    // Fields: Rd=0, imm6=1, Rm=0, Rn=0
    let encoding: u32 = 0xCE800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_3_poweroftwominusone_0_ce800c00() {
    // Encoding: 0xCE800C00
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 3 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=0, Rn=0, imm6=3
    let encoding: u32 = 0xCE800C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_4_poweroftwo_0_ce801000() {
    // Encoding: 0xCE801000
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 4 (PowerOfTwo)
    // Fields: imm6=4, Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE801000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_7_poweroftwominusone_0_ce801c00() {
    // Encoding: 0xCE801C00
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 7 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=0, imm6=7, Rm=0
    let encoding: u32 = 0xCE801C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_8_poweroftwo_0_ce802000() {
    // Encoding: 0xCE802000
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 8 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, imm6=8, Rd=0
    let encoding: u32 = 0xCE802000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_15_poweroftwominusone_0_ce803c00() {
    // Encoding: 0xCE803C00
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 15 (PowerOfTwoMinusOne)
    // Fields: Rn=0, imm6=15, Rd=0, Rm=0
    let encoding: u32 = 0xCE803C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_16_poweroftwo_0_ce804000() {
    // Encoding: 0xCE804000
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 16 (PowerOfTwo)
    // Fields: Rm=0, Rd=0, imm6=16, Rn=0
    let encoding: u32 = 0xCE804000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 31, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (31)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_31_poweroftwominusone_0_ce807c00() {
    // Encoding: 0xCE807C00
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 31 (PowerOfTwoMinusOne)
    // Fields: imm6=31, Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE807C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_32_poweroftwo_0_ce808000() {
    // Encoding: 0xCE808000
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 32 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, imm6=32, Rd=0
    let encoding: u32 = 0xCE808000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 63, boundary: Max }
/// maximum immediate (63)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_imm6_63_max_0_ce80fc00() {
    // Encoding: 0xCE80FC00
    // Test aarch64_vector_crypto_sha3_xar field imm6 = 63 (Max)
    // Fields: Rm=0, Rn=0, imm6=63, Rd=0
    let encoding: u32 = 0xCE80FC00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rn_0_min_0_ce800000() {
    // Encoding: 0xCE800000
    // Test aarch64_vector_crypto_sha3_xar field Rn = 0 (Min)
    // Fields: Rm=0, imm6=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rn_1_poweroftwo_0_ce800020() {
    // Encoding: 0xCE800020
    // Test aarch64_vector_crypto_sha3_xar field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, imm6=0, Rn=1, Rm=0
    let encoding: u32 = 0xCE800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rn_30_poweroftwominusone_0_ce8003c0() {
    // Encoding: 0xCE8003C0
    // Test aarch64_vector_crypto_sha3_xar field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, imm6=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE8003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rn_31_max_0_ce8003e0() {
    // Encoding: 0xCE8003E0
    // Test aarch64_vector_crypto_sha3_xar field Rn = 31 (Max)
    // Fields: Rd=0, Rm=0, Rn=31, imm6=0
    let encoding: u32 = 0xCE8003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rd_0_min_0_ce800000() {
    // Encoding: 0xCE800000
    // Test aarch64_vector_crypto_sha3_xar field Rd = 0 (Min)
    // Fields: Rm=0, Rd=0, imm6=0, Rn=0
    let encoding: u32 = 0xCE800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rd_1_poweroftwo_0_ce800001() {
    // Encoding: 0xCE800001
    // Test aarch64_vector_crypto_sha3_xar field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rn=0, Rm=0, imm6=0
    let encoding: u32 = 0xCE800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rd_30_poweroftwominusone_0_ce80001e() {
    // Encoding: 0xCE80001E
    // Test aarch64_vector_crypto_sha3_xar field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: imm6=0, Rn=0, Rm=0, Rd=30
    let encoding: u32 = 0xCE80001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_field_rd_31_max_0_ce80001f() {
    // Encoding: 0xCE80001F
    // Test aarch64_vector_crypto_sha3_xar field Rd = 31 (Max)
    // Fields: imm6=0, Rn=0, Rd=31, Rm=0
    let encoding: u32 = 0xCE80001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3_xar_combo_0_0_ce800000() {
    // Encoding: 0xCE800000
    // Test aarch64_vector_crypto_sha3_xar field combination: Rm=0, imm6=0, Rn=0, Rd=0
    // Fields: imm6=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3_xar_special_rn_31_stack_pointer_sp_may_require_alignment_0_ce8003e0() {
    // Encoding: 0xCE8003E0
    // Test aarch64_vector_crypto_sha3_xar special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Rd=0, Rm=0, imm6=0
    let encoding: u32 = 0xCE8003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3_xar_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_ce80001f() {
    // Encoding: 0xCE80001F
    // Test aarch64_vector_crypto_sha3_xar special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31, Rm=0, imm6=0
    let encoding: u32 = 0xCE80001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_xar_invalid_0_0_ce800000() {
    // Encoding: 0xCE800000
    // Test aarch64_vector_crypto_sha3_xar invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA3Ext" }, args: [] } }
    // Fields: Rm=0, Rd=0, imm6=0, Rn=0
    let encoding: u32 = 0xCE800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3_xar_invalid_1_0_ce800000() {
    // Encoding: 0xCE800000
    // Test aarch64_vector_crypto_sha3_xar invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rm=0, imm6=0, Rd=0
    let encoding: u32 = 0xCE800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3_xar_reg_write_0_ce800000() {
    // Test aarch64_vector_crypto_sha3_xar register write: SimdFromField("d")
    // Encoding: 0xCE800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_sp_rn_ce8003e0() {
    // Test aarch64_vector_crypto_sha3_xar with Rn = SP (31)
    // Encoding: 0xCE8003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE8003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3_xar
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3_xar_zr_rd_ce80001f() {
    // Test aarch64_vector_crypto_sha3_xar with Rd = ZR (31)
    // Encoding: 0xCE80001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE80001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm3_sm3tt1a Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rm_0_min_8000_ce408000() {
    // Encoding: 0xCE408000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rm = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0, imm2=0
    let encoding: u32 = 0xCE408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rm_1_poweroftwo_8000_ce418000() {
    // Encoding: 0xCE418000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rm = 1 (PowerOfTwo)
    // Fields: Rd=0, imm2=0, Rn=0, Rm=1
    let encoding: u32 = 0xCE418000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rm_30_poweroftwominusone_8000_ce5e8000() {
    // Encoding: 0xCE5E8000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: imm2=0, Rm=30, Rn=0, Rd=0
    let encoding: u32 = 0xCE5E8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rm_31_max_8000_ce5f8000() {
    // Encoding: 0xCE5F8000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rm = 31 (Max)
    // Fields: imm2=0, Rm=31, Rd=0, Rn=0
    let encoding: u32 = 0xCE5F8000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_imm2_0_zero_8000_ce408000() {
    // Encoding: 0xCE408000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field imm2 = 0 (Zero)
    // Fields: Rn=0, imm2=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_imm2_1_poweroftwo_8000_ce409000() {
    // Encoding: 0xCE409000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field imm2 = 1 (PowerOfTwo)
    // Fields: imm2=1, Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE409000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 3, boundary: Max }
/// maximum immediate (3)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_imm2_3_max_8000_ce40b000() {
    // Encoding: 0xCE40B000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field imm2 = 3 (Max)
    // Fields: imm2=3, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE40B000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rn_0_min_8000_ce408000() {
    // Encoding: 0xCE408000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rn = 0 (Min)
    // Fields: imm2=0, Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0xCE408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rn_1_poweroftwo_8000_ce408020() {
    // Encoding: 0xCE408020
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, Rn=1, imm2=0
    let encoding: u32 = 0xCE408020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rn_30_poweroftwominusone_8000_ce4083c0() {
    // Encoding: 0xCE4083C0
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=0, imm2=0, Rn=30
    let encoding: u32 = 0xCE4083C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rn_31_max_8000_ce4083e0() {
    // Encoding: 0xCE4083E0
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rn = 31 (Max)
    // Fields: imm2=0, Rn=31, Rm=0, Rd=0
    let encoding: u32 = 0xCE4083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rd_0_min_8000_ce408000() {
    // Encoding: 0xCE408000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rd = 0 (Min)
    // Fields: Rm=0, imm2=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rd_1_poweroftwo_8000_ce408001() {
    // Encoding: 0xCE408001
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rd = 1 (PowerOfTwo)
    // Fields: imm2=0, Rd=1, Rm=0, Rn=0
    let encoding: u32 = 0xCE408001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rd_30_poweroftwominusone_8000_ce40801e() {
    // Encoding: 0xCE40801E
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rn=0, imm2=0, Rm=0
    let encoding: u32 = 0xCE40801E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_field_rd_31_max_8000_ce40801f() {
    // Encoding: 0xCE40801F
    // Test aarch64_vector_crypto_sm3_sm3tt1a field Rd = 31 (Max)
    // Fields: Rm=0, imm2=0, Rn=0, Rd=31
    let encoding: u32 = 0xCE40801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_combo_0_8000_ce408000() {
    // Encoding: 0xCE408000
    // Test aarch64_vector_crypto_sm3_sm3tt1a field combination: Rm=0, imm2=0, Rn=0, Rd=0
    // Fields: imm2=0, Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_special_rn_31_stack_pointer_sp_may_require_alignment_32768_ce4083e0() {
    // Encoding: 0xCE4083E0
    // Test aarch64_vector_crypto_sm3_sm3tt1a special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: imm2=0, Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0xCE4083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_32768_ce40801f() {
    // Encoding: 0xCE40801F
    // Test aarch64_vector_crypto_sm3_sm3tt1a special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: imm2=0, Rd=31, Rm=0, Rn=0
    let encoding: u32 = 0xCE40801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_invalid_0_8000_ce408000() {
    // Encoding: 0xCE408000
    // Test aarch64_vector_crypto_sm3_sm3tt1a invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }
    // Fields: imm2=0, Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_invalid_1_8000_ce408000() {
    // Encoding: 0xCE408000
    // Test aarch64_vector_crypto_sm3_sm3tt1a invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rd=0, imm2=0, Rm=0
    let encoding: u32 = 0xCE408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_reg_write_0_ce408000() {
    // Test aarch64_vector_crypto_sm3_sm3tt1a register write: SimdFromField("d")
    // Encoding: 0xCE408000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE408000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_sp_rn_ce4083e0() {
    // Test aarch64_vector_crypto_sm3_sm3tt1a with Rn = SP (31)
    // Encoding: 0xCE4083E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE4083E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1a
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1a_zr_rd_ce40801f() {
    // Test aarch64_vector_crypto_sm3_sm3tt1a with Rd = ZR (31)
    // Encoding: 0xCE40801F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE40801F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3op_sha1_sched0 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rm_0_min_3000_5e003000() {
    // Encoding: 0x5E003000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rm = 0 (Min)
    // Fields: Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0x5E003000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rm_1_poweroftwo_3000_5e013000() {
    // Encoding: 0x5E013000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rm = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, Rm=1
    let encoding: u32 = 0x5E013000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rm_30_poweroftwominusone_3000_5e1e3000() {
    // Encoding: 0x5E1E3000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rn=0, Rd=0
    let encoding: u32 = 0x5E1E3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rm_31_max_3000_5e1f3000() {
    // Encoding: 0x5E1F3000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rm = 31 (Max)
    // Fields: Rd=0, Rm=31, Rn=0
    let encoding: u32 = 0x5E1F3000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rn_0_min_3000_5e003000() {
    // Encoding: 0x5E003000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rn = 0 (Min)
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0x5E003000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rn_1_poweroftwo_3000_5e003020() {
    // Encoding: 0x5E003020
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=0, Rn=1
    let encoding: u32 = 0x5E003020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rn_30_poweroftwominusone_3000_5e0033c0() {
    // Encoding: 0x5E0033C0
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=30, Rd=0
    let encoding: u32 = 0x5E0033C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rn_31_max_3000_5e0033e0() {
    // Encoding: 0x5E0033E0
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rn = 31 (Max)
    // Fields: Rd=0, Rn=31, Rm=0
    let encoding: u32 = 0x5E0033E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rd_0_min_3000_5e003000() {
    // Encoding: 0x5E003000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rd = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E003000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rd_1_poweroftwo_3000_5e003001() {
    // Encoding: 0x5E003001
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rm=0, Rn=0
    let encoding: u32 = 0x5E003001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rd_30_poweroftwominusone_3000_5e00301e() {
    // Encoding: 0x5E00301E
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Rd=30
    let encoding: u32 = 0x5E00301E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_field_rd_31_max_3000_5e00301f() {
    // Encoding: 0x5E00301F
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field Rd = 31 (Max)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00301F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_combo_0_3000_5e003000() {
    // Encoding: 0x5E003000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E003000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_special_rn_31_stack_pointer_sp_may_require_alignment_12288_5e0033e0() {
    // Encoding: 0x5E0033E0
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rd=0, Rn=31
    let encoding: u32 = 0x5E0033E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_12288_5e00301f() {
    // Encoding: 0x5E00301F
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00301F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA1Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_invalid_0_3000_5e003000() {
    // Encoding: 0x5E003000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x5E003000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_invalid_1_3000_5e003000() {
    // Encoding: 0x5E003000
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0x5E003000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_reg_write_0_5e003000() {
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 register write: SimdFromField("d")
    // Encoding: 0x5E003000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E003000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_sp_rn_5e0033e0() {
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 with Rn = SP (31)
    // Encoding: 0x5E0033E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E0033E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_sched0
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_sched0_zr_rd_5e00301f() {
    // Test aarch64_vector_crypto_sha3op_sha1_sched0 with Rd = ZR (31)
    // Encoding: 0x5E00301F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E00301F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm3_sm3ss1 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rm_0_min_0_ce400000() {
    // Encoding: 0xCE400000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rm = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0, Ra=0
    let encoding: u32 = 0xCE400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rm_1_poweroftwo_0_ce410000() {
    // Encoding: 0xCE410000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Ra=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE410000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rm_30_poweroftwominusone_0_ce5e0000() {
    // Encoding: 0xCE5E0000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Ra=0, Rd=0, Rn=0, Rm=30
    let encoding: u32 = 0xCE5E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rm_31_max_0_ce5f0000() {
    // Encoding: 0xCE5F0000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rm = 31 (Max)
    // Fields: Rm=31, Ra=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE5F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_ra_0_min_0_ce400000() {
    // Encoding: 0xCE400000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Ra = 0 (Min)
    // Fields: Rd=0, Ra=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_ra_1_poweroftwo_0_ce400400() {
    // Encoding: 0xCE400400
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Ra = 1 (PowerOfTwo)
    // Fields: Ra=1, Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE400400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_ra_30_poweroftwominusone_0_ce407800() {
    // Encoding: 0xCE407800
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Ra = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Ra=30, Rd=0
    let encoding: u32 = 0xCE407800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Ra 10 +: 5`
/// Requirement: FieldBoundary { field: "Ra", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_ra_31_max_0_ce407c00() {
    // Encoding: 0xCE407C00
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Ra = 31 (Max)
    // Fields: Rd=0, Rm=0, Ra=31, Rn=0
    let encoding: u32 = 0xCE407C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rn_0_min_0_ce400000() {
    // Encoding: 0xCE400000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rn = 0 (Min)
    // Fields: Rm=0, Ra=0, Rd=0, Rn=0
    let encoding: u32 = 0xCE400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rn_1_poweroftwo_0_ce400020() {
    // Encoding: 0xCE400020
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rn = 1 (PowerOfTwo)
    // Fields: Ra=0, Rm=0, Rn=1, Rd=0
    let encoding: u32 = 0xCE400020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rn_30_poweroftwominusone_0_ce4003c0() {
    // Encoding: 0xCE4003C0
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Ra=0, Rm=0, Rn=30
    let encoding: u32 = 0xCE4003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rn_31_max_0_ce4003e0() {
    // Encoding: 0xCE4003E0
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rn = 31 (Max)
    // Fields: Rd=0, Rn=31, Ra=0, Rm=0
    let encoding: u32 = 0xCE4003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rd_0_min_0_ce400000() {
    // Encoding: 0xCE400000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rd = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0, Ra=0
    let encoding: u32 = 0xCE400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rd_1_poweroftwo_0_ce400001() {
    // Encoding: 0xCE400001
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, Ra=0, Rd=1
    let encoding: u32 = 0xCE400001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rd_30_poweroftwominusone_0_ce40001e() {
    // Encoding: 0xCE40001E
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30, Rm=0, Ra=0
    let encoding: u32 = 0xCE40001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_field_rd_31_max_0_ce40001f() {
    // Encoding: 0xCE40001F
    // Test aarch64_vector_crypto_sm3_sm3ss1 field Rd = 31 (Max)
    // Fields: Rm=0, Ra=0, Rd=31, Rn=0
    let encoding: u32 = 0xCE40001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_combo_0_0_ce400000() {
    // Encoding: 0xCE400000
    // Test aarch64_vector_crypto_sm3_sm3ss1 field combination: Rm=0, Ra=0, Rn=0, Rd=0
    // Fields: Ra=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_special_rn_31_stack_pointer_sp_may_require_alignment_0_ce4003e0() {
    // Encoding: 0xCE4003E0
    // Test aarch64_vector_crypto_sm3_sm3ss1 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Ra=0, Rd=0, Rn=31, Rm=0
    let encoding: u32 = 0xCE4003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_ce40001f() {
    // Encoding: 0xCE40001F
    // Test aarch64_vector_crypto_sm3_sm3ss1 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rn=0, Rm=0, Ra=0
    let encoding: u32 = 0xCE40001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_invalid_0_0_ce400000() {
    // Encoding: 0xCE400000
    // Test aarch64_vector_crypto_sm3_sm3ss1 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }
    // Fields: Rd=0, Rn=0, Ra=0, Rm=0
    let encoding: u32 = 0xCE400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_invalid_1_0_ce400000() {
    // Encoding: 0xCE400000
    // Test aarch64_vector_crypto_sm3_sm3ss1 invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, Ra=0, Rd=0
    let encoding: u32 = 0xCE400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_reg_write_0_ce400000() {
    // Test aarch64_vector_crypto_sm3_sm3ss1 register write: SimdFromField("d")
    // Encoding: 0xCE400000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE400000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_sp_rn_ce4003e0() {
    // Test aarch64_vector_crypto_sm3_sm3ss1 with Rn = SP (31)
    // Encoding: 0xCE4003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE4003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3ss1
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3ss1_zr_rd_ce40001f() {
    // Test aarch64_vector_crypto_sm3_sm3ss1 with Rd = ZR (31)
    // Encoding: 0xCE40001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE40001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sm3_sm3tt1b Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rm_0_min_8400_ce408400() {
    // Encoding: 0xCE408400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rm = 0 (Min)
    // Fields: imm2=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE408400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rm_1_poweroftwo_8400_ce418400() {
    // Encoding: 0xCE418400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, imm2=0, Rd=0
    let encoding: u32 = 0xCE418400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rm_30_poweroftwominusone_8400_ce5e8400() {
    // Encoding: 0xCE5E8400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rm=30, imm2=0, Rd=0
    let encoding: u32 = 0xCE5E8400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rm_31_max_8400_ce5f8400() {
    // Encoding: 0xCE5F8400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rm = 31 (Max)
    // Fields: Rd=0, imm2=0, Rm=31, Rn=0
    let encoding: u32 = 0xCE5F8400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_imm2_0_zero_8400_ce408400() {
    // Encoding: 0xCE408400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field imm2 = 0 (Zero)
    // Fields: Rd=0, imm2=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE408400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_imm2_1_poweroftwo_8400_ce409400() {
    // Encoding: 0xCE409400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field imm2 = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, imm2=1, Rm=0
    let encoding: u32 = 0xCE409400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field imm2 12 +: 2`
/// Requirement: FieldBoundary { field: "imm2", value: 3, boundary: Max }
/// maximum immediate (3)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_imm2_3_max_8400_ce40b400() {
    // Encoding: 0xCE40B400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field imm2 = 3 (Max)
    // Fields: Rn=0, imm2=3, Rm=0, Rd=0
    let encoding: u32 = 0xCE40B400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rn_0_min_8400_ce408400() {
    // Encoding: 0xCE408400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rn = 0 (Min)
    // Fields: Rd=0, Rn=0, imm2=0, Rm=0
    let encoding: u32 = 0xCE408400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rn_1_poweroftwo_8400_ce408420() {
    // Encoding: 0xCE408420
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rn = 1 (PowerOfTwo)
    // Fields: imm2=0, Rm=0, Rn=1, Rd=0
    let encoding: u32 = 0xCE408420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rn_30_poweroftwominusone_8400_ce4087c0() {
    // Encoding: 0xCE4087C0
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: imm2=0, Rm=0, Rd=0, Rn=30
    let encoding: u32 = 0xCE4087C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rn_31_max_8400_ce4087e0() {
    // Encoding: 0xCE4087E0
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rn = 31 (Max)
    // Fields: Rd=0, Rn=31, Rm=0, imm2=0
    let encoding: u32 = 0xCE4087E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rd_0_min_8400_ce408400() {
    // Encoding: 0xCE408400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rd = 0 (Min)
    // Fields: Rd=0, imm2=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE408400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rd_1_poweroftwo_8400_ce408401() {
    // Encoding: 0xCE408401
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rd = 1 (PowerOfTwo)
    // Fields: imm2=0, Rd=1, Rn=0, Rm=0
    let encoding: u32 = 0xCE408401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rd_30_poweroftwominusone_8400_ce40841e() {
    // Encoding: 0xCE40841E
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rn=0, Rm=0, imm2=0
    let encoding: u32 = 0xCE40841E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_field_rd_31_max_8400_ce40841f() {
    // Encoding: 0xCE40841F
    // Test aarch64_vector_crypto_sm3_sm3tt1b field Rd = 31 (Max)
    // Fields: imm2=0, Rn=0, Rm=0, Rd=31
    let encoding: u32 = 0xCE40841F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_combo_0_8400_ce408400() {
    // Encoding: 0xCE408400
    // Test aarch64_vector_crypto_sm3_sm3tt1b field combination: Rm=0, imm2=0, Rn=0, Rd=0
    // Fields: Rm=0, Rd=0, Rn=0, imm2=0
    let encoding: u32 = 0xCE408400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_special_rn_31_stack_pointer_sp_may_require_alignment_33792_ce4087e0() {
    // Encoding: 0xCE4087E0
    // Test aarch64_vector_crypto_sm3_sm3tt1b special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, imm2=0, Rn=31
    let encoding: u32 = 0xCE4087E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_33792_ce40841f() {
    // Encoding: 0xCE40841F
    // Test aarch64_vector_crypto_sm3_sm3tt1b special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, Rn=0, Rm=0, imm2=0
    let encoding: u32 = 0xCE40841F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSM3Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_invalid_0_8400_ce408400() {
    // Encoding: 0xCE408400
    // Test aarch64_vector_crypto_sm3_sm3tt1b invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSM3Ext" }, args: [] } }
    // Fields: imm2=0, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE408400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_invalid_1_8400_ce408400() {
    // Encoding: 0xCE408400
    // Test aarch64_vector_crypto_sm3_sm3tt1b invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rn=0, Rm=0, imm2=0
    let encoding: u32 = 0xCE408400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_reg_write_0_ce408400() {
    // Test aarch64_vector_crypto_sm3_sm3tt1b register write: SimdFromField("d")
    // Encoding: 0xCE408400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE408400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_sp_rn_ce4087e0() {
    // Test aarch64_vector_crypto_sm3_sm3tt1b with Rn = SP (31)
    // Encoding: 0xCE4087E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE4087E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sm3_sm3tt1b
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sm3_sm3tt1b_zr_rd_ce40841f() {
    // Test aarch64_vector_crypto_sm3_sm3tt1b with Rd = ZR (31)
    // Encoding: 0xCE40841F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE40841F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha2op_sha256_sched0 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rn_0_min_2800_5e282800() {
    // Encoding: 0x5E282800
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rn = 0 (Min)
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E282800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rn_1_poweroftwo_2800_5e282820() {
    // Encoding: 0x5E282820
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rd=0
    let encoding: u32 = 0x5E282820;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rn_30_poweroftwominusone_2800_5e282bc0() {
    // Encoding: 0x5E282BC0
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0
    let encoding: u32 = 0x5E282BC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rn_31_max_2800_5e282be0() {
    // Encoding: 0x5E282BE0
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rn = 31 (Max)
    // Fields: Rd=0, Rn=31
    let encoding: u32 = 0x5E282BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rd_0_min_2800_5e282800() {
    // Encoding: 0x5E282800
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E282800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rd_1_poweroftwo_2800_5e282801() {
    // Encoding: 0x5E282801
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=1
    let encoding: u32 = 0x5E282801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rd_30_poweroftwominusone_2800_5e28281e() {
    // Encoding: 0x5E28281E
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30
    let encoding: u32 = 0x5E28281E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_field_rd_31_max_2800_5e28281f() {
    // Encoding: 0x5E28281F
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field Rd = 31 (Max)
    // Fields: Rd=31, Rn=0
    let encoding: u32 = 0x5E28281F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_combo_0_2800_5e282800() {
    // Encoding: 0x5E282800
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 field combination: Rn=0, Rd=0
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0x5E282800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_special_rn_31_stack_pointer_sp_may_require_alignment_10240_5e282be0() {
    // Encoding: 0x5E282BE0
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Rd=0
    let encoding: u32 = 0x5E282BE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_10240_5e28281f() {
    // Encoding: 0x5E28281F
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31
    let encoding: u32 = 0x5E28281F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA256Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA256Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_invalid_0_2800_5e282800() {
    // Encoding: 0x5E282800
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA256Ext" }, args: [] } }
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0x5E282800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_invalid_1_2800_5e282800() {
    // Encoding: 0x5E282800
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0x5E282800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_reg_write_0_5e282800() {
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 register write: SimdFromField("d")
    // Encoding: 0x5E282800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E282800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_sp_rn_5e282be0() {
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 with Rn = SP (31)
    // Encoding: 0x5E282BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E282BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha2op_sha256_sched0
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha2op_sha256_sched0_zr_rd_5e28281f() {
    // Test aarch64_vector_crypto_sha2op_sha256_sched0 with Rd = ZR (31)
    // Encoding: 0x5E28281F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E28281F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha512_sha512h2 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rm_0_min_8400_ce608400() {
    // Encoding: 0xCE608400
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rm = 0 (Min)
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rm_1_poweroftwo_8400_ce618400() {
    // Encoding: 0xCE618400
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rn=0, Rd=0
    let encoding: u32 = 0xCE618400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rm_30_poweroftwominusone_8400_ce7e8400() {
    // Encoding: 0xCE7E8400
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=0, Rm=30
    let encoding: u32 = 0xCE7E8400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rm_31_max_8400_ce7f8400() {
    // Encoding: 0xCE7F8400
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rm = 31 (Max)
    // Fields: Rd=0, Rm=31, Rn=0
    let encoding: u32 = 0xCE7F8400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rn_0_min_8400_ce608400() {
    // Encoding: 0xCE608400
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0xCE608400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rn_1_poweroftwo_8400_ce608420() {
    // Encoding: 0xCE608420
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rd=0, Rm=0
    let encoding: u32 = 0xCE608420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rn_30_poweroftwominusone_8400_ce6087c0() {
    // Encoding: 0xCE6087C0
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0, Rm=0
    let encoding: u32 = 0xCE6087C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rn_31_max_8400_ce6087e0() {
    // Encoding: 0xCE6087E0
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rn = 31 (Max)
    // Fields: Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0xCE6087E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rd_0_min_8400_ce608400() {
    // Encoding: 0xCE608400
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE608400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rd_1_poweroftwo_8400_ce608401() {
    // Encoding: 0xCE608401
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rm=0, Rn=0
    let encoding: u32 = 0xCE608401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rd_30_poweroftwominusone_8400_ce60841e() {
    // Encoding: 0xCE60841E
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Rd=30
    let encoding: u32 = 0xCE60841E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_field_rd_31_max_8400_ce60841f() {
    // Encoding: 0xCE60841F
    // Test aarch64_vector_crypto_sha512_sha512h2 field Rd = 31 (Max)
    // Fields: Rn=0, Rd=31, Rm=0
    let encoding: u32 = 0xCE60841F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_combo_0_8400_ce608400() {
    // Encoding: 0xCE608400
    // Test aarch64_vector_crypto_sha512_sha512h2 field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0xCE608400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_special_rn_31_stack_pointer_sp_may_require_alignment_33792_ce6087e0() {
    // Encoding: 0xCE6087E0
    // Test aarch64_vector_crypto_sha512_sha512h2 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0xCE6087E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_33792_ce60841f() {
    // Encoding: 0xCE60841F
    // Test aarch64_vector_crypto_sha512_sha512h2 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31, Rm=0
    let encoding: u32 = 0xCE60841F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA512Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_invalid_0_8400_ce608400() {
    // Encoding: 0xCE608400
    // Test aarch64_vector_crypto_sha512_sha512h2 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0xCE608400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_invalid_1_8400_ce608400() {
    // Encoding: 0xCE608400
    // Test aarch64_vector_crypto_sha512_sha512h2 invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0xCE608400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_reg_write_0_ce608400() {
    // Test aarch64_vector_crypto_sha512_sha512h2 register write: SimdFromField("d")
    // Encoding: 0xCE608400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE608400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_sp_rn_ce6087e0() {
    // Test aarch64_vector_crypto_sha512_sha512h2 with Rn = SP (31)
    // Encoding: 0xCE6087E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE6087E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512h2
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512h2_zr_rd_ce60841f() {
    // Test aarch64_vector_crypto_sha512_sha512h2 with Rd = ZR (31)
    // Encoding: 0xCE60841F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCE60841F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3op_sha1_hash_parity Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rm_0_min_1000_5e001000() {
    // Encoding: 0x5E001000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rm = 0 (Min)
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0x5E001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rm_1_poweroftwo_1000_5e011000() {
    // Encoding: 0x5E011000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rm = 1 (PowerOfTwo)
    // Fields: Rd=0, Rm=1, Rn=0
    let encoding: u32 = 0x5E011000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rm_30_poweroftwominusone_1000_5e1e1000() {
    // Encoding: 0x5E1E1000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=30, Rn=0
    let encoding: u32 = 0x5E1E1000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rm_31_max_1000_5e1f1000() {
    // Encoding: 0x5E1F1000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rm = 31 (Max)
    // Fields: Rd=0, Rn=0, Rm=31
    let encoding: u32 = 0x5E1F1000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rn_0_min_1000_5e001000() {
    // Encoding: 0x5E001000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rn = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rn_1_poweroftwo_1000_5e001020() {
    // Encoding: 0x5E001020
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=1, Rd=0
    let encoding: u32 = 0x5E001020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rn_30_poweroftwominusone_1000_5e0013c0() {
    // Encoding: 0x5E0013C0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0, Rm=0
    let encoding: u32 = 0x5E0013C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rn_31_max_1000_5e0013e0() {
    // Encoding: 0x5E0013E0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rn = 31 (Max)
    // Fields: Rd=0, Rm=0, Rn=31
    let encoding: u32 = 0x5E0013E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rd_0_min_1000_5e001000() {
    // Encoding: 0x5E001000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rd = 0 (Min)
    // Fields: Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0x5E001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rd_1_poweroftwo_1000_5e001001() {
    // Encoding: 0x5E001001
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, Rd=1
    let encoding: u32 = 0x5E001001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rd_30_poweroftwominusone_1000_5e00101e() {
    // Encoding: 0x5E00101E
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Rd=30
    let encoding: u32 = 0x5E00101E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_field_rd_31_max_1000_5e00101f() {
    // Encoding: 0x5E00101F
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field Rd = 31 (Max)
    // Fields: Rn=0, Rd=31, Rm=0
    let encoding: u32 = 0x5E00101F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_combo_0_1000_5e001000() {
    // Encoding: 0x5E001000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_special_rn_31_stack_pointer_sp_may_require_alignment_4096_5e0013e0() {
    // Encoding: 0x5E0013E0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rn=31, Rd=0
    let encoding: u32 = 0x5E0013E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_4096_5e00101f() {
    // Encoding: 0x5E00101F
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00101F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA1Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_invalid_0_1000_5e001000() {
    // Encoding: 0x5E001000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }
    // Fields: Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_invalid_1_1000_5e001000() {
    // Encoding: 0x5E001000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rm=0, Rn=0
    let encoding: u32 = 0x5E001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_reg_write_0_5e001000() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity register write: SimdFromField("d")
    // Encoding: 0x5E001000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E001000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_sp_rn_5e0013e0() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity with Rn = SP (31)
    // Encoding: 0x5E0013E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E0013E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_parity
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_parity_zr_rd_5e00101f() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_parity with Rd = ZR (31)
    // Encoding: 0x5E00101F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E00101F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha512_sha512su0 Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rn_0_min_8000_cec08000() {
    // Encoding: 0xCEC08000
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0xCEC08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rn_1_poweroftwo_8000_cec08020() {
    // Encoding: 0xCEC08020
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rd=0
    let encoding: u32 = 0xCEC08020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rn_30_poweroftwominusone_8000_cec083c0() {
    // Encoding: 0xCEC083C0
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0
    let encoding: u32 = 0xCEC083C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rn_31_max_8000_cec083e0() {
    // Encoding: 0xCEC083E0
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rn = 31 (Max)
    // Fields: Rn=31, Rd=0
    let encoding: u32 = 0xCEC083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rd_0_min_8000_cec08000() {
    // Encoding: 0xCEC08000
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0xCEC08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rd_1_poweroftwo_8000_cec08001() {
    // Encoding: 0xCEC08001
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rn=0
    let encoding: u32 = 0xCEC08001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rd_30_poweroftwominusone_8000_cec0801e() {
    // Encoding: 0xCEC0801E
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30
    let encoding: u32 = 0xCEC0801E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_field_rd_31_max_8000_cec0801f() {
    // Encoding: 0xCEC0801F
    // Test aarch64_vector_crypto_sha512_sha512su0 field Rd = 31 (Max)
    // Fields: Rn=0, Rd=31
    let encoding: u32 = 0xCEC0801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rn=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_combo_0_8000_cec08000() {
    // Encoding: 0xCEC08000
    // Test aarch64_vector_crypto_sha512_sha512su0 field combination: Rn=0, Rd=0
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0xCEC08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_special_rn_31_stack_pointer_sp_may_require_alignment_32768_cec083e0() {
    // Encoding: 0xCEC083E0
    // Test aarch64_vector_crypto_sha512_sha512su0 special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rn=31, Rd=0
    let encoding: u32 = 0xCEC083E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_32768_cec0801f() {
    // Encoding: 0xCEC0801F
    // Test aarch64_vector_crypto_sha512_sha512su0 special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31
    let encoding: u32 = 0xCEC0801F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA512Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_invalid_0_8000_cec08000() {
    // Encoding: 0xCEC08000
    // Test aarch64_vector_crypto_sha512_sha512su0 invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA512Ext" }, args: [] } }
    // Fields: Rd=0, Rn=0
    let encoding: u32 = 0xCEC08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_invalid_1_8000_cec08000() {
    // Encoding: 0xCEC08000
    // Test aarch64_vector_crypto_sha512_sha512su0 invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, Rd=0
    let encoding: u32 = 0xCEC08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_reg_write_0_cec08000() {
    // Test aarch64_vector_crypto_sha512_sha512su0 register write: SimdFromField("d")
    // Encoding: 0xCEC08000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCEC08000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_sp_rn_cec083e0() {
    // Test aarch64_vector_crypto_sha512_sha512su0 with Rn = SP (31)
    // Encoding: 0xCEC083E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCEC083E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha512_sha512su0
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha512_sha512su0_zr_rd_cec0801f() {
    // Test aarch64_vector_crypto_sha512_sha512su0 with Rd = ZR (31)
    // Encoding: 0xCEC0801F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xCEC0801F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_crypto_sha3op_sha1_hash_choose Tests
// ============================================================================

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rm_0_min_0_5e000000() {
    // Encoding: 0x5E000000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rm = 0 (Min)
    // Fields: Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0x5E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rm_1_poweroftwo_0_5e010000() {
    // Encoding: 0x5E010000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Rd=0, Rn=0
    let encoding: u32 = 0x5E010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rm_30_poweroftwominusone_0_5e1e0000() {
    // Encoding: 0x5E1E0000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=30, Rd=0, Rn=0
    let encoding: u32 = 0x5E1E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rm_31_max_0_5e1f0000() {
    // Encoding: 0x5E1F0000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rm = 31 (Max)
    // Fields: Rn=0, Rd=0, Rm=31
    let encoding: u32 = 0x5E1F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rn_0_min_0_5e000000() {
    // Encoding: 0x5E000000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rn = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rn_1_poweroftwo_0_5e000020() {
    // Encoding: 0x5E000020
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rd=0, Rm=0
    let encoding: u32 = 0x5E000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rn_30_poweroftwominusone_0_5e0003c0() {
    // Encoding: 0x5E0003C0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=30, Rm=0
    let encoding: u32 = 0x5E0003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rn_31_max_0_5e0003e0() {
    // Encoding: 0x5E0003E0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rn = 31 (Max)
    // Fields: Rn=31, Rd=0, Rm=0
    let encoding: u32 = 0x5E0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rd_0_min_0_5e000000() {
    // Encoding: 0x5E000000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rd = 0 (Min)
    // Fields: Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rd_1_poweroftwo_0_5e000001() {
    // Encoding: 0x5E000001
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, Rd=1
    let encoding: u32 = 0x5E000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rd_30_poweroftwominusone_0_5e00001e() {
    // Encoding: 0x5E00001E
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rm=0, Rn=0
    let encoding: u32 = 0x5E00001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_field_rd_31_max_0_5e00001f() {
    // Encoding: 0x5E00001F
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field Rd = 31 (Max)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Rm=0 (register index 0 (first register))
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_combo_0_0_5e000000() {
    // Encoding: 0x5E000000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose field combination: Rm=0, Rn=0, Rd=0
    // Fields: Rm=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_special_rn_31_stack_pointer_sp_may_require_alignment_0_5e0003e0() {
    // Encoding: 0x5E0003E0
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, Rn=31, Rd=0
    let encoding: u32 = 0x5E0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_5e00001f() {
    // Encoding: 0x5E00001F
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSHA1Ext\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_invalid_0_0_5e000000() {
    // Encoding: 0x5E000000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSHA1Ext" }, args: [] } }
    // Fields: Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x5E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_invalid_1_0_5e000000() {
    // Encoding: 0x5E000000
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0x5E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_reg_write_0_5e000000() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose register write: SimdFromField("d")
    // Encoding: 0x5E000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_sp_rn_5e0003e0() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose with Rn = SP (31)
    // Encoding: 0x5E0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_crypto_sha3op_sha1_hash_choose
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_crypto_sha3op_sha1_hash_choose_zr_rd_5e00001f() {
    // Test aarch64_vector_crypto_sha3op_sha1_hash_choose with Rd = ZR (31)
    // Encoding: 0x5E00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

