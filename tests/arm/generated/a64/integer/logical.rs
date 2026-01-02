//! A64 integer logical tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_integer_logical_shiftedreg Tests
// ============================================================================

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_sf_0_min_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field sf = 0 (Min)
    // Fields: opc=0, Rm=0, shift=0, imm6=0, sf=0, N=0, Rd=0, Rn=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_sf_1_max_0_8a000000() {
    // Encoding: 0x8A000000
    // Test aarch64_integer_logical_shiftedreg field sf = 1 (Max)
    // Fields: Rd=0, sf=1, imm6=0, opc=0, N=0, Rn=0, Rm=0, shift=0
    let encoding: u32 = 0x8A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_opc_0_min_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field opc = 0 (Min)
    // Fields: Rn=0, N=0, Rm=0, imm6=0, Rd=0, opc=0, sf=0, shift=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_opc_1_poweroftwo_0_2a000000() {
    // Encoding: 0x2A000000
    // Test aarch64_integer_logical_shiftedreg field opc = 1 (PowerOfTwo)
    // Fields: shift=0, sf=0, Rm=0, imm6=0, Rn=0, opc=1, N=0, Rd=0
    let encoding: u32 = 0x2A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_opc_2_poweroftwo_0_4a000000() {
    // Encoding: 0x4A000000
    // Test aarch64_integer_logical_shiftedreg field opc = 2 (PowerOfTwo)
    // Fields: sf=0, opc=2, shift=0, imm6=0, Rd=0, N=0, Rn=0, Rm=0
    let encoding: u32 = 0x4A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_opc_3_max_0_6a000000() {
    // Encoding: 0x6A000000
    // Test aarch64_integer_logical_shiftedreg field opc = 3 (Max)
    // Fields: sf=0, Rm=0, Rn=0, Rd=0, N=0, shift=0, imm6=0, opc=3
    let encoding: u32 = 0x6A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift 22 +: 2`
/// Requirement: FieldBoundary { field: "shift", value: 0, boundary: Min }
/// shift type LSL (logical shift left)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_shift_0_min_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field shift = 0 (Min)
    // Fields: Rd=0, sf=0, imm6=0, opc=0, N=0, Rn=0, shift=0, Rm=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift 22 +: 2`
/// Requirement: FieldBoundary { field: "shift", value: 1, boundary: PowerOfTwo }
/// shift type LSR (logical shift right)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_shift_1_poweroftwo_0_0a400000() {
    // Encoding: 0x0A400000
    // Test aarch64_integer_logical_shiftedreg field shift = 1 (PowerOfTwo)
    // Fields: sf=0, Rm=0, opc=0, Rn=0, N=0, imm6=0, shift=1, Rd=0
    let encoding: u32 = 0x0A400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift 22 +: 2`
/// Requirement: FieldBoundary { field: "shift", value: 2, boundary: PowerOfTwo }
/// shift type ASR (arithmetic shift right)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_shift_2_poweroftwo_0_0a800000() {
    // Encoding: 0x0A800000
    // Test aarch64_integer_logical_shiftedreg field shift = 2 (PowerOfTwo)
    // Fields: shift=2, sf=0, Rd=0, N=0, Rn=0, imm6=0, Rm=0, opc=0
    let encoding: u32 = 0x0A800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift 22 +: 2`
/// Requirement: FieldBoundary { field: "shift", value: 3, boundary: Max }
/// shift type ROR (rotate right)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_shift_3_max_0_0ac00000() {
    // Encoding: 0x0AC00000
    // Test aarch64_integer_logical_shiftedreg field shift = 3 (Max)
    // Fields: N=0, Rn=0, Rm=0, Rd=0, imm6=0, sf=0, opc=0, shift=3
    let encoding: u32 = 0x0AC00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field N 21 +: 1`
/// Requirement: FieldBoundary { field: "N", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_n_0_min_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field N = 0 (Min)
    // Fields: Rm=0, Rn=0, Rd=0, shift=0, imm6=0, sf=0, N=0, opc=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field N 21 +: 1`
/// Requirement: FieldBoundary { field: "N", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_n_1_max_0_0a200000() {
    // Encoding: 0x0A200000
    // Test aarch64_integer_logical_shiftedreg field N = 1 (Max)
    // Fields: opc=0, shift=0, Rn=0, Rm=0, Rd=0, N=1, sf=0, imm6=0
    let encoding: u32 = 0x0A200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rm_0_min_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field Rm = 0 (Min)
    // Fields: imm6=0, sf=0, opc=0, N=0, shift=0, Rn=0, Rd=0, Rm=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rm_1_poweroftwo_0_0a010000() {
    // Encoding: 0x0A010000
    // Test aarch64_integer_logical_shiftedreg field Rm = 1 (PowerOfTwo)
    // Fields: opc=0, Rd=0, sf=0, shift=0, Rm=1, imm6=0, N=0, Rn=0
    let encoding: u32 = 0x0A010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rm_30_poweroftwominusone_0_0a1e0000() {
    // Encoding: 0x0A1E0000
    // Test aarch64_integer_logical_shiftedreg field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: opc=0, Rn=0, shift=0, N=0, sf=0, Rd=0, Rm=30, imm6=0
    let encoding: u32 = 0x0A1E0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rm_31_max_0_0a1f0000() {
    // Encoding: 0x0A1F0000
    // Test aarch64_integer_logical_shiftedreg field Rm = 31 (Max)
    // Fields: N=0, Rm=31, sf=0, Rd=0, opc=0, imm6=0, Rn=0, shift=0
    let encoding: u32 = 0x0A1F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_0_zero_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field imm6 = 0 (Zero)
    // Fields: imm6=0, sf=0, Rm=0, opc=0, shift=0, Rn=0, N=0, Rd=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_1_poweroftwo_0_0a000400() {
    // Encoding: 0x0A000400
    // Test aarch64_integer_logical_shiftedreg field imm6 = 1 (PowerOfTwo)
    // Fields: Rm=0, imm6=1, Rd=0, sf=0, N=0, Rn=0, opc=0, shift=0
    let encoding: u32 = 0x0A000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_3_poweroftwominusone_0_0a000c00() {
    // Encoding: 0x0A000C00
    // Test aarch64_integer_logical_shiftedreg field imm6 = 3 (PowerOfTwoMinusOne)
    // Fields: Rn=0, opc=0, sf=0, Rd=0, shift=0, N=0, Rm=0, imm6=3
    let encoding: u32 = 0x0A000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_4_poweroftwo_0_0a001000() {
    // Encoding: 0x0A001000
    // Test aarch64_integer_logical_shiftedreg field imm6 = 4 (PowerOfTwo)
    // Fields: Rm=0, Rd=0, opc=0, shift=0, N=0, Rn=0, imm6=4, sf=0
    let encoding: u32 = 0x0A001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_7_poweroftwominusone_0_0a001c00() {
    // Encoding: 0x0A001C00
    // Test aarch64_integer_logical_shiftedreg field imm6 = 7 (PowerOfTwoMinusOne)
    // Fields: N=0, shift=0, sf=0, Rm=0, imm6=7, opc=0, Rd=0, Rn=0
    let encoding: u32 = 0x0A001C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_8_poweroftwo_0_0a002000() {
    // Encoding: 0x0A002000
    // Test aarch64_integer_logical_shiftedreg field imm6 = 8 (PowerOfTwo)
    // Fields: shift=0, Rn=0, N=0, opc=0, Rm=0, imm6=8, Rd=0, sf=0
    let encoding: u32 = 0x0A002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_15_poweroftwominusone_0_0a003c00() {
    // Encoding: 0x0A003C00
    // Test aarch64_integer_logical_shiftedreg field imm6 = 15 (PowerOfTwoMinusOne)
    // Fields: opc=0, shift=0, Rd=0, N=0, imm6=15, sf=0, Rn=0, Rm=0
    let encoding: u32 = 0x0A003C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_16_poweroftwo_0_0a004000() {
    // Encoding: 0x0A004000
    // Test aarch64_integer_logical_shiftedreg field imm6 = 16 (PowerOfTwo)
    // Fields: imm6=16, opc=0, N=0, sf=0, Rd=0, Rn=0, Rm=0, shift=0
    let encoding: u32 = 0x0A004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 31, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (31)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_31_poweroftwominusone_0_0a007c00() {
    // Encoding: 0x0A007C00
    // Test aarch64_integer_logical_shiftedreg field imm6 = 31 (PowerOfTwoMinusOne)
    // Fields: shift=0, opc=0, Rd=0, imm6=31, Rn=0, sf=0, N=0, Rm=0
    let encoding: u32 = 0x0A007C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_32_poweroftwo_0_0a008000() {
    // Encoding: 0x0A008000
    // Test aarch64_integer_logical_shiftedreg field imm6 = 32 (PowerOfTwo)
    // Fields: N=0, Rd=0, Rn=0, sf=0, opc=0, imm6=32, Rm=0, shift=0
    let encoding: u32 = 0x0A008000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field imm6 10 +: 6`
/// Requirement: FieldBoundary { field: "imm6", value: 63, boundary: Max }
/// maximum immediate (63)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_imm6_63_max_0_0a00fc00() {
    // Encoding: 0x0A00FC00
    // Test aarch64_integer_logical_shiftedreg field imm6 = 63 (Max)
    // Fields: sf=0, opc=0, shift=0, Rd=0, Rm=0, Rn=0, imm6=63, N=0
    let encoding: u32 = 0x0A00FC00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rn_0_min_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field Rn = 0 (Min)
    // Fields: opc=0, Rn=0, shift=0, Rd=0, Rm=0, sf=0, N=0, imm6=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rn_1_poweroftwo_0_0a000020() {
    // Encoding: 0x0A000020
    // Test aarch64_integer_logical_shiftedreg field Rn = 1 (PowerOfTwo)
    // Fields: Rm=0, opc=0, Rd=0, sf=0, N=0, imm6=0, shift=0, Rn=1
    let encoding: u32 = 0x0A000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rn_30_poweroftwominusone_0_0a0003c0() {
    // Encoding: 0x0A0003C0
    // Test aarch64_integer_logical_shiftedreg field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: N=0, Rn=30, Rd=0, imm6=0, sf=0, Rm=0, opc=0, shift=0
    let encoding: u32 = 0x0A0003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rn_31_max_0_0a0003e0() {
    // Encoding: 0x0A0003E0
    // Test aarch64_integer_logical_shiftedreg field Rn = 31 (Max)
    // Fields: imm6=0, opc=0, Rd=0, Rm=0, sf=0, Rn=31, shift=0, N=0
    let encoding: u32 = 0x0A0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rd_0_min_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field Rd = 0 (Min)
    // Fields: opc=0, shift=0, Rn=0, imm6=0, N=0, Rd=0, sf=0, Rm=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rd_1_poweroftwo_0_0a000001() {
    // Encoding: 0x0A000001
    // Test aarch64_integer_logical_shiftedreg field Rd = 1 (PowerOfTwo)
    // Fields: opc=0, Rd=1, Rn=0, sf=0, N=0, imm6=0, shift=0, Rm=0
    let encoding: u32 = 0x0A000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rd_30_poweroftwominusone_0_0a00001e() {
    // Encoding: 0x0A00001E
    // Test aarch64_integer_logical_shiftedreg field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: imm6=0, Rm=0, shift=0, Rd=30, opc=0, Rn=0, sf=0, N=0
    let encoding: u32 = 0x0A00001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_integer_logical_shiftedreg_field_rd_31_max_0_0a00001f() {
    // Encoding: 0x0A00001F
    // Test aarch64_integer_logical_shiftedreg field Rd = 31 (Max)
    // Fields: Rm=0, N=0, imm6=0, opc=0, shift=0, sf=0, Rn=0, Rd=31
    let encoding: u32 = 0x0A00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// sf=0 (8-bit / byte size)
#[test]
fn test_aarch64_integer_logical_shiftedreg_combo_0_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg field combination: sf=0, opc=0, shift=0, N=0, Rm=0, imm6=0, Rn=0, Rd=0
    // Fields: N=0, opc=0, imm6=0, Rm=0, Rn=0, Rd=0, sf=0, shift=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_sf_0_size_variant_0_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg special value sf = 0 (Size variant 0)
    // Fields: sf=0, opc=0, N=0, shift=0, Rd=0, Rn=0, Rm=0, imm6=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_sf_1_size_variant_1_0_8a000000() {
    // Encoding: 0x8A000000
    // Test aarch64_integer_logical_shiftedreg special value sf = 1 (Size variant 1)
    // Fields: sf=1, Rm=0, shift=0, Rd=0, imm6=0, opc=0, N=0, Rn=0
    let encoding: u32 = 0x8A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "opc", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_opc_0_size_variant_0_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg special value opc = 0 (Size variant 0)
    // Fields: Rn=0, Rm=0, N=0, Rd=0, imm6=0, sf=0, opc=0, shift=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "opc", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_opc_1_size_variant_1_0_2a000000() {
    // Encoding: 0x2A000000
    // Test aarch64_integer_logical_shiftedreg special value opc = 1 (Size variant 1)
    // Fields: sf=0, Rd=0, N=0, Rm=0, imm6=0, opc=1, shift=0, Rn=0
    let encoding: u32 = 0x2A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "opc", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_opc_2_size_variant_2_0_4a000000() {
    // Encoding: 0x4A000000
    // Test aarch64_integer_logical_shiftedreg special value opc = 2 (Size variant 2)
    // Fields: shift=0, Rm=0, Rd=0, sf=0, opc=2, imm6=0, Rn=0, N=0
    let encoding: u32 = 0x4A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field opc = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "opc", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_opc_3_size_variant_3_0_6a000000() {
    // Encoding: 0x6A000000
    // Test aarch64_integer_logical_shiftedreg special value opc = 3 (Size variant 3)
    // Fields: Rd=0, sf=0, opc=3, imm6=0, N=0, Rn=0, Rm=0, shift=0
    let encoding: u32 = 0x6A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift = 0 (Shift type LSL)`
/// Requirement: FieldSpecial { field: "shift", value: 0, meaning: "Shift type LSL" }
/// Shift type LSL
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_shift_0_shift_type_lsl_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg special value shift = 0 (Shift type LSL)
    // Fields: shift=0, imm6=0, opc=0, Rm=0, Rn=0, sf=0, N=0, Rd=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift = 1 (Shift type LSR)`
/// Requirement: FieldSpecial { field: "shift", value: 1, meaning: "Shift type LSR" }
/// Shift type LSR
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_shift_1_shift_type_lsr_0_0a400000() {
    // Encoding: 0x0A400000
    // Test aarch64_integer_logical_shiftedreg special value shift = 1 (Shift type LSR)
    // Fields: Rd=0, opc=0, N=0, sf=0, imm6=0, Rn=0, Rm=0, shift=1
    let encoding: u32 = 0x0A400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift = 2 (Shift type ASR)`
/// Requirement: FieldSpecial { field: "shift", value: 2, meaning: "Shift type ASR" }
/// Shift type ASR
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_shift_2_shift_type_asr_0_0a800000() {
    // Encoding: 0x0A800000
    // Test aarch64_integer_logical_shiftedreg special value shift = 2 (Shift type ASR)
    // Fields: Rm=0, sf=0, Rd=0, imm6=0, Rn=0, opc=0, shift=2, N=0
    let encoding: u32 = 0x0A800000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field shift = 3 (Shift type ROR)`
/// Requirement: FieldSpecial { field: "shift", value: 3, meaning: "Shift type ROR" }
/// Shift type ROR
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_shift_3_shift_type_ror_0_0ac00000() {
    // Encoding: 0x0AC00000
    // Test aarch64_integer_logical_shiftedreg special value shift = 3 (Shift type ROR)
    // Fields: opc=0, shift=3, imm6=0, Rd=0, Rm=0, sf=0, Rn=0, N=0
    let encoding: u32 = 0x0AC00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_rn_31_stack_pointer_sp_may_require_alignment_0_0a0003e0(
) {
    // Encoding: 0x0A0003E0
    // Test aarch64_integer_logical_shiftedreg special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Rd=0, N=0, shift=0, Rn=31, sf=0, Rm=0, opc=0, imm6=0
    let encoding: u32 = 0x0A0003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_integer_logical_shiftedreg_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_0a00001f(
) {
    // Encoding: 0x0A00001F
    // Test aarch64_integer_logical_shiftedreg special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: sf=0, Rd=31, imm6=0, shift=0, opc=0, Rn=0, N=0, Rm=0
    let encoding: u32 = 0x0A00001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "sf" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Index { base: Var(QualifiedIdentifier { qualifier: Any, name: "imm6" }), indices: [Single(LitInt(5))] } } }, rhs: LitBits([true]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"sf\" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Index { base: Var(QualifiedIdentifier { qualifier: Any, name: \"imm6\" }), indices: [Single(LitInt(5))] } } }, rhs: LitBits([true]) }" }
/// triggers Undefined
#[test]
fn test_aarch64_integer_logical_shiftedreg_invalid_0_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg invalid encoding: Binary { op: Eq, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "sf" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Index { base: Var(QualifiedIdentifier { qualifier: Any, name: "imm6" }), indices: [Single(LitInt(5))] } } }, rhs: LitBits([true]) }
    // Fields: Rd=0, Rm=0, N=0, sf=0, Rn=0, shift=0, opc=0, imm6=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_integer_logical_shiftedreg_invalid_1_0_0a000000() {
    // Encoding: 0x0A000000
    // Test aarch64_integer_logical_shiftedreg invalid encoding: Unconditional UNDEFINED
    // Fields: opc=0, Rn=0, Rm=0, sf=0, Rd=0, N=0, imm6=0, shift=0
    let encoding: u32 = 0x0A000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #0`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// no shift (32)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_32_0_0a020020() {
    // Test AND shifted 32-bit: no shift (oracle)
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFF);
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFF, "W0 should be 0x000000FF");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #0`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// no shift (64)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_64_0_8a020020() {
    // Test AND shifted 64-bit: no shift (oracle)
    // Encoding: 0x8A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFF);
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x8A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0xFF, "X0 should be 0x00000000000000FF");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #8`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// LSL #8 (32)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_32_1_0a022020() {
    // Test AND shifted 32-bit: LSL #8 (oracle)
    // Encoding: 0x0A022020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x0A022020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x100, "W0 should be 0x00000100");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #8`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// LSL #8 (64)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_64_1_8a022020() {
    // Test AND shifted 64-bit: LSL #8 (oracle)
    // Encoding: 0x8A022020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x8A022020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x100, "X0 should be 0x0000000000000100");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #8`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// LSR #8 (32)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_32_2_0a422020() {
    // Test AND shifted 32-bit: LSR #8 (oracle)
    // Encoding: 0x0A422020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    set_x(&mut cpu, 2, 0xFF000000);
    let encoding: u32 = 0x0A422020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFF0000, "W0 should be 0x00FF0000");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #8`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// LSR #8 (64)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_64_2_8a422020() {
    // Test AND shifted 64-bit: LSR #8 (oracle)
    // Encoding: 0x8A422020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFF000000);
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x8A422020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0xFF0000, "X0 should be 0x0000000000FF0000");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #4`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// ASR #4 negative (32)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_32_3_0a821020() {
    // Test AND shifted 32-bit: ASR #4 negative (oracle)
    // Encoding: 0x0A821020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    set_x(&mut cpu, 2, 0x80000000);
    let encoding: u32 = 0x0A821020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x80000000, "W0 should be 0x80000000");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #4`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// ASR #4 negative (64)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_64_3_8a821020() {
    // Test AND shifted 64-bit: ASR #4 negative (oracle)
    // Encoding: 0x8A821020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x80000000);
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0x8A821020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #4`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// ROR #4 (32)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_32_4_0ac21020() {
    // Test AND shifted 32-bit: ROR #4 (oracle)
    // Encoding: 0x0AC21020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x12345678);
    set_x(&mut cpu, 2, 0xABCDEF01);
    let encoding: u32 = 0x0AC21020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x12345670, "W0 should be 0x12345670");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `AND X0, X1, X2, shift #4`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// ROR #4 (64)
#[test]
fn test_aarch64_integer_logical_shiftedreg_and_shifted_oracle_64_4_8ac21020() {
    // Test AND shifted 64-bit: ROR #4 (oracle)
    // Encoding: 0x8AC21020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xABCDEF01);
    set_x(&mut cpu, 1, 0x12345678);
    let encoding: u32 = 0x8AC21020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x2345670, "X0 should be 0x0000000002345670");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_integer_logical_shiftedreg_reg_write_0_0a000000() {
    // Test aarch64_integer_logical_shiftedreg register write: GpFromField("d")
    // Encoding: 0x0A000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0A000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_integer_logical_shiftedreg_sp_rn_0a0003e0() {
    // Test aarch64_integer_logical_shiftedreg with Rn = SP (31)
    // Encoding: 0x0A0003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0A0003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_integer_logical_shiftedreg_zr_rd_0a00001f() {
    // Test aarch64_integer_logical_shiftedreg with Rd = ZR (31)
    // Encoding: 0x0A00001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x0A00001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_zeroresult_0_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: ZeroResult
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_zeroresult_1_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: ZeroResult
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_negativeresult_2_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: NegativeResult
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_unsignedoverflow_3_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: UnsignedOverflow
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_unsignedoverflow_4_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: UnsignedOverflow
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_signedoverflow_5_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: SignedOverflow
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_signedoverflow_6_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: SignedOverflow
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: aarch64_integer_logical_shiftedreg
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_aarch64_integer_logical_shiftedreg_flags_positiveresult_7_0a020020() {
    // Test aarch64_integer_logical_shiftedreg flag computation: PositiveResult
    // Encoding: 0x0A020020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x0A020020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// aarch64_integer_logical_immediate Tests
// ============================================================================

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_integer_logical_immediate_field_sf_0_min_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field sf = 0 (Min)
    // Fields: N=0, sf=0, immr=0, imms=0, Rn=0, opc=0, Rd=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field sf 31 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_aarch64_integer_logical_immediate_field_sf_1_max_0_92000000() {
    // Encoding: 0x92000000
    // Test aarch64_integer_logical_immediate field sf = 1 (Max)
    // Fields: sf=1, Rn=0, immr=0, N=0, imms=0, Rd=0, opc=0
    let encoding: u32 = 0x92000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_aarch64_integer_logical_immediate_field_opc_0_min_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field opc = 0 (Min)
    // Fields: opc=0, Rd=0, immr=0, imms=0, sf=0, Rn=0, N=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_aarch64_integer_logical_immediate_field_opc_1_poweroftwo_0_32000000() {
    // Encoding: 0x32000000
    // Test aarch64_integer_logical_immediate field opc = 1 (PowerOfTwo)
    // Fields: Rd=0, sf=0, immr=0, N=0, imms=0, Rn=0, opc=1
    let encoding: u32 = 0x32000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_aarch64_integer_logical_immediate_field_opc_2_poweroftwo_0_52000000() {
    // Encoding: 0x52000000
    // Test aarch64_integer_logical_immediate field opc = 2 (PowerOfTwo)
    // Fields: opc=2, Rn=0, N=0, immr=0, imms=0, Rd=0, sf=0
    let encoding: u32 = 0x52000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc 29 +: 2`
/// Requirement: FieldBoundary { field: "opc", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_aarch64_integer_logical_immediate_field_opc_3_max_0_72000000() {
    // Encoding: 0x72000000
    // Test aarch64_integer_logical_immediate field opc = 3 (Max)
    // Fields: immr=0, sf=0, imms=0, Rn=0, opc=3, N=0, Rd=0
    let encoding: u32 = 0x72000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field N 22 +: 1`
/// Requirement: FieldBoundary { field: "N", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_integer_logical_immediate_field_n_0_min_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field N = 0 (Min)
    // Fields: immr=0, Rn=0, N=0, opc=0, imms=0, Rd=0, sf=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field N 22 +: 1`
/// Requirement: FieldBoundary { field: "N", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_aarch64_integer_logical_immediate_field_n_1_max_0_12400000() {
    // Encoding: 0x12400000
    // Test aarch64_integer_logical_immediate field N = 1 (Max)
    // Fields: imms=0, N=1, Rd=0, opc=0, sf=0, immr=0, Rn=0
    let encoding: u32 = 0x12400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_0_zero_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field immr = 0 (Zero)
    // Fields: Rd=0, sf=0, N=0, immr=0, opc=0, Rn=0, imms=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_1_poweroftwo_0_12010000() {
    // Encoding: 0x12010000
    // Test aarch64_integer_logical_immediate field immr = 1 (PowerOfTwo)
    // Fields: opc=0, immr=1, Rn=0, Rd=0, sf=0, N=0, imms=0
    let encoding: u32 = 0x12010000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_3_poweroftwominusone_0_12030000() {
    // Encoding: 0x12030000
    // Test aarch64_integer_logical_immediate field immr = 3 (PowerOfTwoMinusOne)
    // Fields: sf=0, Rd=0, opc=0, immr=3, N=0, imms=0, Rn=0
    let encoding: u32 = 0x12030000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_4_poweroftwo_0_12040000() {
    // Encoding: 0x12040000
    // Test aarch64_integer_logical_immediate field immr = 4 (PowerOfTwo)
    // Fields: Rd=0, N=0, opc=0, sf=0, immr=4, imms=0, Rn=0
    let encoding: u32 = 0x12040000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_7_poweroftwominusone_0_12070000() {
    // Encoding: 0x12070000
    // Test aarch64_integer_logical_immediate field immr = 7 (PowerOfTwoMinusOne)
    // Fields: Rd=0, Rn=0, sf=0, opc=0, immr=7, N=0, imms=0
    let encoding: u32 = 0x12070000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_8_poweroftwo_0_12080000() {
    // Encoding: 0x12080000
    // Test aarch64_integer_logical_immediate field immr = 8 (PowerOfTwo)
    // Fields: Rn=0, sf=0, Rd=0, N=0, opc=0, imms=0, immr=8
    let encoding: u32 = 0x12080000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_15_poweroftwominusone_0_120f0000() {
    // Encoding: 0x120F0000
    // Test aarch64_integer_logical_immediate field immr = 15 (PowerOfTwoMinusOne)
    // Fields: Rd=0, immr=15, imms=0, N=0, sf=0, Rn=0, opc=0
    let encoding: u32 = 0x120F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_16_poweroftwo_0_12100000() {
    // Encoding: 0x12100000
    // Test aarch64_integer_logical_immediate field immr = 16 (PowerOfTwo)
    // Fields: sf=0, immr=16, opc=0, imms=0, N=0, Rn=0, Rd=0
    let encoding: u32 = 0x12100000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 31, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (31)
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_31_poweroftwominusone_0_121f0000() {
    // Encoding: 0x121F0000
    // Test aarch64_integer_logical_immediate field immr = 31 (PowerOfTwoMinusOne)
    // Fields: immr=31, Rd=0, N=0, opc=0, Rn=0, imms=0, sf=0
    let encoding: u32 = 0x121F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_32_poweroftwo_0_12200000() {
    // Encoding: 0x12200000
    // Test aarch64_integer_logical_immediate field immr = 32 (PowerOfTwo)
    // Fields: immr=32, imms=0, N=0, Rd=0, Rn=0, opc=0, sf=0
    let encoding: u32 = 0x12200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field immr 16 +: 6`
/// Requirement: FieldBoundary { field: "immr", value: 63, boundary: Max }
/// maximum immediate (63)
#[test]
fn test_aarch64_integer_logical_immediate_field_immr_63_max_0_123f0000() {
    // Encoding: 0x123F0000
    // Test aarch64_integer_logical_immediate field immr = 63 (Max)
    // Fields: N=0, Rn=0, opc=0, immr=63, Rd=0, sf=0, imms=0
    let encoding: u32 = 0x123F0000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_0_zero_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field imms = 0 (Zero)
    // Fields: Rn=0, imms=0, N=0, sf=0, Rd=0, immr=0, opc=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_1_poweroftwo_0_12000400() {
    // Encoding: 0x12000400
    // Test aarch64_integer_logical_immediate field imms = 1 (PowerOfTwo)
    // Fields: N=0, immr=0, Rn=0, sf=0, opc=0, imms=1, Rd=0
    let encoding: u32 = 0x12000400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_3_poweroftwominusone_0_12000c00() {
    // Encoding: 0x12000C00
    // Test aarch64_integer_logical_immediate field imms = 3 (PowerOfTwoMinusOne)
    // Fields: opc=0, immr=0, Rd=0, imms=3, Rn=0, sf=0, N=0
    let encoding: u32 = 0x12000C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_4_poweroftwo_0_12001000() {
    // Encoding: 0x12001000
    // Test aarch64_integer_logical_immediate field imms = 4 (PowerOfTwo)
    // Fields: sf=0, opc=0, immr=0, Rn=0, N=0, Rd=0, imms=4
    let encoding: u32 = 0x12001000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_7_poweroftwominusone_0_12001c00() {
    // Encoding: 0x12001C00
    // Test aarch64_integer_logical_immediate field imms = 7 (PowerOfTwoMinusOne)
    // Fields: opc=0, sf=0, N=0, immr=0, imms=7, Rd=0, Rn=0
    let encoding: u32 = 0x12001C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_8_poweroftwo_0_12002000() {
    // Encoding: 0x12002000
    // Test aarch64_integer_logical_immediate field imms = 8 (PowerOfTwo)
    // Fields: sf=0, Rd=0, imms=8, N=0, Rn=0, immr=0, opc=0
    let encoding: u32 = 0x12002000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_15_poweroftwominusone_0_12003c00() {
    // Encoding: 0x12003C00
    // Test aarch64_integer_logical_immediate field imms = 15 (PowerOfTwoMinusOne)
    // Fields: immr=0, Rn=0, Rd=0, opc=0, imms=15, N=0, sf=0
    let encoding: u32 = 0x12003C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_16_poweroftwo_0_12004000() {
    // Encoding: 0x12004000
    // Test aarch64_integer_logical_immediate field imms = 16 (PowerOfTwo)
    // Fields: Rn=0, Rd=0, sf=0, N=0, opc=0, immr=0, imms=16
    let encoding: u32 = 0x12004000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 31, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (31)
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_31_poweroftwominusone_0_12007c00() {
    // Encoding: 0x12007C00
    // Test aarch64_integer_logical_immediate field imms = 31 (PowerOfTwoMinusOne)
    // Fields: immr=0, N=0, Rd=0, opc=0, Rn=0, imms=31, sf=0
    let encoding: u32 = 0x12007C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_32_poweroftwo_0_12008000() {
    // Encoding: 0x12008000
    // Test aarch64_integer_logical_immediate field imms = 32 (PowerOfTwo)
    // Fields: N=0, imms=32, Rd=0, immr=0, Rn=0, opc=0, sf=0
    let encoding: u32 = 0x12008000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field imms 10 +: 6`
/// Requirement: FieldBoundary { field: "imms", value: 63, boundary: Max }
/// maximum immediate (63)
#[test]
fn test_aarch64_integer_logical_immediate_field_imms_63_max_0_1200fc00() {
    // Encoding: 0x1200FC00
    // Test aarch64_integer_logical_immediate field imms = 63 (Max)
    // Fields: sf=0, opc=0, N=0, immr=0, imms=63, Rn=0, Rd=0
    let encoding: u32 = 0x1200FC00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_logical_immediate_field_rn_0_min_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field Rn = 0 (Min)
    // Fields: sf=0, imms=0, immr=0, opc=0, Rn=0, N=0, Rd=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_logical_immediate_field_rn_1_poweroftwo_0_12000020() {
    // Encoding: 0x12000020
    // Test aarch64_integer_logical_immediate field Rn = 1 (PowerOfTwo)
    // Fields: opc=0, immr=0, Rd=0, imms=0, sf=0, N=0, Rn=1
    let encoding: u32 = 0x12000020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_logical_immediate_field_rn_30_poweroftwominusone_0_120003c0() {
    // Encoding: 0x120003C0
    // Test aarch64_integer_logical_immediate field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: N=0, immr=0, imms=0, Rn=30, opc=0, sf=0, Rd=0
    let encoding: u32 = 0x120003C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_aarch64_integer_logical_immediate_field_rn_31_max_0_120003e0() {
    // Encoding: 0x120003E0
    // Test aarch64_integer_logical_immediate field Rn = 31 (Max)
    // Fields: imms=0, immr=0, N=0, opc=0, sf=0, Rn=31, Rd=0
    let encoding: u32 = 0x120003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_aarch64_integer_logical_immediate_field_rd_0_min_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field Rd = 0 (Min)
    // Fields: imms=0, opc=0, N=0, immr=0, Rn=0, Rd=0, sf=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_aarch64_integer_logical_immediate_field_rd_1_poweroftwo_0_12000001() {
    // Encoding: 0x12000001
    // Test aarch64_integer_logical_immediate field Rd = 1 (PowerOfTwo)
    // Fields: imms=0, Rd=1, sf=0, N=0, immr=0, opc=0, Rn=0
    let encoding: u32 = 0x12000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_aarch64_integer_logical_immediate_field_rd_30_poweroftwominusone_0_1200001e() {
    // Encoding: 0x1200001E
    // Test aarch64_integer_logical_immediate field Rd = 30 (PowerOfTwoMinusOne)
    // Fields: sf=0, N=0, opc=0, immr=0, imms=0, Rn=0, Rd=30
    let encoding: u32 = 0x1200001E;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rd 0 +: 5`
/// Requirement: FieldBoundary { field: "Rd", value: 31, boundary: Max }
/// register index 31 (ZR - zero register)
#[test]
fn test_aarch64_integer_logical_immediate_field_rd_31_max_0_1200001f() {
    // Encoding: 0x1200001F
    // Test aarch64_integer_logical_immediate field Rd = 31 (Max)
    // Fields: Rn=0, sf=0, Rd=31, N=0, immr=0, opc=0, imms=0
    let encoding: u32 = 0x1200001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// sf=0 (8-bit / byte size)
#[test]
fn test_aarch64_integer_logical_immediate_combo_0_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate field combination: sf=0, opc=0, N=0, immr=0, imms=0, Rn=0, Rd=0
    // Fields: Rd=0, imms=0, Rn=0, sf=0, immr=0, N=0, opc=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_integer_logical_immediate_special_sf_0_size_variant_0_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate special value sf = 0 (Size variant 0)
    // Fields: Rd=0, opc=0, sf=0, N=0, immr=0, imms=0, Rn=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_integer_logical_immediate_special_sf_1_size_variant_1_0_92000000() {
    // Encoding: 0x92000000
    // Test aarch64_integer_logical_immediate special value sf = 1 (Size variant 1)
    // Fields: sf=1, Rn=0, Rd=0, immr=0, N=0, opc=0, imms=0
    let encoding: u32 = 0x92000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "opc", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_aarch64_integer_logical_immediate_special_opc_0_size_variant_0_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate special value opc = 0 (Size variant 0)
    // Fields: Rn=0, Rd=0, opc=0, sf=0, N=0, immr=0, imms=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "opc", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_aarch64_integer_logical_immediate_special_opc_1_size_variant_1_0_32000000() {
    // Encoding: 0x32000000
    // Test aarch64_integer_logical_immediate special value opc = 1 (Size variant 1)
    // Fields: sf=0, Rn=0, N=0, Rd=0, imms=0, immr=0, opc=1
    let encoding: u32 = 0x32000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "opc", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_aarch64_integer_logical_immediate_special_opc_2_size_variant_2_0_52000000() {
    // Encoding: 0x52000000
    // Test aarch64_integer_logical_immediate special value opc = 2 (Size variant 2)
    // Fields: opc=2, N=0, immr=0, imms=0, Rn=0, Rd=0, sf=0
    let encoding: u32 = 0x52000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field opc = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "opc", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_aarch64_integer_logical_immediate_special_opc_3_size_variant_3_0_72000000() {
    // Encoding: 0x72000000
    // Test aarch64_integer_logical_immediate special value opc = 3 (Size variant 3)
    // Fields: sf=0, opc=3, Rd=0, Rn=0, imms=0, N=0, immr=0
    let encoding: u32 = 0x72000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_aarch64_integer_logical_immediate_special_rn_31_stack_pointer_sp_may_require_alignment_0_120003e0(
) {
    // Encoding: 0x120003E0
    // Test aarch64_integer_logical_immediate special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: sf=0, N=0, Rn=31, Rd=0, opc=0, immr=0, imms=0
    let encoding: u32 = 0x120003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `field Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)`
/// Requirement: FieldSpecial { field: "Rd", value: 31, meaning: "Zero register (XZR/WZR) - reads as 0, writes discarded" }
/// Zero register (XZR/WZR) - reads as 0, writes discarded
#[test]
fn test_aarch64_integer_logical_immediate_special_rd_31_zero_register_xzr_wzr_reads_as_0_writes_discarded_0_1200001f(
) {
    // Encoding: 0x1200001F
    // Test aarch64_integer_logical_immediate special value Rd = 31 (Zero register (XZR/WZR) - reads as 0, writes discarded)
    // Fields: Rd=31, imms=0, sf=0, opc=0, immr=0, Rn=0, N=0
    let encoding: u32 = 0x1200001F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `Binary { op: Ne, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "sf" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Var(QualifiedIdentifier { qualifier: Any, name: "N" }) } }, rhs: LitBits([false]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Ne, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"sf\" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Var(QualifiedIdentifier { qualifier: Any, name: \"N\" }) } }, rhs: LitBits([false]) }" }
/// triggers Undefined
#[test]
fn test_aarch64_integer_logical_immediate_invalid_0_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate invalid encoding: Binary { op: Ne, lhs: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "sf" }), rhs: Binary { op: And, lhs: LitBits([false]), rhs: Var(QualifiedIdentifier { qualifier: Any, name: "N" }) } }, rhs: LitBits([false]) }
    // Fields: N=0, imms=0, opc=0, Rd=0, immr=0, sf=0, Rn=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_integer_logical_immediate_invalid_1_0_12000000() {
    // Encoding: 0x12000000
    // Test aarch64_integer_logical_immediate invalid encoding: Unconditional UNDEFINED
    // Fields: sf=0, imms=0, immr=0, Rn=0, Rd=0, opc=0, N=0
    let encoding: u32 = 0x12000000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND X0, X1, #0xFF`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mask lower 8 bits (64)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_64_0_92401c20() {
    // Test AND 64-bit: mask lower 8 bits (oracle)
    // Encoding: 0x92401C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x92401C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0xFF, "X0 should be 0x00000000000000FF");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND X0, X1, #0xFFFF`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mask lower 16 bits (64)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_64_1_92403c20() {
    // Test AND 64-bit: mask lower 16 bits (oracle)
    // Encoding: 0x92403C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x92403C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0xFFFF, "X0 should be 0x000000000000FFFF");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND X0, X1, #0xFFFFFFFF`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mask lower 32 bits (64)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_64_2_92407c20() {
    // Test AND 64-bit: mask lower 32 bits (oracle)
    // Encoding: 0x92407C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x92407C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFF,
        "X0 should be 0x00000000FFFFFFFF"
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND X0, X1, #0x1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit mask (64)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_64_3_92400020() {
    // Test AND 64-bit: single bit mask (oracle)
    // Encoding: 0x92400020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0x92400020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND X0, X1, #0x7FFFFFFFFFFFFFFF`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// all but MSB (64)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_64_4_9240f820() {
    // Test AND 64-bit: all but MSB (oracle)
    // Encoding: 0x9240F820
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xAAAAAAAAAAAAAAAA);
    let encoding: u32 = 0x9240F820;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x2AAAAAAAAAAAAAAA,
        "X0 should be 0x2AAAAAAAAAAAAAAA"
    );
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND W0, W1, #0xFF`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mask lower 8 bits (32)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_32_0_12001c20() {
    // Test AND 32-bit: mask lower 8 bits (oracle)
    // Encoding: 0x12001C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x12001C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFF, "W0 should be 0x000000FF");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND W0, W1, #0xFFFF`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mask lower 16 bits (32)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_32_1_12003c20() {
    // Test AND 32-bit: mask lower 16 bits (oracle)
    // Encoding: 0x12003C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x12003C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFFFF, "W0 should be 0x0000FFFF");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `AND W0, W1, #0x1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit mask (32)
#[test]
fn test_aarch64_integer_logical_immediate_and_oracle_32_2_12000020() {
    // Test AND 32-bit: single bit mask (oracle)
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEF);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x1, "W0 should be 0x00000001");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `Sp write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to Sp
#[test]
fn test_aarch64_integer_logical_immediate_reg_write_0_12000000() {
    // Test aarch64_integer_logical_immediate register write: Sp
    // Encoding: 0x12000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x12000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `GpFromField("d") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "d" }
/// verify register write to GpFromField("d")
#[test]
fn test_aarch64_integer_logical_immediate_reg_write_1_12000000() {
    // Test aarch64_integer_logical_immediate register write: GpFromField("d")
    // Encoding: 0x12000000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x12000000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_aarch64_integer_logical_immediate_sp_rn_120003e0() {
    // Test aarch64_integer_logical_immediate with Rn = SP (31)
    // Encoding: 0x120003E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x120003E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `Rd = 31 (ZR)`
/// Requirement: RegisterSpecial { reg: Zr, behavior: "reads as 0, writes discarded" }
/// zero register (Rd = 31)
#[test]
fn test_aarch64_integer_logical_immediate_zr_rd_1200001f() {
    // Test aarch64_integer_logical_immediate with Rd = ZR (31)
    // Encoding: 0x1200001F
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x1200001F;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 31), 0, "XZR should always be 0");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_aarch64_integer_logical_immediate_flags_zeroresult_0_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: ZeroResult
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_aarch64_integer_logical_immediate_flags_zeroresult_1_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: ZeroResult
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_aarch64_integer_logical_immediate_flags_negativeresult_2_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: NegativeResult
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_aarch64_integer_logical_immediate_flags_unsignedoverflow_3_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: UnsignedOverflow
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_aarch64_integer_logical_immediate_flags_unsignedoverflow_4_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: UnsignedOverflow
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_aarch64_integer_logical_immediate_flags_signedoverflow_5_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: SignedOverflow
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_aarch64_integer_logical_immediate_flags_signedoverflow_6_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: SignedOverflow
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: aarch64_integer_logical_immediate
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_aarch64_integer_logical_immediate_flags_positiveresult_7_12000020() {
    // Test aarch64_integer_logical_immediate flag computation: PositiveResult
    // Encoding: 0x12000020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x12000020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}
