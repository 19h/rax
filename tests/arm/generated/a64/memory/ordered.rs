//! A64 memory ordered tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_memory_ordered Tests
// ============================================================================

/// Provenance: aarch64_memory_ordered
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_ordered_field_size_0_min_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field size = 0 (Min)
    // Fields: Rn=0, Rt=0, o0=0, Rt2=0, size=0, L=0, Rs=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_ordered_field_size_1_poweroftwo_0_48800000() {
    // Encoding: 0x48800000
    // Test aarch64_memory_ordered field size = 1 (PowerOfTwo)
    // Fields: size=1, Rs=0, o0=0, Rn=0, Rt=0, Rt2=0, L=0
    let encoding: u32 = 0x48800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_memory_ordered_field_size_2_poweroftwo_0_88800000() {
    // Encoding: 0x88800000
    // Test aarch64_memory_ordered field size = 2 (PowerOfTwo)
    // Fields: Rt=0, Rn=0, o0=0, size=2, L=0, Rs=0, Rt2=0
    let encoding: u32 = 0x88800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_memory_ordered_field_size_3_max_0_c8800000() {
    // Encoding: 0xC8800000
    // Test aarch64_memory_ordered field size = 3 (Max)
    // Fields: Rt2=0, o0=0, Rn=0, Rt=0, Rs=0, L=0, size=3
    let encoding: u32 = 0xC8800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_ordered_field_l_0_min_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field L = 0 (Min)
    // Fields: o0=0, Rs=0, L=0, Rn=0, Rt=0, Rt2=0, size=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field L 22 +: 1`
/// Requirement: FieldBoundary { field: "L", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_ordered_field_l_1_max_0_08c00000() {
    // Encoding: 0x08C00000
    // Test aarch64_memory_ordered field L = 1 (Max)
    // Fields: o0=0, size=0, L=1, Rs=0, Rn=0, Rt=0, Rt2=0
    let encoding: u32 = 0x08C00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_ordered_field_rs_0_min_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field Rs = 0 (Min)
    // Fields: Rt2=0, size=0, o0=0, Rs=0, L=0, Rn=0, Rt=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_ordered_field_rs_1_poweroftwo_0_08810000() {
    // Encoding: 0x08810000
    // Test aarch64_memory_ordered field Rs = 1 (PowerOfTwo)
    // Fields: L=0, o0=0, Rt2=0, size=0, Rs=1, Rn=0, Rt=0
    let encoding: u32 = 0x08810000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_ordered_field_rs_30_poweroftwominusone_0_089e0000() {
    // Encoding: 0x089E0000
    // Test aarch64_memory_ordered field Rs = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rt2=0, Rs=30, L=0, o0=0, Rn=0, Rt=0
    let encoding: u32 = 0x089E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_memory_ordered_field_rs_31_max_0_089f0000() {
    // Encoding: 0x089F0000
    // Test aarch64_memory_ordered field Rs = 31 (Max)
    // Fields: L=0, Rt2=0, Rs=31, size=0, o0=0, Rn=0, Rt=0
    let encoding: u32 = 0x089F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field o0 15 +: 1`
/// Requirement: FieldBoundary { field: "o0", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_memory_ordered_field_o0_0_min_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field o0 = 0 (Min)
    // Fields: Rt=0, size=0, Rs=0, L=0, Rt2=0, Rn=0, o0=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field o0 15 +: 1`
/// Requirement: FieldBoundary { field: "o0", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_memory_ordered_field_o0_1_max_0_08808000() {
    // Encoding: 0x08808000
    // Test aarch64_memory_ordered field o0 = 1 (Max)
    // Fields: Rt2=0, size=0, Rs=0, o0=1, L=0, Rn=0, Rt=0
    let encoding: u32 = 0x08808000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt2 10 +: 5`
/// Requirement: FieldBoundary { field: "Rt2", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_ordered_field_rt2_0_min_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field Rt2 = 0 (Min)
    // Fields: Rt2=0, o0=0, Rt=0, L=0, Rn=0, size=0, Rs=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt2 10 +: 5`
/// Requirement: FieldBoundary { field: "Rt2", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_ordered_field_rt2_1_poweroftwo_0_08800400() {
    // Encoding: 0x08800400
    // Test aarch64_memory_ordered field Rt2 = 1 (PowerOfTwo)
    // Fields: Rt2=1, Rn=0, Rt=0, L=0, size=0, Rs=0, o0=0
    let encoding: u32 = 0x08800400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt2 10 +: 5`
/// Requirement: FieldBoundary { field: "Rt2", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_ordered_field_rt2_30_poweroftwominusone_0_08807800() {
    // Encoding: 0x08807800
    // Test aarch64_memory_ordered field Rt2 = 30 (PowerOfTwoMinusOne)
    // Fields: o0=0, Rs=0, size=0, L=0, Rn=0, Rt=0, Rt2=30
    let encoding: u32 = 0x08807800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt2 10 +: 5`
/// Requirement: FieldBoundary { field: "Rt2", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_memory_ordered_field_rt2_31_max_0_08807c00() {
    // Encoding: 0x08807C00
    // Test aarch64_memory_ordered field Rt2 = 31 (Max)
    // Fields: Rn=0, L=0, Rs=0, size=0, o0=0, Rt2=31, Rt=0
    let encoding: u32 = 0x08807C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_ordered_field_rn_0_min_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field Rn = 0 (Min)
    // Fields: Rs=0, L=0, o0=0, size=0, Rt2=0, Rn=0, Rt=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_ordered_field_rn_1_poweroftwo_0_08800020() {
    // Encoding: 0x08800020
    // Test aarch64_memory_ordered field Rn = 1 (PowerOfTwo)
    // Fields: L=0, Rt=0, Rn=1, size=0, Rt2=0, o0=0, Rs=0
    let encoding: u32 = 0x08800020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_ordered_field_rn_30_poweroftwominusone_0_088003c0() {
    // Encoding: 0x088003C0
    // Test aarch64_memory_ordered field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rs=0, Rt2=0, Rn=30, L=0, o0=0, Rt=0
    let encoding: u32 = 0x088003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_memory_ordered_field_rn_31_max_0_088003e0() {
    // Encoding: 0x088003E0
    // Test aarch64_memory_ordered field Rn = 31 (Max)
    // Fields: o0=0, Rn=31, L=0, size=0, Rs=0, Rt2=0, Rt=0
    let encoding: u32 = 0x088003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_ordered_field_rt_0_min_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field Rt = 0 (Min)
    // Fields: Rt2=0, Rt=0, Rs=0, o0=0, L=0, Rn=0, size=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_ordered_field_rt_1_poweroftwo_0_08800001() {
    // Encoding: 0x08800001
    // Test aarch64_memory_ordered field Rt = 1 (PowerOfTwo)
    // Fields: o0=0, Rn=0, Rt=1, L=0, Rt2=0, size=0, Rs=0
    let encoding: u32 = 0x08800001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_ordered_field_rt_30_poweroftwominusone_0_0880001e() {
    // Encoding: 0x0880001E
    // Test aarch64_memory_ordered field Rt = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rn=0, L=0, o0=0, Rs=0, Rt=30, Rt2=0
    let encoding: u32 = 0x0880001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_memory_ordered_field_rt_31_max_0_0880001f() {
    // Encoding: 0x0880001F
    // Test aarch64_memory_ordered field Rt = 31 (Max)
    // Fields: L=0, Rt2=0, size=0, Rn=0, Rt=31, Rs=0, o0=0
    let encoding: u32 = 0x0880001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_aarch64_memory_ordered_combo_0_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered field combination: size=0, L=0, Rs=0, o0=0, Rt2=0, Rn=0, Rt=0
    // Fields: size=0, o0=0, Rt2=0, L=0, Rn=0, Rs=0, Rt=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_ordered_special_size_0_size_variant_0_0_08800000() {
    // Encoding: 0x08800000
    // Test aarch64_memory_ordered special value size = 0 (Size variant 0)
    // Fields: size=0, L=0, Rn=0, Rt=0, Rs=0, o0=0, Rt2=0
    let encoding: u32 = 0x08800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_ordered_special_size_1_size_variant_1_0_48800000() {
    // Encoding: 0x48800000
    // Test aarch64_memory_ordered special value size = 1 (Size variant 1)
    // Fields: Rn=0, L=0, Rt=0, size=1, Rs=0, o0=0, Rt2=0
    let encoding: u32 = 0x48800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_memory_ordered_special_size_2_size_variant_2_0_88800000() {
    // Encoding: 0x88800000
    // Test aarch64_memory_ordered special value size = 2 (Size variant 2)
    // Fields: o0=0, Rn=0, Rs=0, size=2, L=0, Rt2=0, Rt=0
    let encoding: u32 = 0x88800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_memory_ordered_special_size_3_size_variant_3_0_c8800000() {
    // Encoding: 0xC8800000
    // Test aarch64_memory_ordered special value size = 3 (Size variant 3)
    // Fields: L=0, Rs=0, Rt2=0, o0=0, Rt=0, Rn=0, size=3
    let encoding: u32 = 0xC8800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_memory_ordered_special_rn_31_stack_pointer_sp_may_require_alignment_0_088003e0() {
    // Encoding: 0x088003E0
    // Test aarch64_memory_ordered special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rt2=0, size=0, L=0, Rs=0, Rn=31, o0=0, Rt=0
    let encoding: u32 = 0x088003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `field Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rt", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_memory_ordered_special_rt_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_0880001f() {
    // Encoding: 0x0880001F
    // Test aarch64_memory_ordered special value Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rt2=0, Rn=0, Rt=31, size=0, L=0, Rs=0, o0=0
    let encoding: u32 = 0x0880001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered
/// ASL: `GpFromField("t") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "t" }
/// verify register write to GpFromField("t")
#[test]
fn test_aarch64_memory_ordered_reg_write_0_08800000() {
    // Test aarch64_memory_ordered register write: GpFromField("t")
    // Encoding: 0x08800000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x08800000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_ordered
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_memory_ordered_sp_rn_088003e0() {
    // Test aarch64_memory_ordered with Rn = SP (31)
    // Encoding: 0x088003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x088003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_ordered
/// ASL: `Rt = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rt = 31)
#[test]
fn test_aarch64_memory_ordered_zr_rt_0880001f() {
    // Test aarch64_memory_ordered with Rt = ZR (31)
    // Encoding: 0x0880001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0880001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_memory_ordered
/// ASL: `Mem[address, 8] = data`
/// Requirement: MemoryAccess { op: Store, size_bits: 64, addressing: "Base { reg: \"address\" }" }
/// 8-byte store
#[test]
fn test_aarch64_memory_ordered_store_0_08800020() {
    // Test aarch64_memory_ordered memory store: 8 bytes
    // Encoding: 0x08800020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x100000000000);
    set_x(&mut cpu, 0, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0x08800020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// aarch64_memory_ordered_rcpc Tests
// ============================================================================

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_memory_ordered_rcpc_field_size_0_min_c000_38a0c000() {
    // Encoding: 0x38A0C000
    // Test aarch64_memory_ordered_rcpc field size = 0 (Min)
    // Fields: Rs=0, Rn=0, size=0, Rt=0
    let encoding: u32 = 0x38A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_memory_ordered_rcpc_field_size_1_poweroftwo_c000_78a0c000() {
    // Encoding: 0x78A0C000
    // Test aarch64_memory_ordered_rcpc field size = 1 (PowerOfTwo)
    // Fields: Rn=0, size=1, Rs=0, Rt=0
    let encoding: u32 = 0x78A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_memory_ordered_rcpc_field_size_2_poweroftwo_c000_b8a0c000() {
    // Encoding: 0xB8A0C000
    // Test aarch64_memory_ordered_rcpc field size = 2 (PowerOfTwo)
    // Fields: Rn=0, size=2, Rs=0, Rt=0
    let encoding: u32 = 0xB8A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size 30 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_memory_ordered_rcpc_field_size_3_max_c000_f8a0c000() {
    // Encoding: 0xF8A0C000
    // Test aarch64_memory_ordered_rcpc field size = 3 (Max)
    // Fields: size=3, Rt=0, Rn=0, Rs=0
    let encoding: u32 = 0xF8A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rs_0_min_c000_38a0c000() {
    // Encoding: 0x38A0C000
    // Test aarch64_memory_ordered_rcpc field Rs = 0 (Min)
    // Fields: Rs=0, size=0, Rn=0, Rt=0
    let encoding: u32 = 0x38A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rs_1_poweroftwo_c000_38a1c000() {
    // Encoding: 0x38A1C000
    // Test aarch64_memory_ordered_rcpc field Rs = 1 (PowerOfTwo)
    // Fields: Rt=0, Rs=1, size=0, Rn=0
    let encoding: u32 = 0x38A1C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rs_30_poweroftwominusone_c000_38bec000() {
    // Encoding: 0x38BEC000
    // Test aarch64_memory_ordered_rcpc field Rs = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rs=30, Rt=0, Rn=0
    let encoding: u32 = 0x38BEC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rs 16 +: 5`
/// Requirement: FieldBoundary { field: "Rs", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rs_31_max_c000_38bfc000() {
    // Encoding: 0x38BFC000
    // Test aarch64_memory_ordered_rcpc field Rs = 31 (Max)
    // Fields: Rn=0, size=0, Rs=31, Rt=0
    let encoding: u32 = 0x38BFC000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rn_0_min_c000_38a0c000() {
    // Encoding: 0x38A0C000
    // Test aarch64_memory_ordered_rcpc field Rn = 0 (Min)
    // Fields: Rt=0, Rs=0, Rn=0, size=0
    let encoding: u32 = 0x38A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rn_1_poweroftwo_c000_38a0c020() {
    // Encoding: 0x38A0C020
    // Test aarch64_memory_ordered_rcpc field Rn = 1 (PowerOfTwo)
    // Fields: size=0, Rs=0, Rn=1, Rt=0
    let encoding: u32 = 0x38A0C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rn_30_poweroftwominusone_c000_38a0c3c0() {
    // Encoding: 0x38A0C3C0
    // Test aarch64_memory_ordered_rcpc field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rn=30, Rs=0, Rt=0
    let encoding: u32 = 0x38A0C3C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rn_31_max_c000_38a0c3e0() {
    // Encoding: 0x38A0C3E0
    // Test aarch64_memory_ordered_rcpc field Rn = 31 (Max)
    // Fields: Rs=0, Rn=31, size=0, Rt=0
    let encoding: u32 = 0x38A0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rt_0_min_c000_38a0c000() {
    // Encoding: 0x38A0C000
    // Test aarch64_memory_ordered_rcpc field Rt = 0 (Min)
    // Fields: Rt=0, Rn=0, Rs=0, size=0
    let encoding: u32 = 0x38A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rt_1_poweroftwo_c000_38a0c001() {
    // Encoding: 0x38A0C001
    // Test aarch64_memory_ordered_rcpc field Rt = 1 (PowerOfTwo)
    // Fields: Rt=1, Rn=0, size=0, Rs=0
    let encoding: u32 = 0x38A0C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rt_30_poweroftwominusone_c000_38a0c01e() {
    // Encoding: 0x38A0C01E
    // Test aarch64_memory_ordered_rcpc field Rt = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rn=0, Rs=0, Rt=30
    let encoding: u32 = 0x38A0C01E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rt 0 +: 5`
/// Requirement: FieldBoundary { field: "Rt", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_memory_ordered_rcpc_field_rt_31_max_c000_38a0c01f() {
    // Encoding: 0x38A0C01F
    // Test aarch64_memory_ordered_rcpc field Rt = 31 (Max)
    // Fields: Rs=0, Rn=0, Rt=31, size=0
    let encoding: u32 = 0x38A0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_aarch64_memory_ordered_rcpc_combo_0_c000_38a0c000() {
    // Encoding: 0x38A0C000
    // Test aarch64_memory_ordered_rcpc field combination: size=0, Rs=0, Rn=0, Rt=0
    // Fields: Rn=0, size=0, Rs=0, Rt=0
    let encoding: u32 = 0x38A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_memory_ordered_rcpc_special_size_0_size_variant_0_49152_38a0c000() {
    // Encoding: 0x38A0C000
    // Test aarch64_memory_ordered_rcpc special value size = 0 (Size variant 0)
    // Fields: Rs=0, Rt=0, Rn=0, size=0
    let encoding: u32 = 0x38A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_memory_ordered_rcpc_special_size_1_size_variant_1_49152_78a0c000() {
    // Encoding: 0x78A0C000
    // Test aarch64_memory_ordered_rcpc special value size = 1 (Size variant 1)
    // Fields: Rn=0, Rt=0, size=1, Rs=0
    let encoding: u32 = 0x78A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_memory_ordered_rcpc_special_size_2_size_variant_2_49152_b8a0c000() {
    // Encoding: 0xB8A0C000
    // Test aarch64_memory_ordered_rcpc special value size = 2 (Size variant 2)
    // Fields: size=2, Rn=0, Rs=0, Rt=0
    let encoding: u32 = 0xB8A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_memory_ordered_rcpc_special_size_3_size_variant_3_49152_f8a0c000() {
    // Encoding: 0xF8A0C000
    // Test aarch64_memory_ordered_rcpc special value size = 3 (Size variant 3)
    // Fields: size=3, Rs=0, Rt=0, Rn=0
    let encoding: u32 = 0xF8A0C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_memory_ordered_rcpc_special_rn_31_stack_pointer_sp_may_require_alignment_49152_38a0c3e0() {
    // Encoding: 0x38A0C3E0
    // Test aarch64_memory_ordered_rcpc special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: size=0, Rn=31, Rt=0, Rs=0
    let encoding: u32 = 0x38A0C3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `field Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rt", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_memory_ordered_rcpc_special_rt_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_49152_38a0c01f() {
    // Encoding: 0x38A0C01F
    // Test aarch64_memory_ordered_rcpc special value Rt = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rt=31, Rs=0, size=0, Rn=0
    let encoding: u32 = 0x38A0C01F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `GpFromField("t") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "t" }
/// verify register write to GpFromField("t")
#[test]
fn test_aarch64_memory_ordered_rcpc_reg_write_0_38a0c000() {
    // Test aarch64_memory_ordered_rcpc register write: GpFromField("t")
    // Encoding: 0x38A0C000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x38A0C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_memory_ordered_rcpc_sp_rn_38a0c3e0() {
    // Test aarch64_memory_ordered_rcpc with Rn = SP (31)
    // Encoding: 0x38A0C3E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x38A0C3E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_memory_ordered_rcpc
/// ASL: `Rt = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rt = 31)
#[test]
fn test_aarch64_memory_ordered_rcpc_zr_rt_38a0c01f() {
    // Test aarch64_memory_ordered_rcpc with Rt = ZR (31)
    // Encoding: 0x38A0C01F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x38A0C01F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

