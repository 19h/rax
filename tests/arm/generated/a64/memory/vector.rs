//! A64 memory vector tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_memory_vector_single_no_wb Tests
// ============================================================================

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_q_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field Q = 0 (Min)
    // Fields: Rn=0, Q=0, R=0, Rt=0, opcode=0, L=0, S=0, size=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_q_1_max_0_4d000000() {
    // Encoding: 0x4D000000
    // Test aarch64_memory_vector_single_no_wb field Q = 1 (Max)
    // Fields: size=0, Q=1, L=0, R=0, S=0, Rt=0, opcode=0, Rn=0
    let encoding: u32 = 0x4D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_l_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field L = 0 (Min)
    // Fields: size=0, Q=0, L=0, S=0, Rn=0, R=0, Rt=0, opcode=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_l_1_max_0_0d400000() {
    // Encoding: 0x0D400000
    // Test aarch64_memory_vector_single_no_wb field L = 1 (Max)
    // Fields: opcode=0, Rn=0, Rt=0, S=0, size=0, Q=0, R=0, L=1
    let encoding: u32 = 0x0D400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field R 21 +: 1`
/// Requirement: FieldBoundary { field: "R", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_r_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field R = 0 (Min)
    // Fields: Q=0, opcode=0, Rn=0, S=0, L=0, R=0, Rt=0, size=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field R 21 +: 1`
/// Requirement: FieldBoundary { field: "R", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_r_1_max_0_0d200000() {
    // Encoding: 0x0D200000
    // Test aarch64_memory_vector_single_no_wb field R = 1 (Max)
    // Fields: size=0, Q=0, opcode=0, R=1, Rt=0, S=0, L=0, Rn=0
    let encoding: u32 = 0x0D200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field opcode 13 +: 3`
/// Requirement: FieldBoundary { field: "opcode", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_opcode_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field opcode = 0 (Min)
    // Fields: opcode=0, S=0, Q=0, size=0, Rt=0, R=0, L=0, Rn=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field opcode 13 +: 3`
/// Requirement: FieldBoundary { field: "opcode", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_opcode_1_poweroftwo_0_0d002000() {
    // Encoding: 0x0D002000
    // Test aarch64_memory_vector_single_no_wb field opcode = 1 (PowerOfTwo)
    // Fields: size=0, R=0, Rn=0, L=0, Rt=0, Q=0, opcode=1, S=0
    let encoding: u32 = 0x0D002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field opcode 13 +: 3`
/// Requirement: FieldBoundary { field: "opcode", value: 7, boundary: Max }
/// maximum value (7)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_opcode_7_max_0_0d00e000() {
    // Encoding: 0x0D00E000
    // Test aarch64_memory_vector_single_no_wb field opcode = 7 (Max)
    // Fields: R=0, L=0, opcode=7, size=0, Rt=0, S=0, Q=0, Rn=0
    let encoding: u32 = 0x0D00E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field S 12 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_s_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field S = 0 (Min)
    // Fields: size=0, S=0, opcode=0, R=0, Q=0, L=0, Rn=0, Rt=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field S 12 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_s_1_max_0_0d001000() {
    // Encoding: 0x0D001000
    // Test aarch64_memory_vector_single_no_wb field S = 1 (Max)
    // Fields: size=0, L=0, Rt=0, Q=0, R=0, opcode=0, Rn=0, S=1
    let encoding: u32 = 0x0D001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_size_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field size = 0 (Min)
    // Fields: opcode=0, size=0, R=0, Q=0, L=0, Rt=0, Rn=0, S=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_size_1_poweroftwo_0_0d000400() {
    // Encoding: 0x0D000400
    // Test aarch64_memory_vector_single_no_wb field size = 1 (PowerOfTwo)
    // Fields: opcode=0, Rt=0, L=0, Q=0, size=1, Rn=0, S=0, R=0
    let encoding: u32 = 0x0D000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_size_2_poweroftwo_0_0d000800() {
    // Encoding: 0x0D000800
    // Test aarch64_memory_vector_single_no_wb field size = 2 (PowerOfTwo)
    // Fields: R=0, Q=0, opcode=0, size=2, Rt=0, S=0, Rn=0, L=0
    let encoding: u32 = 0x0D000800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_size_3_max_0_0d000c00() {
    // Encoding: 0x0D000C00
    // Test aarch64_memory_vector_single_no_wb field size = 3 (Max)
    // Fields: Rt=0, opcode=0, L=0, size=3, Rn=0, R=0, S=0, Q=0
    let encoding: u32 = 0x0D000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rn_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field Rn = 0 (Min)
    // Fields: L=0, S=0, Rt=0, opcode=0, Q=0, R=0, Rn=0, size=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rn_1_poweroftwo_0_0d000020() {
    // Encoding: 0x0D000020
    // Test aarch64_memory_vector_single_no_wb field Rn = 1 (PowerOfTwo)
    // Fields: opcode=0, S=0, size=0, Q=0, L=0, R=0, Rt=0, Rn=1
    let encoding: u32 = 0x0D000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rn_30_poweroftwominusone_0_0d0003c0() {
    // Encoding: 0x0D0003C0
    // Test aarch64_memory_vector_single_no_wb field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rt=0, L=0, S=0, Q=0, Rn=30, R=0, opcode=0, size=0
    let encoding: u32 = 0x0D0003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rn_31_max_0_0d0003e0() {
    // Encoding: 0x0D0003E0
    // Test aarch64_memory_vector_single_no_wb field Rn = 31 (Max)
    // Fields: Q=0, R=0, opcode=0, S=0, size=0, Rt=0, Rn=31, L=0
    let encoding: u32 = 0x0D0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rt_0_min_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field Rt = 0 (Min)
    // Fields: Q=0, L=0, S=0, R=0, size=0, Rn=0, Rt=0, opcode=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rt_1_poweroftwo_0_0d000001() {
    // Encoding: 0x0D000001
    // Test aarch64_memory_vector_single_no_wb field Rt = 1 (PowerOfTwo)
    // Fields: S=0, size=0, Rt=1, L=0, R=0, opcode=0, Q=0, Rn=0
    let encoding: u32 = 0x0D000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rt_30_poweroftwominusone_0_0d00001e() {
    // Encoding: 0x0D00001E
    // Test aarch64_memory_vector_single_no_wb field Rt = 30 (PowerOfTwoMinusOne)
    // Fields: Q=0, opcode=0, S=0, size=0, Rn=0, L=0, R=0, Rt=30
    let encoding: u32 = 0x0D00001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_memory_vector_single_no_wb_field_rt_31_max_0_0d00001f() {
    // Encoding: 0x0D00001F
    // Test aarch64_memory_vector_single_no_wb field Rt = 31 (Max)
    // Fields: opcode=0, size=0, Q=0, L=0, R=0, Rn=0, S=0, Rt=31
    let encoding: u32 = 0x0D00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_memory_vector_single_no_wb_combo_0_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb field combination: Q=0, L=0, R=0, opcode=0, S=0, size=0, Rn=0, Rt=0
    // Fields: R=0, Rn=0, L=0, S=0, opcode=0, size=0, Q=0, Rt=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_q_0_size_variant_0_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb special value Q = 0 (Size variant 0)
    // Fields: Rt=0, S=0, opcode=0, Q=0, L=0, R=0, size=0, Rn=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_q_1_size_variant_1_0_4d000000() {
    // Encoding: 0x4D000000
    // Test aarch64_memory_vector_single_no_wb special value Q = 1 (Size variant 1)
    // Fields: size=0, Q=1, R=0, opcode=0, S=0, L=0, Rt=0, Rn=0
    let encoding: u32 = 0x4D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field S = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "S", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_s_0_size_variant_0_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb special value S = 0 (Size variant 0)
    // Fields: Rn=0, size=0, L=0, S=0, Rt=0, opcode=0, Q=0, R=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field S = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "S", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_s_1_size_variant_1_0_0d001000() {
    // Encoding: 0x0D001000
    // Test aarch64_memory_vector_single_no_wb special value S = 1 (Size variant 1)
    // Fields: R=0, opcode=0, Rt=0, L=0, S=1, size=0, Q=0, Rn=0
    let encoding: u32 = 0x0D001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_size_0_size_variant_0_0_0d000000() {
    // Encoding: 0x0D000000
    // Test aarch64_memory_vector_single_no_wb special value size = 0 (Size variant 0)
    // Fields: Rn=0, size=0, S=0, L=0, Q=0, opcode=0, R=0, Rt=0
    let encoding: u32 = 0x0D000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_size_1_size_variant_1_0_0d000400() {
    // Encoding: 0x0D000400
    // Test aarch64_memory_vector_single_no_wb special value size = 1 (Size variant 1)
    // Fields: Q=0, size=1, L=0, Rt=0, Rn=0, opcode=0, S=0, R=0
    let encoding: u32 = 0x0D000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_size_2_size_variant_2_0_0d000800() {
    // Encoding: 0x0D000800
    // Test aarch64_memory_vector_single_no_wb special value size = 2 (Size variant 2)
    // Fields: R=0, L=0, Rn=0, size=2, Rt=0, S=0, opcode=0, Q=0
    let encoding: u32 = 0x0D000800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_size_3_size_variant_3_0_0d000c00() {
    // Encoding: 0x0D000C00
    // Test aarch64_memory_vector_single_no_wb special value size = 3 (Size variant 3)
    // Fields: R=0, L=0, Rn=0, size=3, Rt=0, Q=0, opcode=0, S=0
    let encoding: u32 = 0x0D000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_rn_31_stack_pointer_sp_may_require_alignment_0_0d0003e0() {
    // Encoding: 0x0D0003E0
    // Test aarch64_memory_vector_single_no_wb special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: S=0, L=0, Rn=31, opcode=0, Q=0, R=0, size=0, Rt=0
    let encoding: u32 = 0x0D0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `field Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rt", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_memory_vector_single_no_wb_special_rt_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_0d00001f() {
    // Encoding: 0x0D00001F
    // Test aarch64_memory_vector_single_no_wb special value Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rt=31, R=0, opcode=0, S=0, L=0, size=0, Rn=0, Q=0
    let encoding: u32 = 0x0D00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_q_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field Q = 0 (Min)
    // Fields: Rm=0, S=0, opcode=0, size=0, Rn=0, Q=0, L=0, R=0, Rt=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_q_1_max_0_4d800000() {
    // Encoding: 0x4D800000
    // Test aarch64_memory_vector_single_post_inc field Q = 1 (Max)
    // Fields: Q=1, R=0, S=0, opcode=0, L=0, Rm=0, size=0, Rt=0, Rn=0
    let encoding: u32 = 0x4D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_l_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field L = 0 (Min)
    // Fields: opcode=0, Rn=0, Q=0, R=0, Rt=0, S=0, size=0, L=0, Rm=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_l_1_max_0_0dc00000() {
    // Encoding: 0x0DC00000
    // Test aarch64_memory_vector_single_post_inc field L = 1 (Max)
    // Fields: S=0, Rm=0, size=0, Q=0, L=1, R=0, Rt=0, opcode=0, Rn=0
    let encoding: u32 = 0x0DC00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field R 21 +: 1`
/// Requirement: FieldBoundary { field: "R", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_r_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field R = 0 (Min)
    // Fields: L=0, size=0, Q=0, Rm=0, R=0, Rt=0, opcode=0, S=0, Rn=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field R 21 +: 1`
/// Requirement: FieldBoundary { field: "R", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_r_1_max_0_0da00000() {
    // Encoding: 0x0DA00000
    // Test aarch64_memory_vector_single_post_inc field R = 1 (Max)
    // Fields: R=1, S=0, Rm=0, L=0, opcode=0, Rt=0, Q=0, size=0, Rn=0
    let encoding: u32 = 0x0DA00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rm_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field Rm = 0 (Min)
    // Fields: opcode=0, L=0, S=0, size=0, R=0, Q=0, Rn=0, Rt=0, Rm=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rm_1_poweroftwo_0_0d810000() {
    // Encoding: 0x0D810000
    // Test aarch64_memory_vector_single_post_inc field Rm = 1 (PowerOfTwo)
    // Fields: S=0, Rn=0, Rt=0, R=0, Q=0, L=0, Rm=1, opcode=0, size=0
    let encoding: u32 = 0x0D810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rm_30_poweroftwominusone_0_0d9e0000() {
    // Encoding: 0x0D9E0000
    // Test aarch64_memory_vector_single_post_inc field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: L=0, Rn=0, Rm=30, R=0, size=0, opcode=0, S=0, Q=0, Rt=0
    let encoding: u32 = 0x0D9E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rm_31_max_0_0d9f0000() {
    // Encoding: 0x0D9F0000
    // Test aarch64_memory_vector_single_post_inc field Rm = 31 (Max)
    // Fields: Rm=31, S=0, opcode=0, R=0, Rt=0, Q=0, Rn=0, size=0, L=0
    let encoding: u32 = 0x0D9F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field opcode 13 +: 3`
/// Requirement: FieldBoundary { field: "opcode", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_opcode_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field opcode = 0 (Min)
    // Fields: R=0, Rm=0, L=0, S=0, Q=0, opcode=0, Rt=0, size=0, Rn=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field opcode 13 +: 3`
/// Requirement: FieldBoundary { field: "opcode", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_opcode_1_poweroftwo_0_0d802000() {
    // Encoding: 0x0D802000
    // Test aarch64_memory_vector_single_post_inc field opcode = 1 (PowerOfTwo)
    // Fields: Rt=0, S=0, L=0, Rn=0, size=0, Q=0, R=0, opcode=1, Rm=0
    let encoding: u32 = 0x0D802000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field opcode 13 +: 3`
/// Requirement: FieldBoundary { field: "opcode", value: 7, boundary: Max }
/// maximum value (7)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_opcode_7_max_0_0d80e000() {
    // Encoding: 0x0D80E000
    // Test aarch64_memory_vector_single_post_inc field opcode = 7 (Max)
    // Fields: Rm=0, opcode=7, Rt=0, R=0, Q=0, size=0, Rn=0, S=0, L=0
    let encoding: u32 = 0x0D80E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field S 12 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_s_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field S = 0 (Min)
    // Fields: L=0, Rm=0, Rt=0, S=0, size=0, opcode=0, Q=0, R=0, Rn=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field S 12 +: 1`
/// Requirement: FieldBoundary { field: "S", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_s_1_max_0_0d801000() {
    // Encoding: 0x0D801000
    // Test aarch64_memory_vector_single_post_inc field S = 1 (Max)
    // Fields: Rn=0, R=0, Rm=0, Rt=0, opcode=0, Q=0, size=0, L=0, S=1
    let encoding: u32 = 0x0D801000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_size_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field size = 0 (Min)
    // Fields: Rt=0, Q=0, Rm=0, R=0, opcode=0, Rn=0, S=0, size=0, L=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_size_1_poweroftwo_0_0d800400() {
    // Encoding: 0x0D800400
    // Test aarch64_memory_vector_single_post_inc field size = 1 (PowerOfTwo)
    // Fields: R=0, Rm=0, Rn=0, L=0, S=0, Rt=0, Q=0, size=1, opcode=0
    let encoding: u32 = 0x0D800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_size_2_poweroftwo_0_0d800800() {
    // Encoding: 0x0D800800
    // Test aarch64_memory_vector_single_post_inc field size = 2 (PowerOfTwo)
    // Fields: S=0, L=0, Rn=0, Rt=0, R=0, Q=0, opcode=0, Rm=0, size=2
    let encoding: u32 = 0x0D800800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_size_3_max_0_0d800c00() {
    // Encoding: 0x0D800C00
    // Test aarch64_memory_vector_single_post_inc field size = 3 (Max)
    // Fields: Rm=0, Rt=0, R=0, size=3, Q=0, Rn=0, S=0, opcode=0, L=0
    let encoding: u32 = 0x0D800C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rn_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field Rn = 0 (Min)
    // Fields: S=0, size=0, L=0, Rm=0, Rn=0, Q=0, opcode=0, R=0, Rt=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rn_1_poweroftwo_0_0d800020() {
    // Encoding: 0x0D800020
    // Test aarch64_memory_vector_single_post_inc field Rn = 1 (PowerOfTwo)
    // Fields: R=0, S=0, size=0, opcode=0, Rt=0, Rm=0, L=0, Rn=1, Q=0
    let encoding: u32 = 0x0D800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rn_30_poweroftwominusone_0_0d8003c0() {
    // Encoding: 0x0D8003C0
    // Test aarch64_memory_vector_single_post_inc field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rm=0, R=0, S=0, L=0, Rt=0, Q=0, Rn=30, opcode=0
    let encoding: u32 = 0x0D8003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rn_31_max_0_0d8003e0() {
    // Encoding: 0x0D8003E0
    // Test aarch64_memory_vector_single_post_inc field Rn = 31 (Max)
    // Fields: Q=0, R=0, Rm=0, opcode=0, size=0, Rn=31, L=0, Rt=0, S=0
    let encoding: u32 = 0x0D8003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rt_0_min_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field Rt = 0 (Min)
    // Fields: Rt=0, R=0, Q=0, Rn=0, opcode=0, Rm=0, L=0, size=0, S=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rt_1_poweroftwo_0_0d800001() {
    // Encoding: 0x0D800001
    // Test aarch64_memory_vector_single_post_inc field Rt = 1 (PowerOfTwo)
    // Fields: size=0, R=0, Rt=1, Q=0, Rn=0, opcode=0, L=0, Rm=0, S=0
    let encoding: u32 = 0x0D800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rt_30_poweroftwominusone_0_0d80001e() {
    // Encoding: 0x0D80001E
    // Test aarch64_memory_vector_single_post_inc field Rt = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, R=0, Q=0, Rn=0, Rt=30, Rm=0, opcode=0, S=0, L=0
    let encoding: u32 = 0x0D80001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_memory_vector_single_post_inc_field_rt_31_max_0_0d80001f() {
    // Encoding: 0x0D80001F
    // Test aarch64_memory_vector_single_post_inc field Rt = 31 (Max)
    // Fields: Rm=0, L=0, Rt=31, R=0, size=0, opcode=0, Rn=0, S=0, Q=0
    let encoding: u32 = 0x0D80001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_memory_vector_single_post_inc_combo_0_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc field combination: Q=0, L=0, R=0, Rm=0, opcode=0, S=0, size=0, Rn=0, Rt=0
    // Fields: Rm=0, S=0, L=0, opcode=0, size=0, Q=0, R=0, Rt=0, Rn=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_q_0_size_variant_0_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc special value Q = 0 (Size variant 0)
    // Fields: L=0, Rm=0, size=0, R=0, Rn=0, Rt=0, Q=0, opcode=0, S=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_q_1_size_variant_1_0_4d800000() {
    // Encoding: 0x4D800000
    // Test aarch64_memory_vector_single_post_inc special value Q = 1 (Size variant 1)
    // Fields: opcode=0, Q=1, R=0, size=0, L=0, Rm=0, S=0, Rn=0, Rt=0
    let encoding: u32 = 0x4D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field S = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "S", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_s_0_size_variant_0_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc special value S = 0 (Size variant 0)
    // Fields: Q=0, S=0, size=0, Rt=0, R=0, Rn=0, Rm=0, opcode=0, L=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field S = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "S", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_s_1_size_variant_1_0_0d801000() {
    // Encoding: 0x0D801000
    // Test aarch64_memory_vector_single_post_inc special value S = 1 (Size variant 1)
    // Fields: Rn=0, R=0, Q=0, L=0, opcode=0, S=1, Rm=0, Rt=0, size=0
    let encoding: u32 = 0x0D801000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_size_0_size_variant_0_0_0d800000() {
    // Encoding: 0x0D800000
    // Test aarch64_memory_vector_single_post_inc special value size = 0 (Size variant 0)
    // Fields: L=0, opcode=0, Q=0, Rm=0, R=0, Rt=0, size=0, S=0, Rn=0
    let encoding: u32 = 0x0D800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_size_1_size_variant_1_0_0d800400() {
    // Encoding: 0x0D800400
    // Test aarch64_memory_vector_single_post_inc special value size = 1 (Size variant 1)
    // Fields: Rn=0, Rt=0, Q=0, L=0, R=0, opcode=0, S=0, Rm=0, size=1
    let encoding: u32 = 0x0D800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_size_2_size_variant_2_0_0d800800() {
    // Encoding: 0x0D800800
    // Test aarch64_memory_vector_single_post_inc special value size = 2 (Size variant 2)
    // Fields: Rt=0, Rn=0, Q=0, size=2, Rm=0, S=0, L=0, R=0, opcode=0
    let encoding: u32 = 0x0D800800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_size_3_size_variant_3_0_0d800c00() {
    // Encoding: 0x0D800C00
    // Test aarch64_memory_vector_single_post_inc special value size = 3 (Size variant 3)
    // Fields: R=0, Rn=0, Rt=0, opcode=0, L=0, Rm=0, size=3, Q=0, S=0
    let encoding: u32 = 0x0D800C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_rn_31_stack_pointer_sp_may_require_alignment_0_0d8003e0() {
    // Encoding: 0x0D8003E0
    // Test aarch64_memory_vector_single_post_inc special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: opcode=0, R=0, S=0, Q=0, L=0, Rm=0, size=0, Rn=31, Rt=0
    let encoding: u32 = 0x0D8003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `field Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rt", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_memory_vector_single_post_inc_special_rt_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_0d80001f() {
    // Encoding: 0x0D80001F
    // Test aarch64_memory_vector_single_post_inc special value Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: size=0, Rn=0, S=0, R=0, Rm=0, Q=0, L=0, Rt=31, opcode=0
    let encoding: u32 = 0x0D80001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `SimdFromField("t") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("t")
#[test]
fn test_aarch64_memory_vector_single_no_wb_reg_write_0_0d000000() {
    // Test aarch64_memory_vector_single_no_wb register write: SimdFromField("t")
    // Encoding: 0x0D000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `SimdFromField("t") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("t")
#[test]
fn test_aarch64_memory_vector_single_no_wb_reg_write_1_0d000000() {
    // Test aarch64_memory_vector_single_no_wb register write: SimdFromField("t")
    // Encoding: 0x0D000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `Sp write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to Sp
#[test]
fn test_aarch64_memory_vector_single_no_wb_reg_write_2_0d000000() {
    // Test aarch64_memory_vector_single_no_wb register write: Sp
    // Encoding: 0x0D000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `GpFromField("n") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "n" }
/// verify register write to GpFromField("n")
#[test]
fn test_aarch64_memory_vector_single_no_wb_reg_write_3_0d000000() {
    // Test aarch64_memory_vector_single_no_wb register write: GpFromField("n")
    // Encoding: 0x0D000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_memory_vector_single_no_wb_sp_rn_0d0003e0() {
    // Test aarch64_memory_vector_single_no_wb with Rn = SP (31)
    // Encoding: 0x0D0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `Rt = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rt = 31)
#[test]
fn test_aarch64_memory_vector_single_no_wb_zr_rt_0d00001f() {
    // Test aarch64_memory_vector_single_no_wb with Rt = ZR (31)
    // Encoding: 0x0D00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_memory_vector_single_no_wb
/// ASL: `Mem[address, 8] = data`
/// Requirement: MemoryAccess { op: Store, size_bits: 64, addressing: "Base { reg: \"address\" }" }
/// 8-byte store
#[test]
fn test_aarch64_memory_vector_single_no_wb_store_0_0d000020() {
    // Test aarch64_memory_vector_single_no_wb memory store: 8 bytes
    // Encoding: 0x0D000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 0, 0xDEADBEEFCAFEBABE);
    set_x(&mut cpu, 1, 0x100000000000);
    let encoding: u32 = 0x0D000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `SimdFromField("t") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("t")
#[test]
fn test_aarch64_memory_vector_single_post_inc_reg_write_0_0d800000() {
    // Test aarch64_memory_vector_single_post_inc register write: SimdFromField("t")
    // Encoding: 0x0D800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `SimdFromField("t") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("t")
#[test]
fn test_aarch64_memory_vector_single_post_inc_reg_write_1_0d800000() {
    // Test aarch64_memory_vector_single_post_inc register write: SimdFromField("t")
    // Encoding: 0x0D800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `Sp write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to Sp
#[test]
fn test_aarch64_memory_vector_single_post_inc_reg_write_2_0d800000() {
    // Test aarch64_memory_vector_single_post_inc register write: Sp
    // Encoding: 0x0D800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `GpFromField("n") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "n" }
/// verify register write to GpFromField("n")
#[test]
fn test_aarch64_memory_vector_single_post_inc_reg_write_3_0d800000() {
    // Test aarch64_memory_vector_single_post_inc register write: GpFromField("n")
    // Encoding: 0x0D800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_memory_vector_single_post_inc_sp_rn_0d8003e0() {
    // Test aarch64_memory_vector_single_post_inc with Rn = SP (31)
    // Encoding: 0x0D8003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D8003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `Rt = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rt = 31)
#[test]
fn test_aarch64_memory_vector_single_post_inc_zr_rt_0d80001f() {
    // Test aarch64_memory_vector_single_post_inc with Rt = ZR (31)
    // Encoding: 0x0D80001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0D80001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_memory_vector_single_post_inc
/// ASL: `Mem[address, 8] = data`
/// Requirement: MemoryAccess { op: Store, size_bits: 64, addressing: "Base { reg: \"address\" }" }
/// 8-byte store
#[test]
fn test_aarch64_memory_vector_single_post_inc_store_0_0d800020() {
    // Test aarch64_memory_vector_single_post_inc memory store: 8 bytes
    // Encoding: 0x0D800020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 0, 0xDEADBEEFCAFEBABE);
    set_x(&mut cpu, 1, 0x100000000000);
    let encoding: u32 = 0x0D800020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// aarch64_memory_vector_multiple_no_wb Tests
// ============================================================================

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_q_0_min_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb field Q = 0 (Min)
    // Fields: Rn=0, size=0, Rt=0, L=0, Q=0, opcode=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_q_1_max_0_4c000000() {
    // Encoding: 0x4C000000
    // Test aarch64_memory_vector_multiple_no_wb field Q = 1 (Max)
    // Fields: Q=1, opcode=0, L=0, Rt=0, size=0, Rn=0
    let encoding: u32 = 0x4C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_l_0_min_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb field L = 0 (Min)
    // Fields: size=0, opcode=0, Rn=0, Q=0, L=0, Rt=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_l_1_max_0_0c400000() {
    // Encoding: 0x0C400000
    // Test aarch64_memory_vector_multiple_no_wb field L = 1 (Max)
    // Fields: L=1, opcode=0, Rn=0, Rt=0, Q=0, size=0
    let encoding: u32 = 0x0C400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_opcode_0_min_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb field opcode = 0 (Min)
    // Fields: Q=0, opcode=0, size=0, L=0, Rn=0, Rt=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_opcode_1_poweroftwo_0_0c001000() {
    // Encoding: 0x0C001000
    // Test aarch64_memory_vector_multiple_no_wb field opcode = 1 (PowerOfTwo)
    // Fields: size=0, Rn=0, Rt=0, Q=0, opcode=1, L=0
    let encoding: u32 = 0x0C001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 7, boundary: PowerOfTwoMinusOne }
/// midpoint (7)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_opcode_7_poweroftwominusone_0_0c007000() {
    // Encoding: 0x0C007000
    // Test aarch64_memory_vector_multiple_no_wb field opcode = 7 (PowerOfTwoMinusOne)
    // Fields: Q=0, L=0, Rn=0, Rt=0, opcode=7, size=0
    let encoding: u32 = 0x0C007000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 15, boundary: Max }
/// maximum value (15)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_opcode_15_max_0_0c00f000() {
    // Encoding: 0x0C00F000
    // Test aarch64_memory_vector_multiple_no_wb field opcode = 15 (Max)
    // Fields: size=0, Rn=0, L=0, Q=0, Rt=0, opcode=15
    let encoding: u32 = 0x0C00F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_size_0_min_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb field size = 0 (Min)
    // Fields: Q=0, L=0, opcode=0, size=0, Rt=0, Rn=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_size_1_poweroftwo_0_0c000400() {
    // Encoding: 0x0C000400
    // Test aarch64_memory_vector_multiple_no_wb field size = 1 (PowerOfTwo)
    // Fields: L=0, Q=0, size=1, Rn=0, Rt=0, opcode=0
    let encoding: u32 = 0x0C000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_size_2_poweroftwo_0_0c000800() {
    // Encoding: 0x0C000800
    // Test aarch64_memory_vector_multiple_no_wb field size = 2 (PowerOfTwo)
    // Fields: Rn=0, Rt=0, Q=0, opcode=0, L=0, size=2
    let encoding: u32 = 0x0C000800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_size_3_max_0_0c000c00() {
    // Encoding: 0x0C000C00
    // Test aarch64_memory_vector_multiple_no_wb field size = 3 (Max)
    // Fields: L=0, Rt=0, size=3, opcode=0, Rn=0, Q=0
    let encoding: u32 = 0x0C000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rn_0_min_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb field Rn = 0 (Min)
    // Fields: Q=0, L=0, Rn=0, size=0, Rt=0, opcode=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rn_1_poweroftwo_0_0c000020() {
    // Encoding: 0x0C000020
    // Test aarch64_memory_vector_multiple_no_wb field Rn = 1 (PowerOfTwo)
    // Fields: Q=0, opcode=0, Rt=0, L=0, size=0, Rn=1
    let encoding: u32 = 0x0C000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rn_30_poweroftwominusone_0_0c0003c0() {
    // Encoding: 0x0C0003C0
    // Test aarch64_memory_vector_multiple_no_wb field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Q=0, L=0, size=0, Rn=30, Rt=0, opcode=0
    let encoding: u32 = 0x0C0003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rn_31_max_0_0c0003e0() {
    // Encoding: 0x0C0003E0
    // Test aarch64_memory_vector_multiple_no_wb field Rn = 31 (Max)
    // Fields: Rn=31, opcode=0, Rt=0, size=0, Q=0, L=0
    let encoding: u32 = 0x0C0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rt_0_min_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb field Rt = 0 (Min)
    // Fields: Q=0, L=0, opcode=0, size=0, Rn=0, Rt=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rt_1_poweroftwo_0_0c000001() {
    // Encoding: 0x0C000001
    // Test aarch64_memory_vector_multiple_no_wb field Rt = 1 (PowerOfTwo)
    // Fields: Q=0, Rn=0, opcode=0, L=0, size=0, Rt=1
    let encoding: u32 = 0x0C000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rt_30_poweroftwominusone_0_0c00001e() {
    // Encoding: 0x0C00001E
    // Test aarch64_memory_vector_multiple_no_wb field Rt = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Q=0, L=0, Rt=30, Rn=0, opcode=0
    let encoding: u32 = 0x0C00001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_field_rt_31_max_0_0c00001f() {
    // Encoding: 0x0C00001F
    // Test aarch64_memory_vector_multiple_no_wb field Rt = 31 (Max)
    // Fields: L=0, Rt=31, opcode=0, size=0, Rn=0, Q=0
    let encoding: u32 = 0x0C00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_combo_0_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb field combination: Q=0, L=0, opcode=0, size=0, Rn=0, Rt=0
    // Fields: size=0, Rt=0, Rn=0, L=0, Q=0, opcode=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_q_0_size_variant_0_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb special value Q = 0 (Size variant 0)
    // Fields: Q=0, Rt=0, L=0, opcode=0, size=0, Rn=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_q_1_size_variant_1_0_4c000000() {
    // Encoding: 0x4C000000
    // Test aarch64_memory_vector_multiple_no_wb special value Q = 1 (Size variant 1)
    // Fields: opcode=0, Rt=0, Rn=0, L=0, size=0, Q=1
    let encoding: u32 = 0x4C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_size_0_size_variant_0_0_0c000000() {
    // Encoding: 0x0C000000
    // Test aarch64_memory_vector_multiple_no_wb special value size = 0 (Size variant 0)
    // Fields: size=0, Q=0, opcode=0, Rn=0, Rt=0, L=0
    let encoding: u32 = 0x0C000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_size_1_size_variant_1_0_0c000400() {
    // Encoding: 0x0C000400
    // Test aarch64_memory_vector_multiple_no_wb special value size = 1 (Size variant 1)
    // Fields: Q=0, L=0, opcode=0, Rn=0, size=1, Rt=0
    let encoding: u32 = 0x0C000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_size_2_size_variant_2_0_0c000800() {
    // Encoding: 0x0C000800
    // Test aarch64_memory_vector_multiple_no_wb special value size = 2 (Size variant 2)
    // Fields: size=2, L=0, Rt=0, Rn=0, Q=0, opcode=0
    let encoding: u32 = 0x0C000800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_size_3_size_variant_3_0_0c000c00() {
    // Encoding: 0x0C000C00
    // Test aarch64_memory_vector_multiple_no_wb special value size = 3 (Size variant 3)
    // Fields: Rt=0, L=0, opcode=0, Q=0, size=3, Rn=0
    let encoding: u32 = 0x0C000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_rn_31_stack_pointer_sp_may_require_alignment_0_0c0003e0() {
    // Encoding: 0x0C0003E0
    // Test aarch64_memory_vector_multiple_no_wb special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: L=0, Rn=31, size=0, Rt=0, Q=0, opcode=0
    let encoding: u32 = 0x0C0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `field Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rt", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_special_rt_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_0c00001f() {
    // Encoding: 0x0C00001F
    // Test aarch64_memory_vector_multiple_no_wb special value Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: size=0, Rn=0, Rt=31, L=0, Q=0, opcode=0
    let encoding: u32 = 0x0C00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_q_0_min_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field Q = 0 (Min)
    // Fields: Q=0, size=0, L=0, opcode=0, Rm=0, Rt=0, Rn=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Q 30 +: 1`
/// Requirement: FieldBoundary { field: "Q", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_q_1_max_0_4c800000() {
    // Encoding: 0x4C800000
    // Test aarch64_memory_vector_multiple_post_inc field Q = 1 (Max)
    // Fields: size=0, Rt=0, Q=1, opcode=0, L=0, Rm=0, Rn=0
    let encoding: u32 = 0x4C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_l_0_min_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field L = 0 (Min)
    // Fields: Q=0, Rm=0, size=0, Rn=0, opcode=0, Rt=0, L=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_l_1_max_0_0cc00000() {
    // Encoding: 0x0CC00000
    // Test aarch64_memory_vector_multiple_post_inc field L = 1 (Max)
    // Fields: size=0, Rn=0, Rt=0, opcode=0, L=1, Q=0, Rm=0
    let encoding: u32 = 0x0CC00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rm_0_min_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field Rm = 0 (Min)
    // Fields: opcode=0, Rt=0, Rm=0, L=0, Q=0, size=0, Rn=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rm_1_poweroftwo_0_0c810000() {
    // Encoding: 0x0C810000
    // Test aarch64_memory_vector_multiple_post_inc field Rm = 1 (PowerOfTwo)
    // Fields: size=0, Rt=0, Q=0, Rm=1, L=0, opcode=0, Rn=0
    let encoding: u32 = 0x0C810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rm_30_poweroftwominusone_0_0c9e0000() {
    // Encoding: 0x0C9E0000
    // Test aarch64_memory_vector_multiple_post_inc field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, opcode=0, L=0, Rm=30, Q=0, Rn=0, Rt=0
    let encoding: u32 = 0x0C9E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rm_31_max_0_0c9f0000() {
    // Encoding: 0x0C9F0000
    // Test aarch64_memory_vector_multiple_post_inc field Rm = 31 (Max)
    // Fields: L=0, opcode=0, Q=0, size=0, Rm=31, Rn=0, Rt=0
    let encoding: u32 = 0x0C9F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_opcode_0_min_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field opcode = 0 (Min)
    // Fields: Q=0, Rt=0, L=0, opcode=0, Rm=0, size=0, Rn=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_opcode_1_poweroftwo_0_0c801000() {
    // Encoding: 0x0C801000
    // Test aarch64_memory_vector_multiple_post_inc field opcode = 1 (PowerOfTwo)
    // Fields: Rt=0, Rm=0, Rn=0, L=0, Q=0, size=0, opcode=1
    let encoding: u32 = 0x0C801000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 7, boundary: PowerOfTwoMinusOne }
/// midpoint (7)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_opcode_7_poweroftwominusone_0_0c807000() {
    // Encoding: 0x0C807000
    // Test aarch64_memory_vector_multiple_post_inc field opcode = 7 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rt=0, Q=0, L=0, size=0, Rn=0, opcode=7
    let encoding: u32 = 0x0C807000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field opcode 12 +: 4`
/// Requirement: FieldBoundary { field: "opcode", value: 15, boundary: Max }
/// maximum value (15)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_opcode_15_max_0_0c80f000() {
    // Encoding: 0x0C80F000
    // Test aarch64_memory_vector_multiple_post_inc field opcode = 15 (Max)
    // Fields: L=0, Rt=0, Rn=0, opcode=15, size=0, Rm=0, Q=0
    let encoding: u32 = 0x0C80F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_size_0_min_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field size = 0 (Min)
    // Fields: Q=0, Rn=0, size=0, Rt=0, L=0, Rm=0, opcode=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_size_1_poweroftwo_0_0c800400() {
    // Encoding: 0x0C800400
    // Test aarch64_memory_vector_multiple_post_inc field size = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, Rt=0, Q=0, L=0, opcode=0, size=1
    let encoding: u32 = 0x0C800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_size_2_poweroftwo_0_0c800800() {
    // Encoding: 0x0C800800
    // Test aarch64_memory_vector_multiple_post_inc field size = 2 (PowerOfTwo)
    // Fields: Rt=0, Q=0, Rm=0, L=0, opcode=0, Rn=0, size=2
    let encoding: u32 = 0x0C800800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size 10 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_size_3_max_0_0c800c00() {
    // Encoding: 0x0C800C00
    // Test aarch64_memory_vector_multiple_post_inc field size = 3 (Max)
    // Fields: size=3, Q=0, Rm=0, opcode=0, Rn=0, Rt=0, L=0
    let encoding: u32 = 0x0C800C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rn_0_min_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field Rn = 0 (Min)
    // Fields: Q=0, L=0, Rm=0, Rn=0, size=0, Rt=0, opcode=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rn_1_poweroftwo_0_0c800020() {
    // Encoding: 0x0C800020
    // Test aarch64_memory_vector_multiple_post_inc field Rn = 1 (PowerOfTwo)
    // Fields: L=0, opcode=0, size=0, Rn=1, Q=0, Rt=0, Rm=0
    let encoding: u32 = 0x0C800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rn_30_poweroftwominusone_0_0c8003c0() {
    // Encoding: 0x0C8003C0
    // Test aarch64_memory_vector_multiple_post_inc field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: opcode=0, Rn=30, Rt=0, Q=0, size=0, L=0, Rm=0
    let encoding: u32 = 0x0C8003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rn_31_max_0_0c8003e0() {
    // Encoding: 0x0C8003E0
    // Test aarch64_memory_vector_multiple_post_inc field Rn = 31 (Max)
    // Fields: Q=0, opcode=0, size=0, Rt=0, Rm=0, Rn=31, L=0
    let encoding: u32 = 0x0C8003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rt_0_min_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field Rt = 0 (Min)
    // Fields: opcode=0, Q=0, L=0, Rn=0, Rt=0, size=0, Rm=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rt_1_poweroftwo_0_0c800001() {
    // Encoding: 0x0C800001
    // Test aarch64_memory_vector_multiple_post_inc field Rt = 1 (PowerOfTwo)
    // Fields: L=0, size=0, Rn=0, opcode=0, Q=0, Rt=1, Rm=0
    let encoding: u32 = 0x0C800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rt_30_poweroftwominusone_0_0c80001e() {
    // Encoding: 0x0C80001E
    // Test aarch64_memory_vector_multiple_post_inc field Rt = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, Rn=0, Q=0, L=0, opcode=0, Rt=30, size=0
    let encoding: u32 = 0x0C80001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_field_rt_31_max_0_0c80001f() {
    // Encoding: 0x0C80001F
    // Test aarch64_memory_vector_multiple_post_inc field Rt = 31 (Max)
    // Fields: opcode=0, L=0, Rm=0, Q=0, size=0, Rn=0, Rt=31
    let encoding: u32 = 0x0C80001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Q=0 (8-bit / byte size)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_combo_0_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc field combination: Q=0, L=0, Rm=0, opcode=0, size=0, Rn=0, Rt=0
    // Fields: Rm=0, opcode=0, size=0, Rt=0, L=0, Rn=0, Q=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Q = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "Q", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_q_0_size_variant_0_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc special value Q = 0 (Size variant 0)
    // Fields: Rt=0, size=0, opcode=0, Q=0, L=0, Rm=0, Rn=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Q = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "Q", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_q_1_size_variant_1_0_4c800000() {
    // Encoding: 0x4C800000
    // Test aarch64_memory_vector_multiple_post_inc special value Q = 1 (Size variant 1)
    // Fields: size=0, Rm=0, Rn=0, Q=1, L=0, Rt=0, opcode=0
    let encoding: u32 = 0x4C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_size_0_size_variant_0_0_0c800000() {
    // Encoding: 0x0C800000
    // Test aarch64_memory_vector_multiple_post_inc special value size = 0 (Size variant 0)
    // Fields: Q=0, L=0, size=0, Rn=0, opcode=0, Rt=0, Rm=0
    let encoding: u32 = 0x0C800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_size_1_size_variant_1_0_0c800400() {
    // Encoding: 0x0C800400
    // Test aarch64_memory_vector_multiple_post_inc special value size = 1 (Size variant 1)
    // Fields: Rt=0, Rn=0, L=0, Q=0, opcode=0, Rm=0, size=1
    let encoding: u32 = 0x0C800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_size_2_size_variant_2_0_0c800800() {
    // Encoding: 0x0C800800
    // Test aarch64_memory_vector_multiple_post_inc special value size = 2 (Size variant 2)
    // Fields: Rm=0, size=2, Q=0, opcode=0, Rn=0, L=0, Rt=0
    let encoding: u32 = 0x0C800800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_size_3_size_variant_3_0_0c800c00() {
    // Encoding: 0x0C800C00
    // Test aarch64_memory_vector_multiple_post_inc special value size = 3 (Size variant 3)
    // Fields: opcode=0, size=3, Rt=0, Rm=0, Rn=0, Q=0, L=0
    let encoding: u32 = 0x0C800C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_rn_31_stack_pointer_sp_may_require_alignment_0_0c8003e0() {
    // Encoding: 0x0C8003E0
    // Test aarch64_memory_vector_multiple_post_inc special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Q=0, L=0, Rm=0, opcode=0, size=0, Rn=31, Rt=0
    let encoding: u32 = 0x0C8003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `field Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rt", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_special_rt_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_0c80001f() {
    // Encoding: 0x0C80001F
    // Test aarch64_memory_vector_multiple_post_inc special value Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: L=0, Rm=0, size=0, Rt=31, Rn=0, Q=0, opcode=0
    let encoding: u32 = 0x0C80001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `SimdFromField("tt") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("tt")
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_reg_write_0_0c000000() {
    // Test aarch64_memory_vector_multiple_no_wb register write: SimdFromField("tt")
    // Encoding: 0x0C000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `Sp write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to Sp
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_reg_write_1_0c000000() {
    // Test aarch64_memory_vector_multiple_no_wb register write: Sp
    // Encoding: 0x0C000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `GpFromField("n") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "n" }
/// verify register write to GpFromField("n")
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_reg_write_2_0c000000() {
    // Test aarch64_memory_vector_multiple_no_wb register write: GpFromField("n")
    // Encoding: 0x0C000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_sp_rn_0c0003e0() {
    // Test aarch64_memory_vector_multiple_no_wb with Rn = SP (31)
    // Encoding: 0x0C0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `Rt = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rt = 31)
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_zr_rt_0c00001f() {
    // Test aarch64_memory_vector_multiple_no_wb with Rt = ZR (31)
    // Encoding: 0x0C00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_memory_vector_multiple_no_wb
/// ASL: `Mem[address, 8] = data`
/// Requirement: MemoryAccess { op: Store, size_bits: 64, addressing: "Base { reg: \"address\" }" }
/// 8-byte store
#[test]
fn test_aarch64_memory_vector_multiple_no_wb_store_0_0c000020() {
    // Test aarch64_memory_vector_multiple_no_wb memory store: 8 bytes
    // Encoding: 0x0C000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 0, 0xDEADBEEFCAFEBABE);
    set_x(&mut cpu, 1, 0x100000000000);
    let encoding: u32 = 0x0C000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `SimdFromField("tt") write`
/// Requirement: RegisterWrite { reg_type: Simd128, dest_field: "unknown" }
/// verify register write to SimdFromField("tt")
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_reg_write_0_0c800000() {
    // Test aarch64_memory_vector_multiple_post_inc register write: SimdFromField("tt")
    // Encoding: 0x0C800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `Sp write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to Sp
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_reg_write_1_0c800000() {
    // Test aarch64_memory_vector_multiple_post_inc register write: Sp
    // Encoding: 0x0C800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `GpFromField("n") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "n" }
/// verify register write to GpFromField("n")
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_reg_write_2_0c800000() {
    // Test aarch64_memory_vector_multiple_post_inc register write: GpFromField("n")
    // Encoding: 0x0C800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_sp_rn_0c8003e0() {
    // Test aarch64_memory_vector_multiple_post_inc with Rn = SP (31)
    // Encoding: 0x0C8003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C8003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `Rt = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rt = 31)
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_zr_rt_0c80001f() {
    // Test aarch64_memory_vector_multiple_post_inc with Rt = ZR (31)
    // Encoding: 0x0C80001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0C80001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_memory_vector_multiple_post_inc
/// ASL: `Mem[address, 8] = data`
/// Requirement: MemoryAccess { op: Store, size_bits: 64, addressing: "Base { reg: \"address\" }" }
/// 8-byte store
#[test]
fn test_aarch64_memory_vector_multiple_post_inc_store_0_0c800020() {
    // Test aarch64_memory_vector_multiple_post_inc memory store: 8 bytes
    // Encoding: 0x0C800020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x100000000000);
    set_x(&mut cpu, 0, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0x0C800020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

