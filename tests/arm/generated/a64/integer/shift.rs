//! A64 integer shift tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_integer_shift_variable Tests
// ============================================================================

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_integer_shift_variable_field_sf_0_min_2000_1ac02000() {
    // Encoding: 0x1AC02000
    // Test aarch64_integer_shift_variable field sf = 0 (Min)
    // Fields: Rd=0, sf=0, Rm=0, Rn=0, op2=0
    let encoding: u32 = 0x1AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_integer_shift_variable_field_sf_1_max_2000_9ac02000() {
    // Encoding: 0x9AC02000
    // Test aarch64_integer_shift_variable field sf = 1 (Max)
    // Fields: sf=1, Rm=0, Rn=0, Rd=0, op2=0
    let encoding: u32 = 0x9AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_shift_variable_field_rm_0_min_2000_1ac02000() {
    // Encoding: 0x1AC02000
    // Test aarch64_integer_shift_variable field Rm = 0 (Min)
    // Fields: op2=0, Rm=0, Rd=0, Rn=0, sf=0
    let encoding: u32 = 0x1AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_shift_variable_field_rm_1_poweroftwo_2000_1ac12000() {
    // Encoding: 0x1AC12000
    // Test aarch64_integer_shift_variable field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, op2=0, Rn=0, Rd=0, sf=0
    let encoding: u32 = 0x1AC12000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_shift_variable_field_rm_30_poweroftwominusone_2000_1ade2000() {
    // Encoding: 0x1ADE2000
    // Test aarch64_integer_shift_variable field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, op2=0, sf=0, Rd=0, Rm=30
    let encoding: u32 = 0x1ADE2000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_integer_shift_variable_field_rm_31_max_2000_1adf2000() {
    // Encoding: 0x1ADF2000
    // Test aarch64_integer_shift_variable field Rm = 31 (Max)
    // Fields: Rd=0, sf=0, Rm=31, op2=0, Rn=0
    let encoding: u32 = 0x1ADF2000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field op2 10 +: 2`
/// Requirement: FieldBoundary { field: "op2", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_integer_shift_variable_field_op2_0_min_2000_1ac02000() {
    // Encoding: 0x1AC02000
    // Test aarch64_integer_shift_variable field op2 = 0 (Min)
    // Fields: Rd=0, sf=0, Rn=0, Rm=0, op2=0
    let encoding: u32 = 0x1AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field op2 10 +: 2`
/// Requirement: FieldBoundary { field: "op2", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_integer_shift_variable_field_op2_1_poweroftwo_2000_1ac02400() {
    // Encoding: 0x1AC02400
    // Test aarch64_integer_shift_variable field op2 = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, op2=1, sf=0, Rd=0
    let encoding: u32 = 0x1AC02400;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field op2 10 +: 2`
/// Requirement: FieldBoundary { field: "op2", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_integer_shift_variable_field_op2_3_max_2000_1ac02c00() {
    // Encoding: 0x1AC02C00
    // Test aarch64_integer_shift_variable field op2 = 3 (Max)
    // Fields: Rn=0, Rd=0, sf=0, op2=3, Rm=0
    let encoding: u32 = 0x1AC02C00;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_shift_variable_field_rn_0_min_2000_1ac02000() {
    // Encoding: 0x1AC02000
    // Test aarch64_integer_shift_variable field Rn = 0 (Min)
    // Fields: Rm=0, op2=0, sf=0, Rn=0, Rd=0
    let encoding: u32 = 0x1AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_shift_variable_field_rn_1_poweroftwo_2000_1ac02020() {
    // Encoding: 0x1AC02020
    // Test aarch64_integer_shift_variable field Rn = 1 (PowerOfTwo)
    // Fields: op2=0, sf=0, Rd=0, Rn=1, Rm=0
    let encoding: u32 = 0x1AC02020;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_shift_variable_field_rn_30_poweroftwominusone_2000_1ac023c0() {
    // Encoding: 0x1AC023C0
    // Test aarch64_integer_shift_variable field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: sf=0, Rm=0, Rn=30, op2=0, Rd=0
    let encoding: u32 = 0x1AC023C0;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_integer_shift_variable_field_rn_31_max_2000_1ac023e0() {
    // Encoding: 0x1AC023E0
    // Test aarch64_integer_shift_variable field Rn = 31 (Max)
    // Fields: Rm=0, Rn=31, sf=0, Rd=0, op2=0
    let encoding: u32 = 0x1AC023E0;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_shift_variable_field_rd_0_min_2000_1ac02000() {
    // Encoding: 0x1AC02000
    // Test aarch64_integer_shift_variable field Rd = 0 (Min)
    // Fields: Rd=0, Rm=0, op2=0, Rn=0, sf=0
    let encoding: u32 = 0x1AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_shift_variable_field_rd_1_poweroftwo_2000_1ac02001() {
    // Encoding: 0x1AC02001
    // Test aarch64_integer_shift_variable field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, op2=0, Rd=1, sf=0, Rm=0
    let encoding: u32 = 0x1AC02001;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_shift_variable_field_rd_30_poweroftwominusone_2000_1ac0201e() {
    // Encoding: 0x1AC0201E
    // Test aarch64_integer_shift_variable field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, sf=0, Rm=0, Rd=30, op2=0
    let encoding: u32 = 0x1AC0201E;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_integer_shift_variable_field_rd_31_max_2000_1ac0201f() {
    // Encoding: 0x1AC0201F
    // Test aarch64_integer_shift_variable field Rd = 31 (Max)
    // Fields: Rn=0, Rd=31, sf=0, Rm=0, op2=0
    let encoding: u32 = 0x1AC0201F;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// sf=0 (8-bit / byte size)
#[test]
fn test_aarch64_integer_shift_variable_combo_0_2000_1ac02000() {
    // Encoding: 0x1AC02000
    // Test aarch64_integer_shift_variable field combination: sf=0, Rm=0, op2=0, Rn=0, Rd=0
    // Fields: op2=0, sf=0, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x1AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_integer_shift_variable_special_sf_0_size_variant_0_8192_1ac02000() {
    // Encoding: 0x1AC02000
    // Test aarch64_integer_shift_variable special value sf = 0 (Size variant 0)
    // Fields: sf=0, Rd=0, Rm=0, Rn=0, op2=0
    let encoding: u32 = 0x1AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_integer_shift_variable_special_sf_1_size_variant_1_8192_9ac02000() {
    // Encoding: 0x9AC02000
    // Test aarch64_integer_shift_variable special value sf = 1 (Size variant 1)
    // Fields: Rd=0, op2=0, Rm=0, Rn=0, sf=1
    let encoding: u32 = 0x9AC02000;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_integer_shift_variable_special_rn_31_stack_pointer_sp_may_require_alignment_8192_1ac023e0(
) {
    // Encoding: 0x1AC023E0
    // Test aarch64_integer_shift_variable special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, op2=0, Rm=0, Rn=31, sf=0
    let encoding: u32 = 0x1AC023E0;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_integer_shift_variable_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_8192_1ac0201f(
) {
    // Encoding: 0x1AC0201F
    // Test aarch64_integer_shift_variable special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rm=0, op2=0, Rn=0, Rd=31, sf=0
    let encoding: u32 = 0x1AC0201F;
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// small values - high bits zero
#[test]
fn test_aarch64_integer_shift_variable_umulh_oracle_0_9bc27c20() {
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// large value * 2 - produces high bits
#[test]
fn test_aarch64_integer_shift_variable_umulh_oracle_1_9bc27c20() {
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max * max unsigned
#[test]
fn test_aarch64_integer_shift_variable_umulh_oracle_2_9bc27c20() {
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// max positive * max positive
#[test]
fn test_aarch64_integer_shift_variable_umulh_oracle_3_9bc27c20() {
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

/// Provenance: aarch64_integer_shift_variable
/// ASL: `UMULH X0, X1, X2`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// 2^32 * 2^32
#[test]
fn test_aarch64_integer_shift_variable_umulh_oracle_4_9bc27c20() {
    // Test UMULH: 2^32 * 2^32 (oracle)
    // Encoding: 0x9BC27C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x100000000);
    set_x(&mut cpu, 2, 0x100000000);
    let encoding: u32 = 0x9BC27C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_integer_shift_variable
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_integer_shift_variable_reg_write_0_1ac02000() {
    // Test aarch64_integer_shift_variable register write: GpFromField("d")
    // Encoding: 0x1AC02000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1AC02000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_shift_variable
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_integer_shift_variable_sp_rn_1ac023e0() {
    // Test aarch64_integer_shift_variable with Rn = SP (31)
    // Encoding: 0x1AC023E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1AC023E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_shift_variable
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_integer_shift_variable_zr_rd_1ac0201f() {
    // Test aarch64_integer_shift_variable with Rd = ZR (31)
    // Encoding: 0x1AC0201F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1AC0201F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}
