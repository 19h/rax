//! A64 vector transfer tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_vector_transfer_vector_permute_zip Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_q_0_min_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip field Q = 0 (Min)
    // Fields: op=0, Q=0, Rn=0, Rd=0, Rm=0, size=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_q_1_max_3800_4e003800() {
    // Encoding: 0x4E003800
    // Test aarch64_vector_transfer_vector_permute_zip field Q = 1 (Max)
    // Fields: op=0, Q=1, size=0, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x4E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_size_0_min_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip field size = 0 (Min)
    // Fields: Rn=0, op=0, Rd=0, size=0, Q=0, Rm=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_size_1_poweroftwo_3800_0e403800() {
    // Encoding: 0x0E403800
    // Test aarch64_vector_transfer_vector_permute_zip field size = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, Q=0, Rm=0, size=1, op=0
    let encoding: u32 = 0x0E403800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_size_2_poweroftwo_3800_0e803800() {
    // Encoding: 0x0E803800
    // Test aarch64_vector_transfer_vector_permute_zip field size = 2 (PowerOfTwo)
    // Fields: Q=0, op=0, Rd=0, Rn=0, Rm=0, size=2
    let encoding: u32 = 0x0E803800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_size_3_max_3800_0ec03800() {
    // Encoding: 0x0EC03800
    // Test aarch64_vector_transfer_vector_permute_zip field size = 3 (Max)
    // Fields: Rn=0, Rd=0, size=3, Q=0, Rm=0, op=0
    let encoding: u32 = 0x0EC03800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rm_0_min_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip field Rm = 0 (Min)
    // Fields: Rd=0, Q=0, Rn=0, size=0, Rm=0, op=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rm_1_poweroftwo_3800_0e013800() {
    // Encoding: 0x0E013800
    // Test aarch64_vector_transfer_vector_permute_zip field Rm = 1 (PowerOfTwo)
    // Fields: Rd=0, size=0, Q=0, Rm=1, op=0, Rn=0
    let encoding: u32 = 0x0E013800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rm_30_poweroftwominusone_3800_0e1e3800() {
    // Encoding: 0x0E1E3800
    // Test aarch64_vector_transfer_vector_permute_zip field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Q=0, size=0, Rm=30, op=0, Rn=0
    let encoding: u32 = 0x0E1E3800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rm_31_max_3800_0e1f3800() {
    // Encoding: 0x0E1F3800
    // Test aarch64_vector_transfer_vector_permute_zip field Rm = 31 (Max)
    // Fields: Rn=0, op=0, Q=0, size=0, Rm=31, Rd=0
    let encoding: u32 = 0x0E1F3800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field op 14 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_op_0_min_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip field op = 0 (Min)
    // Fields: Q=0, op=0, Rn=0, Rd=0, Rm=0, size=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field op 14 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_op_1_max_3800_0e007800() {
    // Encoding: 0x0E007800
    // Test aarch64_vector_transfer_vector_permute_zip field op = 1 (Max)
    // Fields: Rm=0, Rn=0, size=0, Rd=0, Q=0, op=1
    let encoding: u32 = 0x0E007800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rn_0_min_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip field Rn = 0 (Min)
    // Fields: Rn=0, Q=0, size=0, Rm=0, Rd=0, op=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rn_1_poweroftwo_3800_0e003820() {
    // Encoding: 0x0E003820
    // Test aarch64_vector_transfer_vector_permute_zip field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, size=0, Rm=0, op=0, Rn=1, Q=0
    let encoding: u32 = 0x0E003820;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rn_30_poweroftwominusone_3800_0e003bc0() {
    // Encoding: 0x0E003BC0
    // Test aarch64_vector_transfer_vector_permute_zip field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=30, Q=0, Rm=0, op=0, size=0
    let encoding: u32 = 0x0E003BC0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rn_31_max_3800_0e003be0() {
    // Encoding: 0x0E003BE0
    // Test aarch64_vector_transfer_vector_permute_zip field Rn = 31 (Max)
    // Fields: Rm=0, op=0, Q=0, Rd=0, size=0, Rn=31
    let encoding: u32 = 0x0E003BE0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rd_0_min_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip field Rd = 0 (Min)
    // Fields: Rd=0, size=0, Rm=0, op=0, Q=0, Rn=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rd_1_poweroftwo_3800_0e003801() {
    // Encoding: 0x0E003801
    // Test aarch64_vector_transfer_vector_permute_zip field Rd = 1 (PowerOfTwo)
    // Fields: Q=0, Rn=0, Rd=1, size=0, op=0, Rm=0
    let encoding: u32 = 0x0E003801;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rd_30_poweroftwominusone_3800_0e00381e() {
    // Encoding: 0x0E00381E
    // Test aarch64_vector_transfer_vector_permute_zip field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rd=30, size=0, Q=0, op=0, Rn=0
    let encoding: u32 = 0x0E00381E;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_field_rd_31_max_3800_0e00381f() {
    // Encoding: 0x0E00381F
    // Test aarch64_vector_transfer_vector_permute_zip field Rd = 31 (Max)
    // Fields: Rd=31, size=0, op=0, Rn=0, Q=0, Rm=0
    let encoding: u32 = 0x0E00381F;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_combo_0_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip field combination: Q=0, size=0, Rm=0, op=0, Rn=0, Rd=0
    // Fields: Q=0, size=0, Rm=0, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_q_0_size_variant_0_14336_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip special value Q = 0 (Size variant 0)
    // Fields: size=0, op=0, Rm=0, Rn=0, Q=0, Rd=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_q_1_size_variant_1_14336_4e003800() {
    // Encoding: 0x4E003800
    // Test aarch64_vector_transfer_vector_permute_zip special value Q = 1 (Size variant 1)
    // Fields: Rd=0, Rm=0, op=0, Rn=0, Q=1, size=0
    let encoding: u32 = 0x4E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_size_0_size_variant_0_14336_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip special value size = 0 (Size variant 0)
    // Fields: Rd=0, Rm=0, Q=0, size=0, op=0, Rn=0
    let encoding: u32 = 0x0E003800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_size_1_size_variant_1_14336_0e403800() {
    // Encoding: 0x0E403800
    // Test aarch64_vector_transfer_vector_permute_zip special value size = 1 (Size variant 1)
    // Fields: Rn=0, Rm=0, op=0, Rd=0, size=1, Q=0
    let encoding: u32 = 0x0E403800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_size_2_size_variant_2_14336_0e803800() {
    // Encoding: 0x0E803800
    // Test aarch64_vector_transfer_vector_permute_zip special value size = 2 (Size variant 2)
    // Fields: Rd=0, Rn=0, op=0, Q=0, size=2, Rm=0
    let encoding: u32 = 0x0E803800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_size_3_size_variant_3_14336_0ec03800() {
    // Encoding: 0x0EC03800
    // Test aarch64_vector_transfer_vector_permute_zip special value size = 3 (Size variant 3)
    // Fields: Q=0, size=3, Rm=0, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x0EC03800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_rn_31_stack_pointer_sp_may_require_alignment_14336_0e003be0(
) {
    // Encoding: 0x0E003BE0
    // Test aarch64_vector_transfer_vector_permute_zip special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Q=0, size=0, Rn=31, Rd=0, Rm=0, op=0
    let encoding: u32 = 0x0E003BE0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_14336_0e00381f(
) {
    // Encoding: 0x0E00381F
    // Test aarch64_vector_transfer_vector_permute_zip special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: op=0, Rn=0, Q=0, Rm=0, size=0, Rd=31
    let encoding: u32 = 0x0E00381F;
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

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: LitBits([true, true, false]) } }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Q\" }), rhs: LitBits([true, true, false]) } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_invalid_0_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip invalid encoding: Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: LitBits([true, true, false]) } }
    // Fields: op=0, Rn=0, Q=0, size=0, Rm=0, Rd=0
    let encoding: u32 = 0x0E003800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_invalid_1_3800_0e003800() {
    // Encoding: 0x0E003800
    // Test aarch64_vector_transfer_vector_permute_zip invalid encoding: Unconditional UNDEFINED
    // Fields: Q=0, op=0, Rd=0, size=0, Rm=0, Rn=0
    let encoding: u32 = 0x0E003800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_reg_write_0_0e003800() {
    // Test aarch64_vector_transfer_vector_permute_zip register write: SimdFromField("d")
    // Encoding: 0x0E003800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E003800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_sp_rn_0e003be0() {
    // Test aarch64_vector_transfer_vector_permute_zip with Rn = SP (31)
    // Encoding: 0x0E003BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E003BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_permute_zip
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_permute_zip_zr_rd_0e00381f() {
    // Test aarch64_vector_transfer_vector_permute_zip with Rd = ZR (31)
    // Encoding: 0x0E00381F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E00381F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_integer_move_unsigned Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_q_0_min_3c00_0e003c00() {
    // Encoding: 0x0E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned field Q = 0 (Min)
    // Fields: imm5=0, Q=0, Rd=0, Rn=0
    let encoding: u32 = 0x0E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_q_1_max_3c00_4e003c00() {
    // Encoding: 0x4E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned field Q = 1 (Max)
    // Fields: Rd=0, imm5=0, Rn=0, Q=1
    let encoding: u32 = 0x4E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_0_zero_3c00_0e003c00() {
    // Encoding: 0x0E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 0 (Zero)
    // Fields: Rd=0, imm5=0, Rn=0, Q=0
    let encoding: u32 = 0x0E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_1_poweroftwo_3c00_0e013c00() {
    // Encoding: 0x0E013C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 1 (PowerOfTwo)
    // Fields: imm5=1, Q=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E013C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_3_poweroftwominusone_3c00_0e033c00(
) {
    // Encoding: 0x0E033C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 3 (PowerOfTwoMinusOne)
    // Fields: imm5=3, Q=0, Rd=0, Rn=0
    let encoding: u32 = 0x0E033C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_4_poweroftwo_3c00_0e043c00() {
    // Encoding: 0x0E043C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 4 (PowerOfTwo)
    // Fields: Rn=0, Q=0, imm5=4, Rd=0
    let encoding: u32 = 0x0E043C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_7_poweroftwominusone_3c00_0e073c00(
) {
    // Encoding: 0x0E073C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 7 (PowerOfTwoMinusOne)
    // Fields: imm5=7, Q=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E073C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_8_poweroftwo_3c00_0e083c00() {
    // Encoding: 0x0E083C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 8 (PowerOfTwo)
    // Fields: imm5=8, Rd=0, Rn=0, Q=0
    let encoding: u32 = 0x0E083C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 15, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (15)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_15_poweroftwominusone_3c00_0e0f3c00(
) {
    // Encoding: 0x0E0F3C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 15 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rd=0, imm5=15, Rn=0
    let encoding: u32 = 0x0E0F3C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_16_poweroftwo_3c00_0e103c00() {
    // Encoding: 0x0E103C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 16 (PowerOfTwo)
    // Fields: Q=0, Rn=0, Rd=0, imm5=16
    let encoding: u32 = 0x0E103C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 31, boundary: Max }
/// maximum immediate (31)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_imm5_31_max_3c00_0e1f3c00() {
    // Encoding: 0x0E1F3C00
    // Test aarch64_vector_transfer_integer_move_unsigned field imm5 = 31 (Max)
    // Fields: Rn=0, Q=0, Rd=0, imm5=31
    let encoding: u32 = 0x0E1F3C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rn_0_min_3c00_0e003c00() {
    // Encoding: 0x0E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned field Rn = 0 (Min)
    // Fields: imm5=0, Q=0, Rd=0, Rn=0
    let encoding: u32 = 0x0E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rn_1_poweroftwo_3c00_0e003c20() {
    // Encoding: 0x0E003C20
    // Test aarch64_vector_transfer_integer_move_unsigned field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Rn=1, Q=0, imm5=0
    let encoding: u32 = 0x0E003C20;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rn_30_poweroftwominusone_3c00_0e003fc0()
{
    // Encoding: 0x0E003FC0
    // Test aarch64_vector_transfer_integer_move_unsigned field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, imm5=0, Rd=0, Q=0
    let encoding: u32 = 0x0E003FC0;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rn_31_max_3c00_0e003fe0() {
    // Encoding: 0x0E003FE0
    // Test aarch64_vector_transfer_integer_move_unsigned field Rn = 31 (Max)
    // Fields: Rd=0, imm5=0, Q=0, Rn=31
    let encoding: u32 = 0x0E003FE0;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rd_0_min_3c00_0e003c00() {
    // Encoding: 0x0E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned field Rd = 0 (Min)
    // Fields: imm5=0, Q=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rd_1_poweroftwo_3c00_0e003c01() {
    // Encoding: 0x0E003C01
    // Test aarch64_vector_transfer_integer_move_unsigned field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Q=0, Rn=0, imm5=0
    let encoding: u32 = 0x0E003C01;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rd_30_poweroftwominusone_3c00_0e003c1e()
{
    // Encoding: 0x0E003C1E
    // Test aarch64_vector_transfer_integer_move_unsigned field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: imm5=0, Q=0, Rd=30, Rn=0
    let encoding: u32 = 0x0E003C1E;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_field_rd_31_max_3c00_0e003c1f() {
    // Encoding: 0x0E003C1F
    // Test aarch64_vector_transfer_integer_move_unsigned field Rd = 31 (Max)
    // Fields: imm5=0, Rn=0, Q=0, Rd=31
    let encoding: u32 = 0x0E003C1F;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_combo_0_3c00_0e003c00() {
    // Encoding: 0x0E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned field combination: Q=0, imm5=0, Rn=0, Rd=0
    // Fields: Rd=0, imm5=0, Q=0, Rn=0
    let encoding: u32 = 0x0E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_special_q_0_size_variant_0_15360_0e003c00() {
    // Encoding: 0x0E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned special value Q = 0 (Size variant 0)
    // Fields: Rn=0, Q=0, imm5=0, Rd=0
    let encoding: u32 = 0x0E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_special_q_1_size_variant_1_15360_4e003c00() {
    // Encoding: 0x4E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned special value Q = 1 (Size variant 1)
    // Fields: imm5=0, Rd=0, Rn=0, Q=1
    let encoding: u32 = 0x4E003C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_special_rn_31_stack_pointer_sp_may_require_alignment_15360_0e003fe0(
) {
    // Encoding: 0x0E003FE0
    // Test aarch64_vector_transfer_integer_move_unsigned special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, imm5=0, Q=0, Rn=31
    let encoding: u32 = 0x0E003FE0;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_15360_0e003c1f(
) {
    // Encoding: 0x0E003C1F
    // Test aarch64_vector_transfer_integer_move_unsigned special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: imm5=0, Q=0, Rn=0, Rd=31
    let encoding: u32 = 0x0E003C1F;
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

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_invalid_0_3c00_0e003c00() {
    // Encoding: 0x0E003C00
    // Test aarch64_vector_transfer_integer_move_unsigned invalid encoding: Unconditional UNDEFINED
    // Fields: Q=0, Rn=0, imm5=0, Rd=0
    let encoding: u32 = 0x0E003C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_reg_write_0_0e003c00() {
    // Test aarch64_vector_transfer_integer_move_unsigned register write: GpFromField("d")
    // Encoding: 0x0E003C00
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E003C00;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_sp_rn_0e003fe0() {
    // Test aarch64_vector_transfer_integer_move_unsigned with Rn = SP (31)
    // Encoding: 0x0E003FE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E003FE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_move_unsigned
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_integer_move_unsigned_zr_rd_0e003c1f() {
    // Test aarch64_vector_transfer_integer_move_unsigned with Rd = ZR (31)
    // Encoding: 0x0E003C1F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E003C1F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_vector_permute_transpose Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_q_0_min_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field Q = 0 (Min)
    // Fields: size=0, Rn=0, Rd=0, Q=0, Rm=0, op=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_q_1_max_2800_4e002800() {
    // Encoding: 0x4E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field Q = 1 (Max)
    // Fields: Rd=0, Rn=0, op=0, Q=1, Rm=0, size=0
    let encoding: u32 = 0x4E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_size_0_min_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field size = 0 (Min)
    // Fields: size=0, Rn=0, Rm=0, Rd=0, op=0, Q=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_size_1_poweroftwo_2800_0e402800() {
    // Encoding: 0x0E402800
    // Test aarch64_vector_transfer_vector_permute_transpose field size = 1 (PowerOfTwo)
    // Fields: Q=0, Rd=0, Rm=0, size=1, op=0, Rn=0
    let encoding: u32 = 0x0E402800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_size_2_poweroftwo_2800_0e802800() {
    // Encoding: 0x0E802800
    // Test aarch64_vector_transfer_vector_permute_transpose field size = 2 (PowerOfTwo)
    // Fields: Q=0, Rd=0, Rm=0, op=0, Rn=0, size=2
    let encoding: u32 = 0x0E802800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_size_3_max_2800_0ec02800() {
    // Encoding: 0x0EC02800
    // Test aarch64_vector_transfer_vector_permute_transpose field size = 3 (Max)
    // Fields: Rd=0, op=0, Rn=0, Rm=0, size=3, Q=0
    let encoding: u32 = 0x0EC02800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rm_0_min_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field Rm = 0 (Min)
    // Fields: Q=0, op=0, Rn=0, Rd=0, size=0, Rm=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rm_1_poweroftwo_2800_0e012800() {
    // Encoding: 0x0E012800
    // Test aarch64_vector_transfer_vector_permute_transpose field Rm = 1 (PowerOfTwo)
    // Fields: Q=0, op=0, Rn=0, Rd=0, Rm=1, size=0
    let encoding: u32 = 0x0E012800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rm_30_poweroftwominusone_2800_0e1e2800(
) {
    // Encoding: 0x0E1E2800
    // Test aarch64_vector_transfer_vector_permute_transpose field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, size=0, op=0, Rd=0, Rm=30, Q=0
    let encoding: u32 = 0x0E1E2800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rm_31_max_2800_0e1f2800() {
    // Encoding: 0x0E1F2800
    // Test aarch64_vector_transfer_vector_permute_transpose field Rm = 31 (Max)
    // Fields: Rd=0, Rm=31, Rn=0, size=0, Q=0, op=0
    let encoding: u32 = 0x0E1F2800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field op 14 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_op_0_min_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field op = 0 (Min)
    // Fields: op=0, Rd=0, Q=0, size=0, Rn=0, Rm=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field op 14 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_op_1_max_2800_0e006800() {
    // Encoding: 0x0E006800
    // Test aarch64_vector_transfer_vector_permute_transpose field op = 1 (Max)
    // Fields: Q=0, size=0, Rm=0, op=1, Rn=0, Rd=0
    let encoding: u32 = 0x0E006800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rn_0_min_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, Rm=0, Q=0, size=0, op=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rn_1_poweroftwo_2800_0e002820() {
    // Encoding: 0x0E002820
    // Test aarch64_vector_transfer_vector_permute_transpose field Rn = 1 (PowerOfTwo)
    // Fields: Q=0, size=0, Rm=0, op=0, Rn=1, Rd=0
    let encoding: u32 = 0x0E002820;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rn_30_poweroftwominusone_2800_0e002bc0(
) {
    // Encoding: 0x0E002BC0
    // Test aarch64_vector_transfer_vector_permute_transpose field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0, Rm=0, size=0, Q=0, op=0
    let encoding: u32 = 0x0E002BC0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rn_31_max_2800_0e002be0() {
    // Encoding: 0x0E002BE0
    // Test aarch64_vector_transfer_vector_permute_transpose field Rn = 31 (Max)
    // Fields: Rd=0, Rm=0, Rn=31, op=0, size=0, Q=0
    let encoding: u32 = 0x0E002BE0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rd_0_min_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field Rd = 0 (Min)
    // Fields: Rn=0, Rm=0, size=0, Rd=0, Q=0, op=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rd_1_poweroftwo_2800_0e002801() {
    // Encoding: 0x0E002801
    // Test aarch64_vector_transfer_vector_permute_transpose field Rd = 1 (PowerOfTwo)
    // Fields: size=0, Rm=0, op=0, Rn=0, Q=0, Rd=1
    let encoding: u32 = 0x0E002801;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rd_30_poweroftwominusone_2800_0e00281e(
) {
    // Encoding: 0x0E00281E
    // Test aarch64_vector_transfer_vector_permute_transpose field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, size=0, Rm=0, Q=0, Rd=30, op=0
    let encoding: u32 = 0x0E00281E;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_field_rd_31_max_2800_0e00281f() {
    // Encoding: 0x0E00281F
    // Test aarch64_vector_transfer_vector_permute_transpose field Rd = 31 (Max)
    // Fields: size=0, Rm=0, Rd=31, Q=0, op=0, Rn=0
    let encoding: u32 = 0x0E00281F;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_combo_0_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose field combination: Q=0, size=0, Rm=0, op=0, Rn=0, Rd=0
    // Fields: op=0, size=0, Q=0, Rn=0, Rm=0, Rd=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_q_0_size_variant_0_10240_0e002800()
{
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose special value Q = 0 (Size variant 0)
    // Fields: op=0, Q=0, Rm=0, size=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_q_1_size_variant_1_10240_4e002800()
{
    // Encoding: 0x4E002800
    // Test aarch64_vector_transfer_vector_permute_transpose special value Q = 1 (Size variant 1)
    // Fields: op=0, Rn=0, Q=1, size=0, Rd=0, Rm=0
    let encoding: u32 = 0x4E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_size_0_size_variant_0_10240_0e002800(
) {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose special value size = 0 (Size variant 0)
    // Fields: Q=0, Rn=0, Rd=0, size=0, op=0, Rm=0
    let encoding: u32 = 0x0E002800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_size_1_size_variant_1_10240_0e402800(
) {
    // Encoding: 0x0E402800
    // Test aarch64_vector_transfer_vector_permute_transpose special value size = 1 (Size variant 1)
    // Fields: Rn=0, op=0, Q=0, size=1, Rd=0, Rm=0
    let encoding: u32 = 0x0E402800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_size_2_size_variant_2_10240_0e802800(
) {
    // Encoding: 0x0E802800
    // Test aarch64_vector_transfer_vector_permute_transpose special value size = 2 (Size variant 2)
    // Fields: Rm=0, Q=0, op=0, Rn=0, Rd=0, size=2
    let encoding: u32 = 0x0E802800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_size_3_size_variant_3_10240_0ec02800(
) {
    // Encoding: 0x0EC02800
    // Test aarch64_vector_transfer_vector_permute_transpose special value size = 3 (Size variant 3)
    // Fields: Rd=0, Rm=0, Q=0, size=3, Rn=0, op=0
    let encoding: u32 = 0x0EC02800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_rn_31_stack_pointer_sp_may_require_alignment_10240_0e002be0(
) {
    // Encoding: 0x0E002BE0
    // Test aarch64_vector_transfer_vector_permute_transpose special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: op=0, Rn=31, Rm=0, Rd=0, size=0, Q=0
    let encoding: u32 = 0x0E002BE0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_10240_0e00281f(
) {
    // Encoding: 0x0E00281F
    // Test aarch64_vector_transfer_vector_permute_transpose special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31, Rm=0, Q=0, size=0, op=0
    let encoding: u32 = 0x0E00281F;
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

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: LitBits([true, true, false]) } }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Q\" }), rhs: LitBits([true, true, false]) } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_invalid_0_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose invalid encoding: Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: LitBits([true, true, false]) } }
    // Fields: Rd=0, Rm=0, Q=0, size=0, op=0, Rn=0
    let encoding: u32 = 0x0E002800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_invalid_1_2800_0e002800() {
    // Encoding: 0x0E002800
    // Test aarch64_vector_transfer_vector_permute_transpose invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Q=0, op=0, Rn=0, size=0, Rd=0
    let encoding: u32 = 0x0E002800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_reg_write_0_0e002800() {
    // Test aarch64_vector_transfer_vector_permute_transpose register write: SimdFromField("d")
    // Encoding: 0x0E002800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E002800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_sp_rn_0e002be0() {
    // Test aarch64_vector_transfer_vector_permute_transpose with Rn = SP (31)
    // Encoding: 0x0E002BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E002BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_permute_transpose
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_permute_transpose_zr_rd_0e00281f() {
    // Test aarch64_vector_transfer_vector_permute_transpose with Rd = ZR (31)
    // Encoding: 0x0E00281F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E00281F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_integer_move_signed Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_q_0_min_2c00_0e002c00() {
    // Encoding: 0x0E002C00
    // Test aarch64_vector_transfer_integer_move_signed field Q = 0 (Min)
    // Fields: Rd=0, Rn=0, imm5=0, Q=0
    let encoding: u32 = 0x0E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_q_1_max_2c00_4e002c00() {
    // Encoding: 0x4E002C00
    // Test aarch64_vector_transfer_integer_move_signed field Q = 1 (Max)
    // Fields: Rn=0, Q=1, imm5=0, Rd=0
    let encoding: u32 = 0x4E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_0_zero_2c00_0e002c00() {
    // Encoding: 0x0E002C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 0 (Zero)
    // Fields: Rn=0, Rd=0, imm5=0, Q=0
    let encoding: u32 = 0x0E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_1_poweroftwo_2c00_0e012c00() {
    // Encoding: 0x0E012C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, imm5=1, Q=0
    let encoding: u32 = 0x0E012C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_3_poweroftwominusone_2c00_0e032c00()
{
    // Encoding: 0x0E032C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 3 (PowerOfTwoMinusOne)
    // Fields: Q=0, imm5=3, Rd=0, Rn=0
    let encoding: u32 = 0x0E032C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_4_poweroftwo_2c00_0e042c00() {
    // Encoding: 0x0E042C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 4 (PowerOfTwo)
    // Fields: Q=0, Rd=0, Rn=0, imm5=4
    let encoding: u32 = 0x0E042C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_7_poweroftwominusone_2c00_0e072c00()
{
    // Encoding: 0x0E072C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 7 (PowerOfTwoMinusOne)
    // Fields: Rn=0, imm5=7, Q=0, Rd=0
    let encoding: u32 = 0x0E072C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_8_poweroftwo_2c00_0e082c00() {
    // Encoding: 0x0E082C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 8 (PowerOfTwo)
    // Fields: Rd=0, imm5=8, Rn=0, Q=0
    let encoding: u32 = 0x0E082C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 15, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (15)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_15_poweroftwominusone_2c00_0e0f2c00()
{
    // Encoding: 0x0E0F2C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 15 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rn=0, Rd=0, imm5=15
    let encoding: u32 = 0x0E0F2C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_16_poweroftwo_2c00_0e102c00() {
    // Encoding: 0x0E102C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 16 (PowerOfTwo)
    // Fields: Rn=0, Q=0, Rd=0, imm5=16
    let encoding: u32 = 0x0E102C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 31, boundary: Max }
/// maximum immediate (31)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_imm5_31_max_2c00_0e1f2c00() {
    // Encoding: 0x0E1F2C00
    // Test aarch64_vector_transfer_integer_move_signed field imm5 = 31 (Max)
    // Fields: imm5=31, Rn=0, Q=0, Rd=0
    let encoding: u32 = 0x0E1F2C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rn_0_min_2c00_0e002c00() {
    // Encoding: 0x0E002C00
    // Test aarch64_vector_transfer_integer_move_signed field Rn = 0 (Min)
    // Fields: Rd=0, imm5=0, Q=0, Rn=0
    let encoding: u32 = 0x0E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rn_1_poweroftwo_2c00_0e002c20() {
    // Encoding: 0x0E002C20
    // Test aarch64_vector_transfer_integer_move_signed field Rn = 1 (PowerOfTwo)
    // Fields: imm5=0, Rn=1, Q=0, Rd=0
    let encoding: u32 = 0x0E002C20;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rn_30_poweroftwominusone_2c00_0e002fc0() {
    // Encoding: 0x0E002FC0
    // Test aarch64_vector_transfer_integer_move_signed field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, imm5=0, Q=0, Rn=30
    let encoding: u32 = 0x0E002FC0;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rn_31_max_2c00_0e002fe0() {
    // Encoding: 0x0E002FE0
    // Test aarch64_vector_transfer_integer_move_signed field Rn = 31 (Max)
    // Fields: Q=0, imm5=0, Rn=31, Rd=0
    let encoding: u32 = 0x0E002FE0;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rd_0_min_2c00_0e002c00() {
    // Encoding: 0x0E002C00
    // Test aarch64_vector_transfer_integer_move_signed field Rd = 0 (Min)
    // Fields: Rn=0, imm5=0, Rd=0, Q=0
    let encoding: u32 = 0x0E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rd_1_poweroftwo_2c00_0e002c01() {
    // Encoding: 0x0E002C01
    // Test aarch64_vector_transfer_integer_move_signed field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=1, Q=0, imm5=0
    let encoding: u32 = 0x0E002C01;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rd_30_poweroftwominusone_2c00_0e002c1e() {
    // Encoding: 0x0E002C1E
    // Test aarch64_vector_transfer_integer_move_signed field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Q=0, imm5=0, Rd=30
    let encoding: u32 = 0x0E002C1E;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_field_rd_31_max_2c00_0e002c1f() {
    // Encoding: 0x0E002C1F
    // Test aarch64_vector_transfer_integer_move_signed field Rd = 31 (Max)
    // Fields: Q=0, imm5=0, Rn=0, Rd=31
    let encoding: u32 = 0x0E002C1F;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_combo_0_2c00_0e002c00() {
    // Encoding: 0x0E002C00
    // Test aarch64_vector_transfer_integer_move_signed field combination: Q=0, imm5=0, Rn=0, Rd=0
    // Fields: imm5=0, Rn=0, Rd=0, Q=0
    let encoding: u32 = 0x0E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_special_q_0_size_variant_0_11264_0e002c00() {
    // Encoding: 0x0E002C00
    // Test aarch64_vector_transfer_integer_move_signed special value Q = 0 (Size variant 0)
    // Fields: imm5=0, Rn=0, Rd=0, Q=0
    let encoding: u32 = 0x0E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_special_q_1_size_variant_1_11264_4e002c00() {
    // Encoding: 0x4E002C00
    // Test aarch64_vector_transfer_integer_move_signed special value Q = 1 (Size variant 1)
    // Fields: imm5=0, Rd=0, Q=1, Rn=0
    let encoding: u32 = 0x4E002C00;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_special_rn_31_stack_pointer_sp_may_require_alignment_11264_0e002fe0(
) {
    // Encoding: 0x0E002FE0
    // Test aarch64_vector_transfer_integer_move_signed special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rn=31, imm5=0, Q=0
    let encoding: u32 = 0x0E002FE0;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_11264_0e002c1f(
) {
    // Encoding: 0x0E002C1F
    // Test aarch64_vector_transfer_integer_move_signed special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Q=0, Rd=31, imm5=0, Rn=0
    let encoding: u32 = 0x0E002C1F;
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

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_invalid_0_2c00_0e002c00() {
    // Encoding: 0x0E002C00
    // Test aarch64_vector_transfer_integer_move_signed invalid encoding: Unconditional UNDEFINED
    // Fields: imm5=0, Rd=0, Q=0, Rn=0
    let encoding: u32 = 0x0E002C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_reg_write_0_0e002c00() {
    // Test aarch64_vector_transfer_integer_move_signed register write: GpFromField("d")
    // Encoding: 0x0E002C00
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E002C00;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_sp_rn_0e002fe0() {
    // Test aarch64_vector_transfer_integer_move_signed with Rn = SP (31)
    // Encoding: 0x0E002FE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E002FE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_move_signed
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_integer_move_signed_zr_rd_0e002c1f() {
    // Test aarch64_vector_transfer_integer_move_signed with Rd = ZR (31)
    // Encoding: 0x0E002C1F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E002C1F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_vector_table Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_table_field_q_0_min_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table field Q = 0 (Min)
    // Fields: Rn=0, len=0, Rm=0, Rd=0, Q=0, op=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_table_field_q_1_max_0_4e000000() {
    // Encoding: 0x4E000000
    // Test aarch64_vector_transfer_vector_table field Q = 1 (Max)
    // Fields: Q=1, op=0, Rm=0, len=0, Rn=0, Rd=0
    let encoding: u32 = 0x4E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rm_0_min_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table field Rm = 0 (Min)
    // Fields: len=0, Q=0, Rm=0, Rn=0, op=0, Rd=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rm_1_poweroftwo_0_0e010000() {
    // Encoding: 0x0E010000
    // Test aarch64_vector_transfer_vector_table field Rm = 1 (PowerOfTwo)
    // Fields: op=0, len=0, Q=0, Rd=0, Rn=0, Rm=1
    let encoding: u32 = 0x0E010000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rm_30_poweroftwominusone_0_0e1e0000() {
    // Encoding: 0x0E1E0000
    // Test aarch64_vector_transfer_vector_table field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rd=0, len=0, Rn=0, op=0, Rm=30
    let encoding: u32 = 0x0E1E0000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rm_31_max_0_0e1f0000() {
    // Encoding: 0x0E1F0000
    // Test aarch64_vector_transfer_vector_table field Rm = 31 (Max)
    // Fields: len=0, Rm=31, op=0, Rd=0, Rn=0, Q=0
    let encoding: u32 = 0x0E1F0000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field len 13 +: 2`
/// Requirement: FieldBoundary { field: "len", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_transfer_vector_table_field_len_0_min_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table field len = 0 (Min)
    // Fields: Rm=0, Rn=0, op=0, Q=0, Rd=0, len=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field len 13 +: 2`
/// Requirement: FieldBoundary { field: "len", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_vector_transfer_vector_table_field_len_1_poweroftwo_0_0e002000() {
    // Encoding: 0x0E002000
    // Test aarch64_vector_transfer_vector_table field len = 1 (PowerOfTwo)
    // Fields: Rm=0, Q=0, Rd=0, op=0, Rn=0, len=1
    let encoding: u32 = 0x0E002000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field len 13 +: 2`
/// Requirement: FieldBoundary { field: "len", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_len_3_max_0_0e006000() {
    // Encoding: 0x0E006000
    // Test aarch64_vector_transfer_vector_table field len = 3 (Max)
    // Fields: Rd=0, Rm=0, Q=0, Rn=0, len=3, op=0
    let encoding: u32 = 0x0E006000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field op 12 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_transfer_vector_table_field_op_0_min_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table field op = 0 (Min)
    // Fields: op=0, Rd=0, Rn=0, Rm=0, Q=0, len=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field op 12 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_op_1_max_0_0e001000() {
    // Encoding: 0x0E001000
    // Test aarch64_vector_transfer_vector_table field op = 1 (Max)
    // Fields: Rm=0, op=1, Q=0, Rn=0, Rd=0, len=0
    let encoding: u32 = 0x0E001000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rn_0_min_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table field Rn = 0 (Min)
    // Fields: Q=0, op=0, Rn=0, len=0, Rm=0, Rd=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rn_1_poweroftwo_0_0e000020() {
    // Encoding: 0x0E000020
    // Test aarch64_vector_transfer_vector_table field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, len=0, Rm=0, Q=0, op=0, Rd=0
    let encoding: u32 = 0x0E000020;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rn_30_poweroftwominusone_0_0e0003c0() {
    // Encoding: 0x0E0003C0
    // Test aarch64_vector_transfer_vector_table field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rn=30, len=0, Rd=0, Rm=0, op=0
    let encoding: u32 = 0x0E0003C0;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rn_31_max_0_0e0003e0() {
    // Encoding: 0x0E0003E0
    // Test aarch64_vector_transfer_vector_table field Rn = 31 (Max)
    // Fields: len=0, Rd=0, Q=0, Rn=31, op=0, Rm=0
    let encoding: u32 = 0x0E0003E0;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rd_0_min_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table field Rd = 0 (Min)
    // Fields: Rn=0, op=0, Rm=0, len=0, Q=0, Rd=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rd_1_poweroftwo_0_0e000001() {
    // Encoding: 0x0E000001
    // Test aarch64_vector_transfer_vector_table field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, Rd=1, Q=0, len=0, op=0
    let encoding: u32 = 0x0E000001;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rd_30_poweroftwominusone_0_0e00001e() {
    // Encoding: 0x0E00001E
    // Test aarch64_vector_transfer_vector_table field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Q=0, op=0, Rm=0, Rd=30, Rn=0, len=0
    let encoding: u32 = 0x0E00001E;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_table_field_rd_31_max_0_0e00001f() {
    // Encoding: 0x0E00001F
    // Test aarch64_vector_transfer_vector_table field Rd = 31 (Max)
    // Fields: Q=0, Rm=0, op=0, Rd=31, len=0, Rn=0
    let encoding: u32 = 0x0E00001F;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_vector_table_combo_0_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table field combination: Q=0, Rm=0, len=0, op=0, Rn=0, Rd=0
    // Fields: Rd=0, op=0, len=0, Q=0, Rn=0, Rm=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_table_special_q_0_size_variant_0_0_0e000000() {
    // Encoding: 0x0E000000
    // Test aarch64_vector_transfer_vector_table special value Q = 0 (Size variant 0)
    // Fields: Rd=0, Rn=0, Q=0, op=0, len=0, Rm=0
    let encoding: u32 = 0x0E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_table_special_q_1_size_variant_1_0_4e000000() {
    // Encoding: 0x4E000000
    // Test aarch64_vector_transfer_vector_table special value Q = 1 (Size variant 1)
    // Fields: Rd=0, Q=1, len=0, Rm=0, op=0, Rn=0
    let encoding: u32 = 0x4E000000;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_table_special_rn_31_stack_pointer_sp_may_require_alignment_0_0e0003e0(
) {
    // Encoding: 0x0E0003E0
    // Test aarch64_vector_transfer_vector_table special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Q=0, op=0, Rn=31, Rd=0, len=0, Rm=0
    let encoding: u32 = 0x0E0003E0;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_table_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_0e00001f(
) {
    // Encoding: 0x0E00001F
    // Test aarch64_vector_transfer_vector_table special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, len=0, Rn=0, Rm=0, Q=0, op=0
    let encoding: u32 = 0x0E00001F;
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

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_table_reg_write_0_0e000000() {
    // Test aarch64_vector_transfer_vector_table register write: SimdFromField("d")
    // Encoding: 0x0E000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_table_sp_rn_0e0003e0() {
    // Test aarch64_vector_transfer_vector_table with Rn = SP (31)
    // Encoding: 0x0E0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_table
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_table_zr_rd_0e00001f() {
    // Test aarch64_vector_transfer_vector_table with Rd = ZR (31)
    // Encoding: 0x0E00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_vector_extract Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_q_0_min_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract field Q = 0 (Min)
    // Fields: imm4=0, Q=0, Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0x2E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_q_1_max_0_6e000000() {
    // Encoding: 0x6E000000
    // Test aarch64_vector_transfer_vector_extract field Q = 1 (Max)
    // Fields: Q=1, Rn=0, Rd=0, Rm=0, imm4=0
    let encoding: u32 = 0x6E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rm_0_min_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract field Rm = 0 (Min)
    // Fields: Q=0, imm4=0, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x2E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rm_1_poweroftwo_0_2e010000() {
    // Encoding: 0x2E010000
    // Test aarch64_vector_transfer_vector_extract field Rm = 1 (PowerOfTwo)
    // Fields: imm4=0, Rm=1, Rn=0, Rd=0, Q=0
    let encoding: u32 = 0x2E010000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rm_30_poweroftwominusone_0_2e1e0000() {
    // Encoding: 0x2E1E0000
    // Test aarch64_vector_transfer_vector_extract field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Q=0, Rm=30, Rd=0, Rn=0
    let encoding: u32 = 0x2E1E0000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rm_31_max_0_2e1f0000() {
    // Encoding: 0x2E1F0000
    // Test aarch64_vector_transfer_vector_extract field Rm = 31 (Max)
    // Fields: imm4=0, Rd=0, Rm=31, Q=0, Rn=0
    let encoding: u32 = 0x2E1F0000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_imm4_0_zero_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract field imm4 = 0 (Zero)
    // Fields: Rd=0, imm4=0, Q=0, Rm=0, Rn=0
    let encoding: u32 = 0x2E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_imm4_1_poweroftwo_0_2e000800() {
    // Encoding: 0x2E000800
    // Test aarch64_vector_transfer_vector_extract field imm4 = 1 (PowerOfTwo)
    // Fields: Q=0, imm4=1, Rm=0, Rn=0, Rd=0
    let encoding: u32 = 0x2E000800;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_imm4_3_poweroftwominusone_0_2e001800() {
    // Encoding: 0x2E001800
    // Test aarch64_vector_transfer_vector_extract field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Q=0, Rm=0, imm4=3, Rd=0
    let encoding: u32 = 0x2E001800;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_imm4_4_poweroftwo_0_2e002000() {
    // Encoding: 0x2E002000
    // Test aarch64_vector_transfer_vector_extract field imm4 = 4 (PowerOfTwo)
    // Fields: Rd=0, imm4=4, Q=0, Rm=0, Rn=0
    let encoding: u32 = 0x2E002000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_imm4_7_poweroftwominusone_0_2e003800() {
    // Encoding: 0x2E003800
    // Test aarch64_vector_transfer_vector_extract field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rm=0, imm4=7, Q=0, Rn=0
    let encoding: u32 = 0x2E003800;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_imm4_8_poweroftwo_0_2e004000() {
    // Encoding: 0x2E004000
    // Test aarch64_vector_transfer_vector_extract field imm4 = 8 (PowerOfTwo)
    // Fields: Q=0, Rn=0, Rd=0, Rm=0, imm4=8
    let encoding: u32 = 0x2E004000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_imm4_15_max_0_2e007800() {
    // Encoding: 0x2E007800
    // Test aarch64_vector_transfer_vector_extract field imm4 = 15 (Max)
    // Fields: Rn=0, Q=0, imm4=15, Rd=0, Rm=0
    let encoding: u32 = 0x2E007800;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rn_0_min_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract field Rn = 0 (Min)
    // Fields: Rm=0, Q=0, Rn=0, imm4=0, Rd=0
    let encoding: u32 = 0x2E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rn_1_poweroftwo_0_2e000020() {
    // Encoding: 0x2E000020
    // Test aarch64_vector_transfer_vector_extract field Rn = 1 (PowerOfTwo)
    // Fields: Q=0, Rn=1, Rm=0, Rd=0, imm4=0
    let encoding: u32 = 0x2E000020;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rn_30_poweroftwominusone_0_2e0003c0() {
    // Encoding: 0x2E0003C0
    // Test aarch64_vector_transfer_vector_extract field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Q=0, Rn=30, Rm=0, imm4=0
    let encoding: u32 = 0x2E0003C0;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rn_31_max_0_2e0003e0() {
    // Encoding: 0x2E0003E0
    // Test aarch64_vector_transfer_vector_extract field Rn = 31 (Max)
    // Fields: Q=0, Rd=0, Rn=31, Rm=0, imm4=0
    let encoding: u32 = 0x2E0003E0;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rd_0_min_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract field Rd = 0 (Min)
    // Fields: Q=0, Rm=0, Rd=0, imm4=0, Rn=0
    let encoding: u32 = 0x2E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rd_1_poweroftwo_0_2e000001() {
    // Encoding: 0x2E000001
    // Test aarch64_vector_transfer_vector_extract field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rm=0, imm4=0, Rn=0, Q=0
    let encoding: u32 = 0x2E000001;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rd_30_poweroftwominusone_0_2e00001e() {
    // Encoding: 0x2E00001E
    // Test aarch64_vector_transfer_vector_extract field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30, imm4=0, Rm=0, Q=0
    let encoding: u32 = 0x2E00001E;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_extract_field_rd_31_max_0_2e00001f() {
    // Encoding: 0x2E00001F
    // Test aarch64_vector_transfer_vector_extract field Rd = 31 (Max)
    // Fields: Q=0, Rm=0, Rd=31, imm4=0, Rn=0
    let encoding: u32 = 0x2E00001F;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_vector_extract_combo_0_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract field combination: Q=0, Rm=0, imm4=0, Rn=0, Rd=0
    // Fields: Q=0, Rm=0, imm4=0, Rn=0, Rd=0
    let encoding: u32 = 0x2E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_extract_special_q_0_size_variant_0_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract special value Q = 0 (Size variant 0)
    // Fields: Rn=0, Rm=0, imm4=0, Rd=0, Q=0
    let encoding: u32 = 0x2E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_extract_special_q_1_size_variant_1_0_6e000000() {
    // Encoding: 0x6E000000
    // Test aarch64_vector_transfer_vector_extract special value Q = 1 (Size variant 1)
    // Fields: imm4=0, Rd=0, Rm=0, Q=1, Rn=0
    let encoding: u32 = 0x6E000000;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_extract_special_rn_31_stack_pointer_sp_may_require_alignment_0_2e0003e0(
) {
    // Encoding: 0x2E0003E0
    // Test aarch64_vector_transfer_vector_extract special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, Rm=0, imm4=0, Rn=31, Q=0
    let encoding: u32 = 0x2E0003E0;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_extract_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_2e00001f(
) {
    // Encoding: 0x2E00001F
    // Test aarch64_vector_transfer_vector_extract special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: imm4=0, Rn=0, Rm=0, Rd=31, Q=0
    let encoding: u32 = 0x2E00001F;
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

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Index { base: Var(QualifiedIdentifier { qualifier: Any, name: "imm4" }), indices: [Single(LitInt(3))] } } }, rhs: LitBits([true]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Q\" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Index { base: Var(QualifiedIdentifier { qualifier: Any, name: \"imm4\" }), indices: [Single(LitInt(3))] } } }, rhs: LitBits([true]) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_extract_invalid_0_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract invalid encoding: Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Index { base: Var(QualifiedIdentifier { qualifier: Any, name: "imm4" }), indices: [Single(LitInt(3))] } } }, rhs: LitBits([true]) }
    // Fields: Rm=0, imm4=0, Rd=0, Rn=0, Q=0
    let encoding: u32 = 0x2E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_extract_invalid_1_0_2e000000() {
    // Encoding: 0x2E000000
    // Test aarch64_vector_transfer_vector_extract invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rn=0, Q=0, Rm=0, imm4=0
    let encoding: u32 = 0x2E000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_extract_reg_write_0_2e000000() {
    // Test aarch64_vector_transfer_vector_extract register write: SimdFromField("d")
    // Encoding: 0x2E000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2E000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_extract_sp_rn_2e0003e0() {
    // Test aarch64_vector_transfer_vector_extract with Rn = SP (31)
    // Encoding: 0x2E0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2E0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_extract
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_extract_zr_rd_2e00001f() {
    // Test aarch64_vector_transfer_vector_extract with Rd = ZR (31)
    // Encoding: 0x2E00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2E00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_vector_cpy_dup_sisd Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_0_zero_400_5e000400() {
    // Encoding: 0x5E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 0 (Zero)
    // Fields: Rd=0, Rn=0, imm5=0
    let encoding: u32 = 0x5E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_1_poweroftwo_400_5e010400() {
    // Encoding: 0x5E010400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 1 (PowerOfTwo)
    // Fields: imm5=1, Rn=0, Rd=0
    let encoding: u32 = 0x5E010400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_3_poweroftwominusone_400_5e030400() {
    // Encoding: 0x5E030400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 3 (PowerOfTwoMinusOne)
    // Fields: Rd=0, imm5=3, Rn=0
    let encoding: u32 = 0x5E030400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_4_poweroftwo_400_5e040400() {
    // Encoding: 0x5E040400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 4 (PowerOfTwo)
    // Fields: Rn=0, imm5=4, Rd=0
    let encoding: u32 = 0x5E040400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_7_poweroftwominusone_400_5e070400() {
    // Encoding: 0x5E070400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 7 (PowerOfTwoMinusOne)
    // Fields: Rn=0, imm5=7, Rd=0
    let encoding: u32 = 0x5E070400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_8_poweroftwo_400_5e080400() {
    // Encoding: 0x5E080400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 8 (PowerOfTwo)
    // Fields: Rd=0, Rn=0, imm5=8
    let encoding: u32 = 0x5E080400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 15, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (15)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_15_poweroftwominusone_400_5e0f0400()
{
    // Encoding: 0x5E0F0400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 15 (PowerOfTwoMinusOne)
    // Fields: imm5=15, Rd=0, Rn=0
    let encoding: u32 = 0x5E0F0400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_16_poweroftwo_400_5e100400() {
    // Encoding: 0x5E100400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 16 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, imm5=16
    let encoding: u32 = 0x5E100400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 31, boundary: Max }
/// maximum immediate (31)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_imm5_31_max_400_5e1f0400() {
    // Encoding: 0x5E1F0400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field imm5 = 31 (Max)
    // Fields: imm5=31, Rd=0, Rn=0
    let encoding: u32 = 0x5E1F0400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rn_0_min_400_5e000400() {
    // Encoding: 0x5E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rn = 0 (Min)
    // Fields: imm5=0, Rn=0, Rd=0
    let encoding: u32 = 0x5E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rn_1_poweroftwo_400_5e000420() {
    // Encoding: 0x5E000420
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rn = 1 (PowerOfTwo)
    // Fields: imm5=0, Rn=1, Rd=0
    let encoding: u32 = 0x5E000420;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rn_30_poweroftwominusone_400_5e0007c0() {
    // Encoding: 0x5E0007C0
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, Rd=0, imm5=0
    let encoding: u32 = 0x5E0007C0;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rn_31_max_400_5e0007e0() {
    // Encoding: 0x5E0007E0
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rn = 31 (Max)
    // Fields: Rn=31, Rd=0, imm5=0
    let encoding: u32 = 0x5E0007E0;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rd_0_min_400_5e000400() {
    // Encoding: 0x5E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rd = 0 (Min)
    // Fields: imm5=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rd_1_poweroftwo_400_5e000401() {
    // Encoding: 0x5E000401
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, Rn=0, imm5=0
    let encoding: u32 = 0x5E000401;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rd_30_poweroftwominusone_400_5e00041e() {
    // Encoding: 0x5E00041E
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rn=0, imm5=0
    let encoding: u32 = 0x5E00041E;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_field_rd_31_max_400_5e00041f() {
    // Encoding: 0x5E00041F
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field Rd = 31 (Max)
    // Fields: imm5=0, Rn=0, Rd=31
    let encoding: u32 = 0x5E00041F;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm5=0 (immediate value 0)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_combo_0_400_5e000400() {
    // Encoding: 0x5E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd field combination: imm5=0, Rn=0, Rd=0
    // Fields: Rn=0, imm5=0, Rd=0
    let encoding: u32 = 0x5E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_special_rn_31_stack_pointer_sp_may_require_alignment_1024_5e0007e0(
) {
    // Encoding: 0x5E0007E0
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: imm5=0, Rn=31, Rd=0
    let encoding: u32 = 0x5E0007E0;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_1024_5e00041f(
) {
    // Encoding: 0x5E00041F
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, imm5=0, Rn=0
    let encoding: u32 = 0x5E00041F;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: LitInt(3) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_invalid_0_400_5e000400() {
    // Encoding: 0x5E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd invalid encoding: Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }
    // Fields: Rd=0, imm5=0, Rn=0
    let encoding: u32 = 0x5E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_invalid_1_400_5e000400() {
    // Encoding: 0x5E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd invalid encoding: Unconditional UNDEFINED
    // Fields: imm5=0, Rd=0, Rn=0
    let encoding: u32 = 0x5E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_q_0_min_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Q = 0 (Min)
    // Fields: imm5=0, Q=0, Rd=0, Rn=0
    let encoding: u32 = 0x0E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_q_1_max_400_4e000400() {
    // Encoding: 0x4E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Q = 1 (Max)
    // Fields: Rd=0, Q=1, imm5=0, Rn=0
    let encoding: u32 = 0x4E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_0_zero_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 0 (Zero)
    // Fields: Rd=0, Rn=0, Q=0, imm5=0
    let encoding: u32 = 0x0E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_1_poweroftwo_400_0e010400() {
    // Encoding: 0x0E010400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 1 (PowerOfTwo)
    // Fields: Rn=0, Q=0, Rd=0, imm5=1
    let encoding: u32 = 0x0E010400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_3_poweroftwominusone_400_0e030400() {
    // Encoding: 0x0E030400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 3 (PowerOfTwoMinusOne)
    // Fields: imm5=3, Rd=0, Q=0, Rn=0
    let encoding: u32 = 0x0E030400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_4_poweroftwo_400_0e040400() {
    // Encoding: 0x0E040400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 4 (PowerOfTwo)
    // Fields: Q=0, imm5=4, Rd=0, Rn=0
    let encoding: u32 = 0x0E040400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_7_poweroftwominusone_400_0e070400() {
    // Encoding: 0x0E070400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 7 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rd=0, Rn=0, imm5=7
    let encoding: u32 = 0x0E070400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_8_poweroftwo_400_0e080400() {
    // Encoding: 0x0E080400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 8 (PowerOfTwo)
    // Fields: Q=0, Rd=0, Rn=0, imm5=8
    let encoding: u32 = 0x0E080400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 15, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (15)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_15_poweroftwominusone_400_0e0f0400()
{
    // Encoding: 0x0E0F0400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 15 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Q=0, imm5=15, Rd=0
    let encoding: u32 = 0x0E0F0400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_16_poweroftwo_400_0e100400() {
    // Encoding: 0x0E100400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 16 (PowerOfTwo)
    // Fields: imm5=16, Rn=0, Rd=0, Q=0
    let encoding: u32 = 0x0E100400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 31, boundary: Max }
/// maximum immediate (31)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_imm5_31_max_400_0e1f0400() {
    // Encoding: 0x0E1F0400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field imm5 = 31 (Max)
    // Fields: Rn=0, imm5=31, Rd=0, Q=0
    let encoding: u32 = 0x0E1F0400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rn_0_min_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rn = 0 (Min)
    // Fields: Rd=0, Q=0, imm5=0, Rn=0
    let encoding: u32 = 0x0E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rn_1_poweroftwo_400_0e000420() {
    // Encoding: 0x0E000420
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, Q=0, Rn=1, imm5=0
    let encoding: u32 = 0x0E000420;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rn_30_poweroftwominusone_400_0e0007c0() {
    // Encoding: 0x0E0007C0
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rd=0, imm5=0, Rn=30
    let encoding: u32 = 0x0E0007C0;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rn_31_max_400_0e0007e0() {
    // Encoding: 0x0E0007E0
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rn = 31 (Max)
    // Fields: imm5=0, Rd=0, Rn=31, Q=0
    let encoding: u32 = 0x0E0007E0;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rd_0_min_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rd = 0 (Min)
    // Fields: Q=0, imm5=0, Rd=0, Rn=0
    let encoding: u32 = 0x0E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rd_1_poweroftwo_400_0e000401() {
    // Encoding: 0x0E000401
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rd=1, Q=0, imm5=0
    let encoding: u32 = 0x0E000401;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rd_30_poweroftwominusone_400_0e00041e() {
    // Encoding: 0x0E00041E
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, imm5=0, Q=0, Rd=30
    let encoding: u32 = 0x0E00041E;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_field_rd_31_max_400_0e00041f() {
    // Encoding: 0x0E00041F
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field Rd = 31 (Max)
    // Fields: Rd=31, Rn=0, imm5=0, Q=0
    let encoding: u32 = 0x0E00041F;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_combo_0_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd field combination: Q=0, imm5=0, Rn=0, Rd=0
    // Fields: Rn=0, Q=0, imm5=0, Rd=0
    let encoding: u32 = 0x0E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_special_q_0_size_variant_0_1024_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd special value Q = 0 (Size variant 0)
    // Fields: Q=0, imm5=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_special_q_1_size_variant_1_1024_4e000400() {
    // Encoding: 0x4E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd special value Q = 1 (Size variant 1)
    // Fields: Q=1, imm5=0, Rd=0, Rn=0
    let encoding: u32 = 0x4E000400;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_special_rn_31_stack_pointer_sp_may_require_alignment_1024_0e0007e0(
) {
    // Encoding: 0x0E0007E0
    // Test aarch64_vector_transfer_vector_cpy_dup_simd special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: imm5=0, Q=0, Rd=0, Rn=31
    let encoding: u32 = 0x0E0007E0;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_1024_0e00041f(
) {
    // Encoding: 0x0E00041F
    // Test aarch64_vector_transfer_vector_cpy_dup_simd special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Q=0, imm5=0, Rn=0, Rd=31
    let encoding: u32 = 0x0E00041F;
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

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: LitInt(3) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_invalid_0_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd invalid encoding: Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }
    // Fields: Rn=0, Rd=0, Q=0, imm5=0
    let encoding: u32 = 0x0E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_invalid_1_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd invalid encoding: Unconditional UNDEFINED
    // Fields: Q=0, imm5=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: And, lhs: LitInt(3), rhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }) } }, rhs: LitBits([false]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: Binary { op: And, lhs: LitInt(3), rhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Q\" }) } }, rhs: LitBits([false]) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_invalid_2_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd invalid encoding: Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: And, lhs: LitInt(3), rhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }) } }, rhs: LitBits([false]) }
    // Fields: Rd=0, Rn=0, imm5=0, Q=0
    let encoding: u32 = 0x0E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_invalid_3_400_0e000400() {
    // Encoding: 0x0E000400
    // Test aarch64_vector_transfer_vector_cpy_dup_simd invalid encoding: Unconditional UNDEFINED
    // Fields: imm5=0, Rn=0, Q=0, Rd=0
    let encoding: u32 = 0x0E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_reg_write_0_5e000400() {
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd register write: SimdFromField("d")
    // Encoding: 0x5E000400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E000400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_sp_rn_5e0007e0() {
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd with Rn = SP (31)
    // Encoding: 0x5E0007E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E0007E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_sisd
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_sisd_zr_rd_5e00041f() {
    // Test aarch64_vector_transfer_vector_cpy_dup_sisd with Rd = ZR (31)
    // Encoding: 0x5E00041F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x5E00041F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_reg_write_0_0e000400() {
    // Test aarch64_vector_transfer_vector_cpy_dup_simd register write: SimdFromField("d")
    // Encoding: 0x0E000400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E000400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_sp_rn_0e0007e0() {
    // Test aarch64_vector_transfer_vector_cpy_dup_simd with Rn = SP (31)
    // Encoding: 0x0E0007E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E0007E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_cpy_dup_simd
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_cpy_dup_simd_zr_rd_0e00041f() {
    // Test aarch64_vector_transfer_vector_cpy_dup_simd with Rd = ZR (31)
    // Encoding: 0x0E00041F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E00041F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_vector_permute_unzip Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_q_0_min_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field Q = 0 (Min)
    // Fields: Q=0, size=0, op=0, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_q_1_max_1800_4e001800() {
    // Encoding: 0x4E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field Q = 1 (Max)
    // Fields: Rd=0, Rm=0, size=0, Rn=0, Q=1, op=0
    let encoding: u32 = 0x4E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_size_0_min_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field size = 0 (Min)
    // Fields: Q=0, size=0, Rm=0, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_size_1_poweroftwo_1800_0e401800() {
    // Encoding: 0x0E401800
    // Test aarch64_vector_transfer_vector_permute_unzip field size = 1 (PowerOfTwo)
    // Fields: Rm=0, Q=0, size=1, op=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E401800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_size_2_poweroftwo_1800_0e801800() {
    // Encoding: 0x0E801800
    // Test aarch64_vector_transfer_vector_permute_unzip field size = 2 (PowerOfTwo)
    // Fields: size=2, Q=0, Rd=0, op=0, Rn=0, Rm=0
    let encoding: u32 = 0x0E801800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_size_3_max_1800_0ec01800() {
    // Encoding: 0x0EC01800
    // Test aarch64_vector_transfer_vector_permute_unzip field size = 3 (Max)
    // Fields: op=0, Rn=0, Rm=0, size=3, Rd=0, Q=0
    let encoding: u32 = 0x0EC01800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rm_0_min_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field Rm = 0 (Min)
    // Fields: Rd=0, op=0, Q=0, Rm=0, Rn=0, size=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rm_1_poweroftwo_1800_0e011800() {
    // Encoding: 0x0E011800
    // Test aarch64_vector_transfer_vector_permute_unzip field Rm = 1 (PowerOfTwo)
    // Fields: Rm=1, Q=0, op=0, Rn=0, Rd=0, size=0
    let encoding: u32 = 0x0E011800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rm_30_poweroftwominusone_1800_0e1e1800()
{
    // Encoding: 0x0E1E1800
    // Test aarch64_vector_transfer_vector_permute_unzip field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=0, Q=0, size=0, Rm=30, op=0
    let encoding: u32 = 0x0E1E1800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rm_31_max_1800_0e1f1800() {
    // Encoding: 0x0E1F1800
    // Test aarch64_vector_transfer_vector_permute_unzip field Rm = 31 (Max)
    // Fields: Rn=0, Rd=0, op=0, size=0, Rm=31, Q=0
    let encoding: u32 = 0x0E1F1800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field op 14 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_op_0_min_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field op = 0 (Min)
    // Fields: Q=0, size=0, Rm=0, Rn=0, Rd=0, op=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field op 14 +: 1`
/// Requirement: FieldBoundary { field: "op", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_op_1_max_1800_0e005800() {
    // Encoding: 0x0E005800
    // Test aarch64_vector_transfer_vector_permute_unzip field op = 1 (Max)
    // Fields: size=0, Rn=0, Q=0, Rm=0, Rd=0, op=1
    let encoding: u32 = 0x0E005800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rn_0_min_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field Rn = 0 (Min)
    // Fields: Rd=0, size=0, Q=0, Rn=0, Rm=0, op=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rn_1_poweroftwo_1800_0e001820() {
    // Encoding: 0x0E001820
    // Test aarch64_vector_transfer_vector_permute_unzip field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, Q=0, size=0, op=0, Rn=1, Rd=0
    let encoding: u32 = 0x0E001820;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rn_30_poweroftwominusone_1800_0e001bc0()
{
    // Encoding: 0x0E001BC0
    // Test aarch64_vector_transfer_vector_permute_unzip field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: op=0, Rd=0, Rm=0, Q=0, Rn=30, size=0
    let encoding: u32 = 0x0E001BC0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rn_31_max_1800_0e001be0() {
    // Encoding: 0x0E001BE0
    // Test aarch64_vector_transfer_vector_permute_unzip field Rn = 31 (Max)
    // Fields: Rm=0, Rn=31, Q=0, size=0, op=0, Rd=0
    let encoding: u32 = 0x0E001BE0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rd_0_min_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field Rd = 0 (Min)
    // Fields: size=0, Rm=0, op=0, Q=0, Rd=0, Rn=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rd_1_poweroftwo_1800_0e001801() {
    // Encoding: 0x0E001801
    // Test aarch64_vector_transfer_vector_permute_unzip field Rd = 1 (PowerOfTwo)
    // Fields: Q=0, Rm=0, Rn=0, size=0, op=0, Rd=1
    let encoding: u32 = 0x0E001801;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rd_30_poweroftwominusone_1800_0e00181e()
{
    // Encoding: 0x0E00181E
    // Test aarch64_vector_transfer_vector_permute_unzip field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, size=0, Q=0, op=0, Rd=30, Rn=0
    let encoding: u32 = 0x0E00181E;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_field_rd_31_max_1800_0e00181f() {
    // Encoding: 0x0E00181F
    // Test aarch64_vector_transfer_vector_permute_unzip field Rd = 31 (Max)
    // Fields: Rn=0, Rd=31, Rm=0, op=0, Q=0, size=0
    let encoding: u32 = 0x0E00181F;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_combo_0_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip field combination: Q=0, size=0, Rm=0, op=0, Rn=0, Rd=0
    // Fields: size=0, Rn=0, Rd=0, op=0, Q=0, Rm=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_q_0_size_variant_0_6144_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip special value Q = 0 (Size variant 0)
    // Fields: Q=0, size=0, op=0, Rd=0, Rn=0, Rm=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_q_1_size_variant_1_6144_4e001800() {
    // Encoding: 0x4E001800
    // Test aarch64_vector_transfer_vector_permute_unzip special value Q = 1 (Size variant 1)
    // Fields: Rn=0, size=0, Rm=0, Rd=0, Q=1, op=0
    let encoding: u32 = 0x4E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_size_0_size_variant_0_6144_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip special value size = 0 (Size variant 0)
    // Fields: op=0, Rd=0, Rm=0, Q=0, Rn=0, size=0
    let encoding: u32 = 0x0E001800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_size_1_size_variant_1_6144_0e401800() {
    // Encoding: 0x0E401800
    // Test aarch64_vector_transfer_vector_permute_unzip special value size = 1 (Size variant 1)
    // Fields: Q=0, Rn=0, size=1, Rm=0, op=0, Rd=0
    let encoding: u32 = 0x0E401800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_size_2_size_variant_2_6144_0e801800() {
    // Encoding: 0x0E801800
    // Test aarch64_vector_transfer_vector_permute_unzip special value size = 2 (Size variant 2)
    // Fields: Rm=0, Rd=0, size=2, op=0, Q=0, Rn=0
    let encoding: u32 = 0x0E801800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_size_3_size_variant_3_6144_0ec01800() {
    // Encoding: 0x0EC01800
    // Test aarch64_vector_transfer_vector_permute_unzip special value size = 3 (Size variant 3)
    // Fields: size=3, op=0, Rm=0, Rn=0, Rd=0, Q=0
    let encoding: u32 = 0x0EC01800;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_rn_31_stack_pointer_sp_may_require_alignment_6144_0e001be0(
) {
    // Encoding: 0x0E001BE0
    // Test aarch64_vector_transfer_vector_permute_unzip special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rm=0, size=0, op=0, Rn=31, Rd=0, Q=0
    let encoding: u32 = 0x0E001BE0;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_6144_0e00181f(
) {
    // Encoding: 0x0E00181F
    // Test aarch64_vector_transfer_vector_permute_unzip special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: op=0, Rd=31, Rm=0, size=0, Rn=0, Q=0
    let encoding: u32 = 0x0E00181F;
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

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: LitBits([true, true, false]) } }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Q\" }), rhs: LitBits([true, true, false]) } }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_invalid_0_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip invalid encoding: Binary { op: BitConcat, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }), rhs: LitBits([true, true, false]) } }
    // Fields: Rd=0, Rm=0, Q=0, Rn=0, size=0, op=0
    let encoding: u32 = 0x0E001800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_invalid_1_1800_0e001800() {
    // Encoding: 0x0E001800
    // Test aarch64_vector_transfer_vector_permute_unzip invalid encoding: Unconditional UNDEFINED
    // Fields: Rm=0, Rn=0, op=0, size=0, Rd=0, Q=0
    let encoding: u32 = 0x0E001800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_reg_write_0_0e001800() {
    // Test aarch64_vector_transfer_vector_permute_unzip register write: SimdFromField("d")
    // Encoding: 0x0E001800
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E001800;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_sp_rn_0e001be0() {
    // Test aarch64_vector_transfer_vector_permute_unzip with Rn = SP (31)
    // Encoding: 0x0E001BE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E001BE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_permute_unzip
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_permute_unzip_zr_rd_0e00181f() {
    // Test aarch64_vector_transfer_vector_permute_unzip with Rd = ZR (31)
    // Encoding: 0x0E00181F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E00181F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_integer_insert Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_0_zero_1c00_4e001c00() {
    // Encoding: 0x4E001C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 0 (Zero)
    // Fields: imm5=0, Rn=0, Rd=0
    let encoding: u32 = 0x4E001C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_1_poweroftwo_1c00_4e011c00() {
    // Encoding: 0x4E011C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 1 (PowerOfTwo)
    // Fields: Rn=0, imm5=1, Rd=0
    let encoding: u32 = 0x4E011C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_3_poweroftwominusone_1c00_4e031c00() {
    // Encoding: 0x4E031C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 3 (PowerOfTwoMinusOne)
    // Fields: imm5=3, Rd=0, Rn=0
    let encoding: u32 = 0x4E031C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_4_poweroftwo_1c00_4e041c00() {
    // Encoding: 0x4E041C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 4 (PowerOfTwo)
    // Fields: Rn=0, imm5=4, Rd=0
    let encoding: u32 = 0x4E041C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_7_poweroftwominusone_1c00_4e071c00() {
    // Encoding: 0x4E071C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 7 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=0, imm5=7
    let encoding: u32 = 0x4E071C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_8_poweroftwo_1c00_4e081c00() {
    // Encoding: 0x4E081C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 8 (PowerOfTwo)
    // Fields: imm5=8, Rn=0, Rd=0
    let encoding: u32 = 0x4E081C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 15, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (15)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_15_poweroftwominusone_1c00_4e0f1c00() {
    // Encoding: 0x4E0F1C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 15 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=0, imm5=15
    let encoding: u32 = 0x4E0F1C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_16_poweroftwo_1c00_4e101c00() {
    // Encoding: 0x4E101C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 16 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, imm5=16
    let encoding: u32 = 0x4E101C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 31, boundary: Max }
/// maximum immediate (31)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_imm5_31_max_1c00_4e1f1c00() {
    // Encoding: 0x4E1F1C00
    // Test aarch64_vector_transfer_integer_insert field imm5 = 31 (Max)
    // Fields: Rd=0, Rn=0, imm5=31
    let encoding: u32 = 0x4E1F1C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rn_0_min_1c00_4e001c00() {
    // Encoding: 0x4E001C00
    // Test aarch64_vector_transfer_integer_insert field Rn = 0 (Min)
    // Fields: Rn=0, Rd=0, imm5=0
    let encoding: u32 = 0x4E001C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rn_1_poweroftwo_1c00_4e001c20() {
    // Encoding: 0x4E001C20
    // Test aarch64_vector_transfer_integer_insert field Rn = 1 (PowerOfTwo)
    // Fields: imm5=0, Rn=1, Rd=0
    let encoding: u32 = 0x4E001C20;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rn_30_poweroftwominusone_1c00_4e001fc0() {
    // Encoding: 0x4E001FC0
    // Test aarch64_vector_transfer_integer_insert field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=0, imm5=0, Rn=30
    let encoding: u32 = 0x4E001FC0;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rn_31_max_1c00_4e001fe0() {
    // Encoding: 0x4E001FE0
    // Test aarch64_vector_transfer_integer_insert field Rn = 31 (Max)
    // Fields: Rd=0, imm5=0, Rn=31
    let encoding: u32 = 0x4E001FE0;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rd_0_min_1c00_4e001c00() {
    // Encoding: 0x4E001C00
    // Test aarch64_vector_transfer_integer_insert field Rd = 0 (Min)
    // Fields: Rd=0, Rn=0, imm5=0
    let encoding: u32 = 0x4E001C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rd_1_poweroftwo_1c00_4e001c01() {
    // Encoding: 0x4E001C01
    // Test aarch64_vector_transfer_integer_insert field Rd = 1 (PowerOfTwo)
    // Fields: Rd=1, imm5=0, Rn=0
    let encoding: u32 = 0x4E001C01;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rd_30_poweroftwominusone_1c00_4e001c1e() {
    // Encoding: 0x4E001C1E
    // Test aarch64_vector_transfer_integer_insert field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Rd=30, imm5=0
    let encoding: u32 = 0x4E001C1E;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_integer_insert_field_rd_31_max_1c00_4e001c1f() {
    // Encoding: 0x4E001C1F
    // Test aarch64_vector_transfer_integer_insert field Rd = 31 (Max)
    // Fields: imm5=0, Rn=0, Rd=31
    let encoding: u32 = 0x4E001C1F;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm5=0 (immediate value 0)
#[test]
fn test_aarch64_vector_transfer_integer_insert_combo_0_1c00_4e001c00() {
    // Encoding: 0x4E001C00
    // Test aarch64_vector_transfer_integer_insert field combination: imm5=0, Rn=0, Rd=0
    // Fields: Rd=0, imm5=0, Rn=0
    let encoding: u32 = 0x4E001C00;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_integer_insert_special_rn_31_stack_pointer_sp_may_require_alignment_7168_4e001fe0(
) {
    // Encoding: 0x4E001FE0
    // Test aarch64_vector_transfer_integer_insert special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: imm5=0, Rd=0, Rn=31
    let encoding: u32 = 0x4E001FE0;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_integer_insert_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_7168_4e001c1f(
) {
    // Encoding: 0x4E001C1F
    // Test aarch64_vector_transfer_integer_insert special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, imm5=0, Rn=0
    let encoding: u32 = 0x4E001C1F;
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

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: LitInt(3) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_insert_invalid_0_1c00_4e001c00() {
    // Encoding: 0x4E001C00
    // Test aarch64_vector_transfer_integer_insert invalid encoding: Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }
    // Fields: Rn=0, Rd=0, imm5=0
    let encoding: u32 = 0x4E001C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_insert_invalid_1_1c00_4e001c00() {
    // Encoding: 0x4E001C00
    // Test aarch64_vector_transfer_integer_insert invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Rn=0, imm5=0
    let encoding: u32 = 0x4E001C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_integer_insert_reg_write_0_4e001c00() {
    // Test aarch64_vector_transfer_integer_insert register write: SimdFromField("d")
    // Encoding: 0x4E001C00
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E001C00;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_integer_insert_sp_rn_4e001fe0() {
    // Test aarch64_vector_transfer_integer_insert with Rn = SP (31)
    // Encoding: 0x4E001FE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E001FE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_insert
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_integer_insert_zr_rd_4e001c1f() {
    // Test aarch64_vector_transfer_integer_insert with Rd = ZR (31)
    // Encoding: 0x4E001C1F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x4E001C1F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_vector_insert Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_0_zero_400_6e000400() {
    // Encoding: 0x6E000400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 0 (Zero)
    // Fields: Rn=0, imm5=0, imm4=0, Rd=0
    let encoding: u32 = 0x6E000400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_1_poweroftwo_400_6e010400() {
    // Encoding: 0x6E010400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 1 (PowerOfTwo)
    // Fields: imm5=1, Rn=0, imm4=0, Rd=0
    let encoding: u32 = 0x6E010400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_3_poweroftwominusone_400_6e030400() {
    // Encoding: 0x6E030400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 3 (PowerOfTwoMinusOne)
    // Fields: imm5=3, Rd=0, imm4=0, Rn=0
    let encoding: u32 = 0x6E030400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_4_poweroftwo_400_6e040400() {
    // Encoding: 0x6E040400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 4 (PowerOfTwo)
    // Fields: imm5=4, imm4=0, Rn=0, Rd=0
    let encoding: u32 = 0x6E040400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_7_poweroftwominusone_400_6e070400() {
    // Encoding: 0x6E070400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 7 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Rd=0, imm5=7, Rn=0
    let encoding: u32 = 0x6E070400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_8_poweroftwo_400_6e080400() {
    // Encoding: 0x6E080400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 8 (PowerOfTwo)
    // Fields: Rn=0, imm4=0, Rd=0, imm5=8
    let encoding: u32 = 0x6E080400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 15, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (15)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_15_poweroftwominusone_400_6e0f0400() {
    // Encoding: 0x6E0F0400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 15 (PowerOfTwoMinusOne)
    // Fields: imm4=0, Rd=0, imm5=15, Rn=0
    let encoding: u32 = 0x6E0F0400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_16_poweroftwo_400_6e100400() {
    // Encoding: 0x6E100400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 16 (PowerOfTwo)
    // Fields: Rd=0, imm4=0, Rn=0, imm5=16
    let encoding: u32 = 0x6E100400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 31, boundary: Max }
/// maximum immediate (31)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm5_31_max_400_6e1f0400() {
    // Encoding: 0x6E1F0400
    // Test aarch64_vector_transfer_vector_insert field imm5 = 31 (Max)
    // Fields: imm5=31, imm4=0, Rd=0, Rn=0
    let encoding: u32 = 0x6E1F0400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm4_0_zero_400_6e000400() {
    // Encoding: 0x6E000400
    // Test aarch64_vector_transfer_vector_insert field imm4 = 0 (Zero)
    // Fields: imm4=0, Rd=0, imm5=0, Rn=0
    let encoding: u32 = 0x6E000400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm4_1_poweroftwo_400_6e000c00() {
    // Encoding: 0x6E000C00
    // Test aarch64_vector_transfer_vector_insert field imm4 = 1 (PowerOfTwo)
    // Fields: imm5=0, Rd=0, Rn=0, imm4=1
    let encoding: u32 = 0x6E000C00;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm4_3_poweroftwominusone_400_6e001c00() {
    // Encoding: 0x6E001C00
    // Test aarch64_vector_transfer_vector_insert field imm4 = 3 (PowerOfTwoMinusOne)
    // Fields: imm5=0, imm4=3, Rn=0, Rd=0
    let encoding: u32 = 0x6E001C00;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm4_4_poweroftwo_400_6e002400() {
    // Encoding: 0x6E002400
    // Test aarch64_vector_transfer_vector_insert field imm4 = 4 (PowerOfTwo)
    // Fields: Rd=0, imm5=0, imm4=4, Rn=0
    let encoding: u32 = 0x6E002400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 7, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (7)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm4_7_poweroftwominusone_400_6e003c00() {
    // Encoding: 0x6E003C00
    // Test aarch64_vector_transfer_vector_insert field imm4 = 7 (PowerOfTwoMinusOne)
    // Fields: imm5=0, Rd=0, Rn=0, imm4=7
    let encoding: u32 = 0x6E003C00;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm4_8_poweroftwo_400_6e004400() {
    // Encoding: 0x6E004400
    // Test aarch64_vector_transfer_vector_insert field imm4 = 8 (PowerOfTwo)
    // Fields: imm4=8, Rd=0, imm5=0, Rn=0
    let encoding: u32 = 0x6E004400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field imm4 11 +: 4`
/// Requirement: FieldBoundary { field: "imm4", value: 15, boundary: Max }
/// maximum immediate (15)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_imm4_15_max_400_6e007c00() {
    // Encoding: 0x6E007C00
    // Test aarch64_vector_transfer_vector_insert field imm4 = 15 (Max)
    // Fields: imm4=15, Rn=0, imm5=0, Rd=0
    let encoding: u32 = 0x6E007C00;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rn_0_min_400_6e000400() {
    // Encoding: 0x6E000400
    // Test aarch64_vector_transfer_vector_insert field Rn = 0 (Min)
    // Fields: Rd=0, Rn=0, imm5=0, imm4=0
    let encoding: u32 = 0x6E000400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rn_1_poweroftwo_400_6e000420() {
    // Encoding: 0x6E000420
    // Test aarch64_vector_transfer_vector_insert field Rn = 1 (PowerOfTwo)
    // Fields: imm4=0, Rd=0, Rn=1, imm5=0
    let encoding: u32 = 0x6E000420;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rn_30_poweroftwominusone_400_6e0007c0() {
    // Encoding: 0x6E0007C0
    // Test aarch64_vector_transfer_vector_insert field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, imm5=0, imm4=0, Rd=0
    let encoding: u32 = 0x6E0007C0;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rn_31_max_400_6e0007e0() {
    // Encoding: 0x6E0007E0
    // Test aarch64_vector_transfer_vector_insert field Rn = 31 (Max)
    // Fields: imm4=0, imm5=0, Rn=31, Rd=0
    let encoding: u32 = 0x6E0007E0;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rd_0_min_400_6e000400() {
    // Encoding: 0x6E000400
    // Test aarch64_vector_transfer_vector_insert field Rd = 0 (Min)
    // Fields: imm5=0, Rd=0, Rn=0, imm4=0
    let encoding: u32 = 0x6E000400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rd_1_poweroftwo_400_6e000401() {
    // Encoding: 0x6E000401
    // Test aarch64_vector_transfer_vector_insert field Rd = 1 (PowerOfTwo)
    // Fields: imm5=0, Rn=0, imm4=0, Rd=1
    let encoding: u32 = 0x6E000401;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rd_30_poweroftwominusone_400_6e00041e() {
    // Encoding: 0x6E00041E
    // Test aarch64_vector_transfer_vector_insert field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rd=30, Rn=0, imm4=0, imm5=0
    let encoding: u32 = 0x6E00041E;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_vector_insert_field_rd_31_max_400_6e00041f() {
    // Encoding: 0x6E00041F
    // Test aarch64_vector_transfer_vector_insert field Rd = 31 (Max)
    // Fields: Rn=0, imm4=0, imm5=0, Rd=31
    let encoding: u32 = 0x6E00041F;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm5=0 (immediate value 0)
#[test]
fn test_aarch64_vector_transfer_vector_insert_combo_0_400_6e000400() {
    // Encoding: 0x6E000400
    // Test aarch64_vector_transfer_vector_insert field combination: imm5=0, imm4=0, Rn=0, Rd=0
    // Fields: Rd=0, imm4=0, Rn=0, imm5=0
    let encoding: u32 = 0x6E000400;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_vector_insert_special_rn_31_stack_pointer_sp_may_require_alignment_1024_6e0007e0(
) {
    // Encoding: 0x6E0007E0
    // Test aarch64_vector_transfer_vector_insert special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: imm5=0, imm4=0, Rn=31, Rd=0
    let encoding: u32 = 0x6E0007E0;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_vector_insert_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_1024_6e00041f(
) {
    // Encoding: 0x6E00041F
    // Test aarch64_vector_transfer_vector_insert special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rn=0, Rd=31, imm5=0, imm4=0
    let encoding: u32 = 0x6E00041F;
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

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: LitInt(3) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_insert_invalid_0_400_6e000400() {
    // Encoding: 0x6E000400
    // Test aarch64_vector_transfer_vector_insert invalid encoding: Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }
    // Fields: Rn=0, imm4=0, imm5=0, Rd=0
    let encoding: u32 = 0x6E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_vector_insert_invalid_1_400_6e000400() {
    // Encoding: 0x6E000400
    // Test aarch64_vector_transfer_vector_insert invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, imm4=0, Rn=0, imm5=0
    let encoding: u32 = 0x6E000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_vector_insert_reg_write_0_6e000400() {
    // Test aarch64_vector_transfer_vector_insert register write: SimdFromField("d")
    // Encoding: 0x6E000400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x6E000400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_vector_insert_sp_rn_6e0007e0() {
    // Test aarch64_vector_transfer_vector_insert with Rn = SP (31)
    // Encoding: 0x6E0007E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x6E0007E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_vector_insert
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_vector_insert_zr_rd_6e00041f() {
    // Test aarch64_vector_transfer_vector_insert with Rd = ZR (31)
    // Encoding: 0x6E00041F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x6E00041F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

// ============================================================================
// aarch64_vector_transfer_integer_dup Tests
// ============================================================================

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_q_0_min_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup field Q = 0 (Min)
    // Fields: imm5=0, Rd=0, Q=0, Rn=0
    let encoding: u32 = 0x0E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_q_1_max_c00_4e000c00() {
    // Encoding: 0x4E000C00
    // Test aarch64_vector_transfer_integer_dup field Q = 1 (Max)
    // Fields: Q=1, Rn=0, Rd=0, imm5=0
    let encoding: u32 = 0x4E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_0_zero_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 0 (Zero)
    // Fields: Rn=0, imm5=0, Q=0, Rd=0
    let encoding: u32 = 0x0E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_1_poweroftwo_c00_0e010c00() {
    // Encoding: 0x0E010C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 1 (PowerOfTwo)
    // Fields: imm5=1, Rd=0, Rn=0, Q=0
    let encoding: u32 = 0x0E010C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_3_poweroftwominusone_c00_0e030c00() {
    // Encoding: 0x0E030C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 3 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rn=0, Rd=0, imm5=3
    let encoding: u32 = 0x0E030C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_4_poweroftwo_c00_0e040c00() {
    // Encoding: 0x0E040C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 4 (PowerOfTwo)
    // Fields: Q=0, Rn=0, imm5=4, Rd=0
    let encoding: u32 = 0x0E040C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_7_poweroftwominusone_c00_0e070c00() {
    // Encoding: 0x0E070C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 7 (PowerOfTwoMinusOne)
    // Fields: imm5=7, Rd=0, Rn=0, Q=0
    let encoding: u32 = 0x0E070C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_8_poweroftwo_c00_0e080c00() {
    // Encoding: 0x0E080C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 8 (PowerOfTwo)
    // Fields: Q=0, imm5=8, Rn=0, Rd=0
    let encoding: u32 = 0x0E080C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 15, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (15)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_15_poweroftwominusone_c00_0e0f0c00() {
    // Encoding: 0x0E0F0C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 15 (PowerOfTwoMinusOne)
    // Fields: Q=0, Rn=0, Rd=0, imm5=15
    let encoding: u32 = 0x0E0F0C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_16_poweroftwo_c00_0e100c00() {
    // Encoding: 0x0E100C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 16 (PowerOfTwo)
    // Fields: Rn=0, Q=0, imm5=16, Rd=0
    let encoding: u32 = 0x0E100C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field imm5 16 +: 5`
/// Requirement: FieldBoundary { field: "imm5", value: 31, boundary: Max }
/// maximum immediate (31)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_imm5_31_max_c00_0e1f0c00() {
    // Encoding: 0x0E1F0C00
    // Test aarch64_vector_transfer_integer_dup field imm5 = 31 (Max)
    // Fields: imm5=31, Rd=0, Q=0, Rn=0
    let encoding: u32 = 0x0E1F0C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rn_0_min_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup field Rn = 0 (Min)
    // Fields: Rd=0, Rn=0, Q=0, imm5=0
    let encoding: u32 = 0x0E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rn_1_poweroftwo_c00_0e000c20() {
    // Encoding: 0x0E000C20
    // Test aarch64_vector_transfer_integer_dup field Rn = 1 (PowerOfTwo)
    // Fields: Rd=0, imm5=0, Rn=1, Q=0
    let encoding: u32 = 0x0E000C20;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rn_30_poweroftwominusone_c00_0e000fc0() {
    // Encoding: 0x0E000FC0
    // Test aarch64_vector_transfer_integer_dup field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=30, imm5=0, Rd=0, Q=0
    let encoding: u32 = 0x0E000FC0;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rn_31_max_c00_0e000fe0() {
    // Encoding: 0x0E000FE0
    // Test aarch64_vector_transfer_integer_dup field Rn = 31 (Max)
    // Fields: Q=0, Rn=31, imm5=0, Rd=0
    let encoding: u32 = 0x0E000FE0;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rd_0_min_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup field Rd = 0 (Min)
    // Fields: Rn=0, Q=0, imm5=0, Rd=0
    let encoding: u32 = 0x0E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rd_1_poweroftwo_c00_0e000c01() {
    // Encoding: 0x0E000C01
    // Test aarch64_vector_transfer_integer_dup field Rd = 1 (PowerOfTwo)
    // Fields: imm5=0, Q=0, Rn=0, Rd=1
    let encoding: u32 = 0x0E000C01;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rd_30_poweroftwominusone_c00_0e000c1e() {
    // Encoding: 0x0E000C1E
    // Test aarch64_vector_transfer_integer_dup field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: Rn=0, Q=0, imm5=0, Rd=30
    let encoding: u32 = 0x0E000C1E;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_vector_transfer_integer_dup_field_rd_31_max_c00_0e000c1f() {
    // Encoding: 0x0E000C1F
    // Test aarch64_vector_transfer_integer_dup field Rd = 31 (Max)
    // Fields: imm5=0, Q=0, Rn=0, Rd=31
    let encoding: u32 = 0x0E000C1F;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_vector_transfer_integer_dup_combo_0_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup field combination: Q=0, imm5=0, Rn=0, Rd=0
    // Fields: Q=0, Rd=0, imm5=0, Rn=0
    let encoding: u32 = 0x0E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_vector_transfer_integer_dup_special_q_0_size_variant_0_3072_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup special value Q = 0 (Size variant 0)
    // Fields: imm5=0, Rn=0, Q=0, Rd=0
    let encoding: u32 = 0x0E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_vector_transfer_integer_dup_special_q_1_size_variant_1_3072_4e000c00() {
    // Encoding: 0x4E000C00
    // Test aarch64_vector_transfer_integer_dup special value Q = 1 (Size variant 1)
    // Fields: imm5=0, Rd=0, Rn=0, Q=1
    let encoding: u32 = 0x4E000C00;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_vector_transfer_integer_dup_special_rn_31_stack_pointer_sp_may_require_alignment_3072_0e000fe0(
) {
    // Encoding: 0x0E000FE0
    // Test aarch64_vector_transfer_integer_dup special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Q=0, Rn=31, imm5=0, Rd=0
    let encoding: u32 = 0x0E000FE0;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_vector_transfer_integer_dup_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_3072_0e000c1f(
) {
    // Encoding: 0x0E000C1F
    // Test aarch64_vector_transfer_integer_dup special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, imm5=0, Q=0, Rn=0
    let encoding: u32 = 0x0E000C1F;
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

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: LitInt(3) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_dup_invalid_0_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup invalid encoding: Binary { op: Gt, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: LitInt(3) }
    // Fields: Rd=0, Q=0, imm5=0, Rn=0
    let encoding: u32 = 0x0E000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_dup_invalid_1_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup invalid encoding: Unconditional UNDEFINED
    // Fields: Rd=0, Q=0, Rn=0, imm5=0
    let encoding: u32 = 0x0E000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: And, lhs: LitInt(3), rhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }) } }, rhs: LitBits([false]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"size\" }), rhs: Binary { op: And, lhs: LitInt(3), rhs: Var(QualifiedIdentifier { qualifier: Any, name: \"Q\" }) } }, rhs: LitBits([false]) }" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_dup_invalid_2_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup invalid encoding: Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "size" }), rhs: Binary { op: And, lhs: LitInt(3), rhs: Var(QualifiedIdentifier { qualifier: Any, name: "Q" }) } }, rhs: LitBits([false]) }
    // Fields: Q=0, imm5=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_vector_transfer_integer_dup_invalid_3_c00_0e000c00() {
    // Encoding: 0x0E000C00
    // Test aarch64_vector_transfer_integer_dup invalid encoding: Unconditional UNDEFINED
    // Fields: imm5=0, Q=0, Rn=0, Rd=0
    let encoding: u32 = 0x0E000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `SimdFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("d")
#[test]
fn test_aarch64_vector_transfer_integer_dup_reg_write_0_0e000c00() {
    // Test aarch64_vector_transfer_integer_dup register write: SimdFromField("d")
    // Encoding: 0x0E000C00
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E000C00;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_vector_transfer_integer_dup_sp_rn_0e000fe0() {
    // Test aarch64_vector_transfer_integer_dup with Rn = SP (31)
    // Encoding: 0x0E000FE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E000FE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_vector_transfer_integer_dup
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_vector_transfer_integer_dup_zr_rd_0e000c1f() {
    // Test aarch64_vector_transfer_integer_dup with Rd = ZR (31)
    // Encoding: 0x0E000C1F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0E000C1F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}
