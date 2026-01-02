//! A64 system exceptions tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// aarch64_system_exceptions_runtime_smc Tests
// ============================================================================

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_0_zero_3_d4000003() {
    // Encoding: 0xD4000003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 0 (Zero)
    // Fields: imm16=0
    let encoding: u32 = 0xD4000003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_1_poweroftwo_3_d4000023() {
    // Encoding: 0xD4000023
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 1 (PowerOfTwo)
    // Fields: imm16=1
    let encoding: u32 = 0xD4000023;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_3_poweroftwominusone_3_d4000063() {
    // Encoding: 0xD4000063
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 3 (PowerOfTwoMinusOne)
    // Fields: imm16=3
    let encoding: u32 = 0xD4000063;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_4_poweroftwo_3_d4000083() {
    // Encoding: 0xD4000083
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 4 (PowerOfTwo)
    // Fields: imm16=4
    let encoding: u32 = 0xD4000083;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_7_poweroftwominusone_3_d40000e3() {
    // Encoding: 0xD40000E3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 7 (PowerOfTwoMinusOne)
    // Fields: imm16=7
    let encoding: u32 = 0xD40000E3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_8_poweroftwo_3_d4000103() {
    // Encoding: 0xD4000103
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 8 (PowerOfTwo)
    // Fields: imm16=8
    let encoding: u32 = 0xD4000103;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_15_poweroftwominusone_3_d40001e3() {
    // Encoding: 0xD40001E3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 15 (PowerOfTwoMinusOne)
    // Fields: imm16=15
    let encoding: u32 = 0xD40001E3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_16_poweroftwo_3_d4000203() {
    // Encoding: 0xD4000203
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 16 (PowerOfTwo)
    // Fields: imm16=16
    let encoding: u32 = 0xD4000203;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_31_poweroftwominusone_3_d40003e3() {
    // Encoding: 0xD40003E3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 31 (PowerOfTwoMinusOne)
    // Fields: imm16=31
    let encoding: u32 = 0xD40003E3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_32_poweroftwo_3_d4000403() {
    // Encoding: 0xD4000403
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 32 (PowerOfTwo)
    // Fields: imm16=32
    let encoding: u32 = 0xD4000403;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_63_poweroftwominusone_3_d40007e3() {
    // Encoding: 0xD40007E3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 63 (PowerOfTwoMinusOne)
    // Fields: imm16=63
    let encoding: u32 = 0xD40007E3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_64_poweroftwo_3_d4000803() {
    // Encoding: 0xD4000803
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 64 (PowerOfTwo)
    // Fields: imm16=64
    let encoding: u32 = 0xD4000803;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_127_poweroftwominusone_3_d4000fe3() {
    // Encoding: 0xD4000FE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 127 (PowerOfTwoMinusOne)
    // Fields: imm16=127
    let encoding: u32 = 0xD4000FE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_128_poweroftwo_3_d4001003() {
    // Encoding: 0xD4001003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 128 (PowerOfTwo)
    // Fields: imm16=128
    let encoding: u32 = 0xD4001003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_255_poweroftwominusone_3_d4001fe3() {
    // Encoding: 0xD4001FE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 255 (PowerOfTwoMinusOne)
    // Fields: imm16=255
    let encoding: u32 = 0xD4001FE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_256_poweroftwo_3_d4002003() {
    // Encoding: 0xD4002003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 256 (PowerOfTwo)
    // Fields: imm16=256
    let encoding: u32 = 0xD4002003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_511_poweroftwominusone_3_d4003fe3() {
    // Encoding: 0xD4003FE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 511 (PowerOfTwoMinusOne)
    // Fields: imm16=511
    let encoding: u32 = 0xD4003FE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_512_poweroftwo_3_d4004003() {
    // Encoding: 0xD4004003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 512 (PowerOfTwo)
    // Fields: imm16=512
    let encoding: u32 = 0xD4004003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_1023_poweroftwominusone_3_d4007fe3() {
    // Encoding: 0xD4007FE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 1023 (PowerOfTwoMinusOne)
    // Fields: imm16=1023
    let encoding: u32 = 0xD4007FE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_1024_poweroftwo_3_d4008003() {
    // Encoding: 0xD4008003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 1024 (PowerOfTwo)
    // Fields: imm16=1024
    let encoding: u32 = 0xD4008003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_2047_poweroftwominusone_3_d400ffe3() {
    // Encoding: 0xD400FFE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 2047 (PowerOfTwoMinusOne)
    // Fields: imm16=2047
    let encoding: u32 = 0xD400FFE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_2048_poweroftwo_3_d4010003() {
    // Encoding: 0xD4010003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 2048 (PowerOfTwo)
    // Fields: imm16=2048
    let encoding: u32 = 0xD4010003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4095, boundary: PowerOfTwoMinusOne }
/// 2^12 - 1 = 4095
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_4095_poweroftwominusone_3_d401ffe3() {
    // Encoding: 0xD401FFE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm16=4095
    let encoding: u32 = 0xD401FFE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_4096_poweroftwo_3_d4020003() {
    // Encoding: 0xD4020003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 4096 (PowerOfTwo)
    // Fields: imm16=4096
    let encoding: u32 = 0xD4020003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8191, boundary: PowerOfTwoMinusOne }
/// 2^13 - 1 = 8191
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_8191_poweroftwominusone_3_d403ffe3() {
    // Encoding: 0xD403FFE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 8191 (PowerOfTwoMinusOne)
    // Fields: imm16=8191
    let encoding: u32 = 0xD403FFE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8192, boundary: PowerOfTwo }
/// power of 2 (2^13 = 8192)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_8192_poweroftwo_3_d4040003() {
    // Encoding: 0xD4040003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 8192 (PowerOfTwo)
    // Fields: imm16=8192
    let encoding: u32 = 0xD4040003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16383, boundary: PowerOfTwoMinusOne }
/// 2^14 - 1 = 16383
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_16383_poweroftwominusone_3_d407ffe3() {
    // Encoding: 0xD407FFE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 16383 (PowerOfTwoMinusOne)
    // Fields: imm16=16383
    let encoding: u32 = 0xD407FFE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16384, boundary: PowerOfTwo }
/// power of 2 (2^14 = 16384)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_16384_poweroftwo_3_d4080003() {
    // Encoding: 0xD4080003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 16384 (PowerOfTwo)
    // Fields: imm16=16384
    let encoding: u32 = 0xD4080003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32767, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (32767)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_32767_poweroftwominusone_3_d40fffe3() {
    // Encoding: 0xD40FFFE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 32767 (PowerOfTwoMinusOne)
    // Fields: imm16=32767
    let encoding: u32 = 0xD40FFFE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32768, boundary: PowerOfTwo }
/// power of 2 (2^15 = 32768)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_32768_poweroftwo_3_d4100003() {
    // Encoding: 0xD4100003
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 32768 (PowerOfTwo)
    // Fields: imm16=32768
    let encoding: u32 = 0xD4100003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 65535, boundary: Max }
/// maximum immediate (65535)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_field_imm16_65535_max_3_d41fffe3() {
    // Encoding: 0xD41FFFE3
    // Test aarch64_system_exceptions_runtime_smc field imm16 = 65535 (Max)
    // Fields: imm16=65535
    let encoding: u32 = 0xD41FFFE3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=0 (immediate value 0)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_0_3_d4000003() {
    // Encoding: 0xD4000003
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=0
    // Fields: imm16=0
    let encoding: u32 = 0xD4000003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=1 (immediate value 1)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_1_3_d4000023() {
    // Encoding: 0xD4000023
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=1
    // Fields: imm16=1
    let encoding: u32 = 0xD4000023;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=3 (2^2 - 1 = 3)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_2_3_d4000063() {
    // Encoding: 0xD4000063
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=3
    // Fields: imm16=3
    let encoding: u32 = 0xD4000063;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=4 (power of 2 (2^2 = 4))
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_3_3_d4000083() {
    // Encoding: 0xD4000083
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=4
    // Fields: imm16=4
    let encoding: u32 = 0xD4000083;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=7 (2^3 - 1 = 7)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_4_3_d40000e3() {
    // Encoding: 0xD40000E3
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=7
    // Fields: imm16=7
    let encoding: u32 = 0xD40000E3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=8 (power of 2 (2^3 = 8))
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_5_3_d4000103() {
    // Encoding: 0xD4000103
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=8
    // Fields: imm16=8
    let encoding: u32 = 0xD4000103;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=15 (2^4 - 1 = 15)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_6_3_d40001e3() {
    // Encoding: 0xD40001E3
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=15
    // Fields: imm16=15
    let encoding: u32 = 0xD40001E3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=16 (power of 2 (2^4 = 16))
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_7_3_d4000203() {
    // Encoding: 0xD4000203
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=16
    // Fields: imm16=16
    let encoding: u32 = 0xD4000203;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=31 (2^5 - 1 = 31)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_8_3_d40003e3() {
    // Encoding: 0xD40003E3
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=31
    // Fields: imm16=31
    let encoding: u32 = 0xD40003E3;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=32 (power of 2 (2^5 = 32))
#[test]
fn test_aarch64_system_exceptions_runtime_smc_combo_9_3_d4000403() {
    // Encoding: 0xD4000403
    // Test aarch64_system_exceptions_runtime_smc field combination: imm16=32
    // Fields: imm16=32
    let encoding: u32 = 0xD4000403;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// zero value (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_0_5ac00020() {
    // Test RBIT 32-bit: zero value (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// zero value (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_0_dac00020() {
    // Test RBIT 64-bit: zero value (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (LSB) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_1_5ac00020() {
    // Test RBIT 32-bit: single bit set (LSB) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x80000000, "W0 should be 0x80000000");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (LSB) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_1_dac00020() {
    // Test RBIT 64-bit: single bit set (LSB) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x8000000000000000,
        "X0 should be 0x8000000000000000"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 64) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_2_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 64) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_2_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 32) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_3_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x1, "W0 should be 0x00000001");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 32) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_3_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x100000000,
        "X0 should be 0x0000000100000000"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// all ones (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_4_5ac00020() {
    // Test RBIT 32-bit: all ones (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFFFFFFFF, "W0 should be 0xFFFFFFFF");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// all ones (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_4_dac00020() {
    // Test RBIT 64-bit: all ones (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFF,
        "X0 should be 0xFFFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mixed pattern (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_5_5ac00020() {
    // Test RBIT 32-bit: mixed pattern (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x9ABCDEF0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF7B3D59, "W0 should be 0x0F7B3D59");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mixed pattern (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_5_dac00020() {
    // Test RBIT 64-bit: mixed pattern (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x123456789ABCDEF0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF7B3D591E6A2C48,
        "X0 should be 0x0F7B3D591E6A2C48"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// alternating nibbles (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_6_5ac00020() {
    // Test RBIT 32-bit: alternating nibbles (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF0F0F0F0, "W0 should be 0xF0F0F0F0");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// alternating nibbles (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_6_dac00020() {
    // Test RBIT 64-bit: alternating nibbles (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F0F0F0F0F);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF0F0F0F0F0F0F0F0,
        "X0 should be 0xF0F0F0F0F0F0F0F0"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// magic values (32)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_32_7_5ac00020() {
    // Test RBIT 32-bit: magic values (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xCAFEBABE);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x7D5D7F53, "W0 should be 0x7D5D7F53");
}

/// Provenance: aarch64_system_exceptions_runtime_smc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// magic values (64)
#[test]
fn test_aarch64_system_exceptions_runtime_smc_rbit_oracle_64_7_dac00020() {
    // Test RBIT 64-bit: magic values (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x7D5D7F53F77DB57B,
        "X0 should be 0x7D5D7F53F77DB57B"
    );
}

// ============================================================================
// aarch64_system_exceptions_debug_halt Tests
// ============================================================================

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_0_zero_0_d4400000() {
    // Encoding: 0xD4400000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 0 (Zero)
    // Fields: imm16=0
    let encoding: u32 = 0xD4400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_1_poweroftwo_0_d4400020() {
    // Encoding: 0xD4400020
    // Test aarch64_system_exceptions_debug_halt field imm16 = 1 (PowerOfTwo)
    // Fields: imm16=1
    let encoding: u32 = 0xD4400020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_3_poweroftwominusone_0_d4400060() {
    // Encoding: 0xD4400060
    // Test aarch64_system_exceptions_debug_halt field imm16 = 3 (PowerOfTwoMinusOne)
    // Fields: imm16=3
    let encoding: u32 = 0xD4400060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_4_poweroftwo_0_d4400080() {
    // Encoding: 0xD4400080
    // Test aarch64_system_exceptions_debug_halt field imm16 = 4 (PowerOfTwo)
    // Fields: imm16=4
    let encoding: u32 = 0xD4400080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_7_poweroftwominusone_0_d44000e0() {
    // Encoding: 0xD44000E0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 7 (PowerOfTwoMinusOne)
    // Fields: imm16=7
    let encoding: u32 = 0xD44000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_8_poweroftwo_0_d4400100() {
    // Encoding: 0xD4400100
    // Test aarch64_system_exceptions_debug_halt field imm16 = 8 (PowerOfTwo)
    // Fields: imm16=8
    let encoding: u32 = 0xD4400100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_15_poweroftwominusone_0_d44001e0() {
    // Encoding: 0xD44001E0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 15 (PowerOfTwoMinusOne)
    // Fields: imm16=15
    let encoding: u32 = 0xD44001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_16_poweroftwo_0_d4400200() {
    // Encoding: 0xD4400200
    // Test aarch64_system_exceptions_debug_halt field imm16 = 16 (PowerOfTwo)
    // Fields: imm16=16
    let encoding: u32 = 0xD4400200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_31_poweroftwominusone_0_d44003e0() {
    // Encoding: 0xD44003E0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 31 (PowerOfTwoMinusOne)
    // Fields: imm16=31
    let encoding: u32 = 0xD44003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_32_poweroftwo_0_d4400400() {
    // Encoding: 0xD4400400
    // Test aarch64_system_exceptions_debug_halt field imm16 = 32 (PowerOfTwo)
    // Fields: imm16=32
    let encoding: u32 = 0xD4400400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_63_poweroftwominusone_0_d44007e0() {
    // Encoding: 0xD44007E0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 63 (PowerOfTwoMinusOne)
    // Fields: imm16=63
    let encoding: u32 = 0xD44007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_64_poweroftwo_0_d4400800() {
    // Encoding: 0xD4400800
    // Test aarch64_system_exceptions_debug_halt field imm16 = 64 (PowerOfTwo)
    // Fields: imm16=64
    let encoding: u32 = 0xD4400800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_127_poweroftwominusone_0_d4400fe0() {
    // Encoding: 0xD4400FE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 127 (PowerOfTwoMinusOne)
    // Fields: imm16=127
    let encoding: u32 = 0xD4400FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_128_poweroftwo_0_d4401000() {
    // Encoding: 0xD4401000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 128 (PowerOfTwo)
    // Fields: imm16=128
    let encoding: u32 = 0xD4401000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_255_poweroftwominusone_0_d4401fe0() {
    // Encoding: 0xD4401FE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 255 (PowerOfTwoMinusOne)
    // Fields: imm16=255
    let encoding: u32 = 0xD4401FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_256_poweroftwo_0_d4402000() {
    // Encoding: 0xD4402000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 256 (PowerOfTwo)
    // Fields: imm16=256
    let encoding: u32 = 0xD4402000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_511_poweroftwominusone_0_d4403fe0() {
    // Encoding: 0xD4403FE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 511 (PowerOfTwoMinusOne)
    // Fields: imm16=511
    let encoding: u32 = 0xD4403FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_512_poweroftwo_0_d4404000() {
    // Encoding: 0xD4404000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 512 (PowerOfTwo)
    // Fields: imm16=512
    let encoding: u32 = 0xD4404000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_1023_poweroftwominusone_0_d4407fe0() {
    // Encoding: 0xD4407FE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 1023 (PowerOfTwoMinusOne)
    // Fields: imm16=1023
    let encoding: u32 = 0xD4407FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_1024_poweroftwo_0_d4408000() {
    // Encoding: 0xD4408000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 1024 (PowerOfTwo)
    // Fields: imm16=1024
    let encoding: u32 = 0xD4408000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_2047_poweroftwominusone_0_d440ffe0() {
    // Encoding: 0xD440FFE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 2047 (PowerOfTwoMinusOne)
    // Fields: imm16=2047
    let encoding: u32 = 0xD440FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_2048_poweroftwo_0_d4410000() {
    // Encoding: 0xD4410000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 2048 (PowerOfTwo)
    // Fields: imm16=2048
    let encoding: u32 = 0xD4410000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4095, boundary: PowerOfTwoMinusOne }
/// 2^12 - 1 = 4095
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_4095_poweroftwominusone_0_d441ffe0() {
    // Encoding: 0xD441FFE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm16=4095
    let encoding: u32 = 0xD441FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_4096_poweroftwo_0_d4420000() {
    // Encoding: 0xD4420000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 4096 (PowerOfTwo)
    // Fields: imm16=4096
    let encoding: u32 = 0xD4420000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8191, boundary: PowerOfTwoMinusOne }
/// 2^13 - 1 = 8191
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_8191_poweroftwominusone_0_d443ffe0() {
    // Encoding: 0xD443FFE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 8191 (PowerOfTwoMinusOne)
    // Fields: imm16=8191
    let encoding: u32 = 0xD443FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8192, boundary: PowerOfTwo }
/// power of 2 (2^13 = 8192)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_8192_poweroftwo_0_d4440000() {
    // Encoding: 0xD4440000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 8192 (PowerOfTwo)
    // Fields: imm16=8192
    let encoding: u32 = 0xD4440000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16383, boundary: PowerOfTwoMinusOne }
/// 2^14 - 1 = 16383
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_16383_poweroftwominusone_0_d447ffe0() {
    // Encoding: 0xD447FFE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 16383 (PowerOfTwoMinusOne)
    // Fields: imm16=16383
    let encoding: u32 = 0xD447FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16384, boundary: PowerOfTwo }
/// power of 2 (2^14 = 16384)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_16384_poweroftwo_0_d4480000() {
    // Encoding: 0xD4480000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 16384 (PowerOfTwo)
    // Fields: imm16=16384
    let encoding: u32 = 0xD4480000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32767, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (32767)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_32767_poweroftwominusone_0_d44fffe0() {
    // Encoding: 0xD44FFFE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 32767 (PowerOfTwoMinusOne)
    // Fields: imm16=32767
    let encoding: u32 = 0xD44FFFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32768, boundary: PowerOfTwo }
/// power of 2 (2^15 = 32768)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_32768_poweroftwo_0_d4500000() {
    // Encoding: 0xD4500000
    // Test aarch64_system_exceptions_debug_halt field imm16 = 32768 (PowerOfTwo)
    // Fields: imm16=32768
    let encoding: u32 = 0xD4500000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 65535, boundary: Max }
/// maximum immediate (65535)
#[test]
fn test_aarch64_system_exceptions_debug_halt_field_imm16_65535_max_0_d45fffe0() {
    // Encoding: 0xD45FFFE0
    // Test aarch64_system_exceptions_debug_halt field imm16 = 65535 (Max)
    // Fields: imm16=65535
    let encoding: u32 = 0xD45FFFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=0 (immediate value 0)
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_0_0_d4400000() {
    // Encoding: 0xD4400000
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=0
    // Fields: imm16=0
    let encoding: u32 = 0xD4400000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=1 (immediate value 1)
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_1_0_d4400020() {
    // Encoding: 0xD4400020
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=1
    // Fields: imm16=1
    let encoding: u32 = 0xD4400020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=3 (2^2 - 1 = 3)
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_2_0_d4400060() {
    // Encoding: 0xD4400060
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=3
    // Fields: imm16=3
    let encoding: u32 = 0xD4400060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=4 (power of 2 (2^2 = 4))
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_3_0_d4400080() {
    // Encoding: 0xD4400080
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=4
    // Fields: imm16=4
    let encoding: u32 = 0xD4400080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=7 (2^3 - 1 = 7)
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_4_0_d44000e0() {
    // Encoding: 0xD44000E0
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=7
    // Fields: imm16=7
    let encoding: u32 = 0xD44000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=8 (power of 2 (2^3 = 8))
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_5_0_d4400100() {
    // Encoding: 0xD4400100
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=8
    // Fields: imm16=8
    let encoding: u32 = 0xD4400100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=15 (2^4 - 1 = 15)
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_6_0_d44001e0() {
    // Encoding: 0xD44001E0
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=15
    // Fields: imm16=15
    let encoding: u32 = 0xD44001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=16 (power of 2 (2^4 = 16))
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_7_0_d4400200() {
    // Encoding: 0xD4400200
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=16
    // Fields: imm16=16
    let encoding: u32 = 0xD4400200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=31 (2^5 - 1 = 31)
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_8_0_d44003e0() {
    // Encoding: 0xD44003E0
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=31
    // Fields: imm16=31
    let encoding: u32 = 0xD44003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=32 (power of 2 (2^5 = 32))
#[test]
fn test_aarch64_system_exceptions_debug_halt_combo_9_0_d4400400() {
    // Encoding: 0xD4400400
    // Test aarch64_system_exceptions_debug_halt field combination: imm16=32
    // Fields: imm16=32
    let encoding: u32 = 0xD4400400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// zero value (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_0_5ac00020() {
    // Test RBIT 32-bit: zero value (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// zero value (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_0_dac00020() {
    // Test RBIT 64-bit: zero value (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (LSB) (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_1_5ac00020() {
    // Test RBIT 32-bit: single bit set (LSB) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x80000000, "W0 should be 0x80000000");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (LSB) (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_1_dac00020() {
    // Test RBIT 64-bit: single bit set (LSB) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x8000000000000000,
        "X0 should be 0x8000000000000000"
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 64) (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_2_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 64) (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_2_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 32) (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_3_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x1, "W0 should be 0x00000001");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 32) (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_3_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x100000000,
        "X0 should be 0x0000000100000000"
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// all ones (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_4_5ac00020() {
    // Test RBIT 32-bit: all ones (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFFFFFFFF, "W0 should be 0xFFFFFFFF");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// all ones (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_4_dac00020() {
    // Test RBIT 64-bit: all ones (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFF,
        "X0 should be 0xFFFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mixed pattern (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_5_5ac00020() {
    // Test RBIT 32-bit: mixed pattern (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x9ABCDEF0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF7B3D59, "W0 should be 0x0F7B3D59");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mixed pattern (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_5_dac00020() {
    // Test RBIT 64-bit: mixed pattern (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x123456789ABCDEF0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF7B3D591E6A2C48,
        "X0 should be 0x0F7B3D591E6A2C48"
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// alternating nibbles (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_6_5ac00020() {
    // Test RBIT 32-bit: alternating nibbles (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF0F0F0F0, "W0 should be 0xF0F0F0F0");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// alternating nibbles (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_6_dac00020() {
    // Test RBIT 64-bit: alternating nibbles (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F0F0F0F0F);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF0F0F0F0F0F0F0F0,
        "X0 should be 0xF0F0F0F0F0F0F0F0"
    );
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// magic values (32)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_32_7_5ac00020() {
    // Test RBIT 32-bit: magic values (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xCAFEBABE);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x7D5D7F53, "W0 should be 0x7D5D7F53");
}

/// Provenance: aarch64_system_exceptions_debug_halt
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// magic values (64)
#[test]
fn test_aarch64_system_exceptions_debug_halt_rbit_oracle_64_7_dac00020() {
    // Test RBIT 64-bit: magic values (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x7D5D7F53F77DB57B,
        "X0 should be 0x7D5D7F53F77DB57B"
    );
}

// ============================================================================
// aarch64_system_exceptions_debug_breakpoint Tests
// ============================================================================

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_0_zero_0_d4200000() {
    // Encoding: 0xD4200000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 0 (Zero)
    // Fields: imm16=0
    let encoding: u32 = 0xD4200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_1_poweroftwo_0_d4200020() {
    // Encoding: 0xD4200020
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 1 (PowerOfTwo)
    // Fields: imm16=1
    let encoding: u32 = 0xD4200020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_3_poweroftwominusone_0_d4200060() {
    // Encoding: 0xD4200060
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 3 (PowerOfTwoMinusOne)
    // Fields: imm16=3
    let encoding: u32 = 0xD4200060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_4_poweroftwo_0_d4200080() {
    // Encoding: 0xD4200080
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 4 (PowerOfTwo)
    // Fields: imm16=4
    let encoding: u32 = 0xD4200080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_7_poweroftwominusone_0_d42000e0() {
    // Encoding: 0xD42000E0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 7 (PowerOfTwoMinusOne)
    // Fields: imm16=7
    let encoding: u32 = 0xD42000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_8_poweroftwo_0_d4200100() {
    // Encoding: 0xD4200100
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 8 (PowerOfTwo)
    // Fields: imm16=8
    let encoding: u32 = 0xD4200100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_15_poweroftwominusone_0_d42001e0() {
    // Encoding: 0xD42001E0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 15 (PowerOfTwoMinusOne)
    // Fields: imm16=15
    let encoding: u32 = 0xD42001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_16_poweroftwo_0_d4200200() {
    // Encoding: 0xD4200200
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 16 (PowerOfTwo)
    // Fields: imm16=16
    let encoding: u32 = 0xD4200200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_31_poweroftwominusone_0_d42003e0() {
    // Encoding: 0xD42003E0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 31 (PowerOfTwoMinusOne)
    // Fields: imm16=31
    let encoding: u32 = 0xD42003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_32_poweroftwo_0_d4200400() {
    // Encoding: 0xD4200400
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 32 (PowerOfTwo)
    // Fields: imm16=32
    let encoding: u32 = 0xD4200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_63_poweroftwominusone_0_d42007e0() {
    // Encoding: 0xD42007E0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 63 (PowerOfTwoMinusOne)
    // Fields: imm16=63
    let encoding: u32 = 0xD42007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_64_poweroftwo_0_d4200800() {
    // Encoding: 0xD4200800
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 64 (PowerOfTwo)
    // Fields: imm16=64
    let encoding: u32 = 0xD4200800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_127_poweroftwominusone_0_d4200fe0() {
    // Encoding: 0xD4200FE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 127 (PowerOfTwoMinusOne)
    // Fields: imm16=127
    let encoding: u32 = 0xD4200FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_128_poweroftwo_0_d4201000() {
    // Encoding: 0xD4201000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 128 (PowerOfTwo)
    // Fields: imm16=128
    let encoding: u32 = 0xD4201000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_255_poweroftwominusone_0_d4201fe0() {
    // Encoding: 0xD4201FE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 255 (PowerOfTwoMinusOne)
    // Fields: imm16=255
    let encoding: u32 = 0xD4201FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_256_poweroftwo_0_d4202000() {
    // Encoding: 0xD4202000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 256 (PowerOfTwo)
    // Fields: imm16=256
    let encoding: u32 = 0xD4202000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_511_poweroftwominusone_0_d4203fe0() {
    // Encoding: 0xD4203FE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 511 (PowerOfTwoMinusOne)
    // Fields: imm16=511
    let encoding: u32 = 0xD4203FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_512_poweroftwo_0_d4204000() {
    // Encoding: 0xD4204000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 512 (PowerOfTwo)
    // Fields: imm16=512
    let encoding: u32 = 0xD4204000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_1023_poweroftwominusone_0_d4207fe0()
{
    // Encoding: 0xD4207FE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 1023 (PowerOfTwoMinusOne)
    // Fields: imm16=1023
    let encoding: u32 = 0xD4207FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_1024_poweroftwo_0_d4208000() {
    // Encoding: 0xD4208000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 1024 (PowerOfTwo)
    // Fields: imm16=1024
    let encoding: u32 = 0xD4208000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_2047_poweroftwominusone_0_d420ffe0()
{
    // Encoding: 0xD420FFE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 2047 (PowerOfTwoMinusOne)
    // Fields: imm16=2047
    let encoding: u32 = 0xD420FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_2048_poweroftwo_0_d4210000() {
    // Encoding: 0xD4210000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 2048 (PowerOfTwo)
    // Fields: imm16=2048
    let encoding: u32 = 0xD4210000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4095, boundary: PowerOfTwoMinusOne }
/// 2^12 - 1 = 4095
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_4095_poweroftwominusone_0_d421ffe0()
{
    // Encoding: 0xD421FFE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm16=4095
    let encoding: u32 = 0xD421FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_4096_poweroftwo_0_d4220000() {
    // Encoding: 0xD4220000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 4096 (PowerOfTwo)
    // Fields: imm16=4096
    let encoding: u32 = 0xD4220000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8191, boundary: PowerOfTwoMinusOne }
/// 2^13 - 1 = 8191
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_8191_poweroftwominusone_0_d423ffe0()
{
    // Encoding: 0xD423FFE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 8191 (PowerOfTwoMinusOne)
    // Fields: imm16=8191
    let encoding: u32 = 0xD423FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8192, boundary: PowerOfTwo }
/// power of 2 (2^13 = 8192)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_8192_poweroftwo_0_d4240000() {
    // Encoding: 0xD4240000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 8192 (PowerOfTwo)
    // Fields: imm16=8192
    let encoding: u32 = 0xD4240000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16383, boundary: PowerOfTwoMinusOne }
/// 2^14 - 1 = 16383
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_16383_poweroftwominusone_0_d427ffe0()
{
    // Encoding: 0xD427FFE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 16383 (PowerOfTwoMinusOne)
    // Fields: imm16=16383
    let encoding: u32 = 0xD427FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16384, boundary: PowerOfTwo }
/// power of 2 (2^14 = 16384)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_16384_poweroftwo_0_d4280000() {
    // Encoding: 0xD4280000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 16384 (PowerOfTwo)
    // Fields: imm16=16384
    let encoding: u32 = 0xD4280000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32767, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (32767)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_32767_poweroftwominusone_0_d42fffe0()
{
    // Encoding: 0xD42FFFE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 32767 (PowerOfTwoMinusOne)
    // Fields: imm16=32767
    let encoding: u32 = 0xD42FFFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32768, boundary: PowerOfTwo }
/// power of 2 (2^15 = 32768)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_32768_poweroftwo_0_d4300000() {
    // Encoding: 0xD4300000
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 32768 (PowerOfTwo)
    // Fields: imm16=32768
    let encoding: u32 = 0xD4300000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 65535, boundary: Max }
/// maximum immediate (65535)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_field_imm16_65535_max_0_d43fffe0() {
    // Encoding: 0xD43FFFE0
    // Test aarch64_system_exceptions_debug_breakpoint field imm16 = 65535 (Max)
    // Fields: imm16=65535
    let encoding: u32 = 0xD43FFFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=0 (immediate value 0)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_0_0_d4200000() {
    // Encoding: 0xD4200000
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=0
    // Fields: imm16=0
    let encoding: u32 = 0xD4200000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=1 (immediate value 1)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_1_0_d4200020() {
    // Encoding: 0xD4200020
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=1
    // Fields: imm16=1
    let encoding: u32 = 0xD4200020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=3 (2^2 - 1 = 3)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_2_0_d4200060() {
    // Encoding: 0xD4200060
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=3
    // Fields: imm16=3
    let encoding: u32 = 0xD4200060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=4 (power of 2 (2^2 = 4))
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_3_0_d4200080() {
    // Encoding: 0xD4200080
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=4
    // Fields: imm16=4
    let encoding: u32 = 0xD4200080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=7 (2^3 - 1 = 7)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_4_0_d42000e0() {
    // Encoding: 0xD42000E0
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=7
    // Fields: imm16=7
    let encoding: u32 = 0xD42000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=8 (power of 2 (2^3 = 8))
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_5_0_d4200100() {
    // Encoding: 0xD4200100
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=8
    // Fields: imm16=8
    let encoding: u32 = 0xD4200100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=15 (2^4 - 1 = 15)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_6_0_d42001e0() {
    // Encoding: 0xD42001E0
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=15
    // Fields: imm16=15
    let encoding: u32 = 0xD42001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=16 (power of 2 (2^4 = 16))
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_7_0_d4200200() {
    // Encoding: 0xD4200200
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=16
    // Fields: imm16=16
    let encoding: u32 = 0xD4200200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=31 (2^5 - 1 = 31)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_8_0_d42003e0() {
    // Encoding: 0xD42003E0
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=31
    // Fields: imm16=31
    let encoding: u32 = 0xD42003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=32 (power of 2 (2^5 = 32))
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_combo_9_0_d4200400() {
    // Encoding: 0xD4200400
    // Test aarch64_system_exceptions_debug_breakpoint field combination: imm16=32
    // Fields: imm16=32
    let encoding: u32 = 0xD4200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// zero value (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_0_5ac00020() {
    // Test RBIT 32-bit: zero value (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// zero value (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_0_dac00020() {
    // Test RBIT 64-bit: zero value (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (LSB) (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_1_5ac00020() {
    // Test RBIT 32-bit: single bit set (LSB) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x80000000, "W0 should be 0x80000000");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (LSB) (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_1_dac00020() {
    // Test RBIT 64-bit: single bit set (LSB) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x8000000000000000,
        "X0 should be 0x8000000000000000"
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 64) (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_2_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 64) (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_2_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 32) (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_3_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x1, "W0 should be 0x00000001");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 32) (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_3_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x100000000,
        "X0 should be 0x0000000100000000"
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// all ones (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_4_5ac00020() {
    // Test RBIT 32-bit: all ones (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFFFFFFFF, "W0 should be 0xFFFFFFFF");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// all ones (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_4_dac00020() {
    // Test RBIT 64-bit: all ones (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFF,
        "X0 should be 0xFFFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mixed pattern (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_5_5ac00020() {
    // Test RBIT 32-bit: mixed pattern (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x9ABCDEF0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF7B3D59, "W0 should be 0x0F7B3D59");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mixed pattern (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_5_dac00020() {
    // Test RBIT 64-bit: mixed pattern (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x123456789ABCDEF0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF7B3D591E6A2C48,
        "X0 should be 0x0F7B3D591E6A2C48"
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// alternating nibbles (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_6_5ac00020() {
    // Test RBIT 32-bit: alternating nibbles (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF0F0F0F0, "W0 should be 0xF0F0F0F0");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// alternating nibbles (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_6_dac00020() {
    // Test RBIT 64-bit: alternating nibbles (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F0F0F0F0F);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF0F0F0F0F0F0F0F0,
        "X0 should be 0xF0F0F0F0F0F0F0F0"
    );
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// magic values (32)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_32_7_5ac00020() {
    // Test RBIT 32-bit: magic values (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xCAFEBABE);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x7D5D7F53, "W0 should be 0x7D5D7F53");
}

/// Provenance: aarch64_system_exceptions_debug_breakpoint
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// magic values (64)
#[test]
fn test_aarch64_system_exceptions_debug_breakpoint_rbit_oracle_64_7_dac00020() {
    // Test RBIT 64-bit: magic values (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x7D5D7F53F77DB57B,
        "X0 should be 0x7D5D7F53F77DB57B"
    );
}

// ============================================================================
// aarch64_system_exceptions_runtime_hvc Tests
// ============================================================================

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_0_zero_2_d4000002() {
    // Encoding: 0xD4000002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 0 (Zero)
    // Fields: imm16=0
    let encoding: u32 = 0xD4000002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_1_poweroftwo_2_d4000022() {
    // Encoding: 0xD4000022
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 1 (PowerOfTwo)
    // Fields: imm16=1
    let encoding: u32 = 0xD4000022;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_3_poweroftwominusone_2_d4000062() {
    // Encoding: 0xD4000062
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 3 (PowerOfTwoMinusOne)
    // Fields: imm16=3
    let encoding: u32 = 0xD4000062;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_4_poweroftwo_2_d4000082() {
    // Encoding: 0xD4000082
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 4 (PowerOfTwo)
    // Fields: imm16=4
    let encoding: u32 = 0xD4000082;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_7_poweroftwominusone_2_d40000e2() {
    // Encoding: 0xD40000E2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 7 (PowerOfTwoMinusOne)
    // Fields: imm16=7
    let encoding: u32 = 0xD40000E2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_8_poweroftwo_2_d4000102() {
    // Encoding: 0xD4000102
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 8 (PowerOfTwo)
    // Fields: imm16=8
    let encoding: u32 = 0xD4000102;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_15_poweroftwominusone_2_d40001e2() {
    // Encoding: 0xD40001E2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 15 (PowerOfTwoMinusOne)
    // Fields: imm16=15
    let encoding: u32 = 0xD40001E2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_16_poweroftwo_2_d4000202() {
    // Encoding: 0xD4000202
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 16 (PowerOfTwo)
    // Fields: imm16=16
    let encoding: u32 = 0xD4000202;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_31_poweroftwominusone_2_d40003e2() {
    // Encoding: 0xD40003E2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 31 (PowerOfTwoMinusOne)
    // Fields: imm16=31
    let encoding: u32 = 0xD40003E2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_32_poweroftwo_2_d4000402() {
    // Encoding: 0xD4000402
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 32 (PowerOfTwo)
    // Fields: imm16=32
    let encoding: u32 = 0xD4000402;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_63_poweroftwominusone_2_d40007e2() {
    // Encoding: 0xD40007E2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 63 (PowerOfTwoMinusOne)
    // Fields: imm16=63
    let encoding: u32 = 0xD40007E2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_64_poweroftwo_2_d4000802() {
    // Encoding: 0xD4000802
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 64 (PowerOfTwo)
    // Fields: imm16=64
    let encoding: u32 = 0xD4000802;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_127_poweroftwominusone_2_d4000fe2() {
    // Encoding: 0xD4000FE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 127 (PowerOfTwoMinusOne)
    // Fields: imm16=127
    let encoding: u32 = 0xD4000FE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_128_poweroftwo_2_d4001002() {
    // Encoding: 0xD4001002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 128 (PowerOfTwo)
    // Fields: imm16=128
    let encoding: u32 = 0xD4001002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_255_poweroftwominusone_2_d4001fe2() {
    // Encoding: 0xD4001FE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 255 (PowerOfTwoMinusOne)
    // Fields: imm16=255
    let encoding: u32 = 0xD4001FE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_256_poweroftwo_2_d4002002() {
    // Encoding: 0xD4002002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 256 (PowerOfTwo)
    // Fields: imm16=256
    let encoding: u32 = 0xD4002002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_511_poweroftwominusone_2_d4003fe2() {
    // Encoding: 0xD4003FE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 511 (PowerOfTwoMinusOne)
    // Fields: imm16=511
    let encoding: u32 = 0xD4003FE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_512_poweroftwo_2_d4004002() {
    // Encoding: 0xD4004002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 512 (PowerOfTwo)
    // Fields: imm16=512
    let encoding: u32 = 0xD4004002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_1023_poweroftwominusone_2_d4007fe2() {
    // Encoding: 0xD4007FE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 1023 (PowerOfTwoMinusOne)
    // Fields: imm16=1023
    let encoding: u32 = 0xD4007FE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_1024_poweroftwo_2_d4008002() {
    // Encoding: 0xD4008002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 1024 (PowerOfTwo)
    // Fields: imm16=1024
    let encoding: u32 = 0xD4008002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_2047_poweroftwominusone_2_d400ffe2() {
    // Encoding: 0xD400FFE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 2047 (PowerOfTwoMinusOne)
    // Fields: imm16=2047
    let encoding: u32 = 0xD400FFE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_2048_poweroftwo_2_d4010002() {
    // Encoding: 0xD4010002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 2048 (PowerOfTwo)
    // Fields: imm16=2048
    let encoding: u32 = 0xD4010002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4095, boundary: PowerOfTwoMinusOne }
/// 2^12 - 1 = 4095
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_4095_poweroftwominusone_2_d401ffe2() {
    // Encoding: 0xD401FFE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm16=4095
    let encoding: u32 = 0xD401FFE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_4096_poweroftwo_2_d4020002() {
    // Encoding: 0xD4020002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 4096 (PowerOfTwo)
    // Fields: imm16=4096
    let encoding: u32 = 0xD4020002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8191, boundary: PowerOfTwoMinusOne }
/// 2^13 - 1 = 8191
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_8191_poweroftwominusone_2_d403ffe2() {
    // Encoding: 0xD403FFE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 8191 (PowerOfTwoMinusOne)
    // Fields: imm16=8191
    let encoding: u32 = 0xD403FFE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8192, boundary: PowerOfTwo }
/// power of 2 (2^13 = 8192)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_8192_poweroftwo_2_d4040002() {
    // Encoding: 0xD4040002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 8192 (PowerOfTwo)
    // Fields: imm16=8192
    let encoding: u32 = 0xD4040002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16383, boundary: PowerOfTwoMinusOne }
/// 2^14 - 1 = 16383
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_16383_poweroftwominusone_2_d407ffe2() {
    // Encoding: 0xD407FFE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 16383 (PowerOfTwoMinusOne)
    // Fields: imm16=16383
    let encoding: u32 = 0xD407FFE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16384, boundary: PowerOfTwo }
/// power of 2 (2^14 = 16384)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_16384_poweroftwo_2_d4080002() {
    // Encoding: 0xD4080002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 16384 (PowerOfTwo)
    // Fields: imm16=16384
    let encoding: u32 = 0xD4080002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32767, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (32767)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_32767_poweroftwominusone_2_d40fffe2() {
    // Encoding: 0xD40FFFE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 32767 (PowerOfTwoMinusOne)
    // Fields: imm16=32767
    let encoding: u32 = 0xD40FFFE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32768, boundary: PowerOfTwo }
/// power of 2 (2^15 = 32768)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_32768_poweroftwo_2_d4100002() {
    // Encoding: 0xD4100002
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 32768 (PowerOfTwo)
    // Fields: imm16=32768
    let encoding: u32 = 0xD4100002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 65535, boundary: Max }
/// maximum immediate (65535)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_field_imm16_65535_max_2_d41fffe2() {
    // Encoding: 0xD41FFFE2
    // Test aarch64_system_exceptions_runtime_hvc field imm16 = 65535 (Max)
    // Fields: imm16=65535
    let encoding: u32 = 0xD41FFFE2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=0 (immediate value 0)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_0_2_d4000002() {
    // Encoding: 0xD4000002
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=0
    // Fields: imm16=0
    let encoding: u32 = 0xD4000002;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=1 (immediate value 1)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_1_2_d4000022() {
    // Encoding: 0xD4000022
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=1
    // Fields: imm16=1
    let encoding: u32 = 0xD4000022;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=3 (2^2 - 1 = 3)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_2_2_d4000062() {
    // Encoding: 0xD4000062
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=3
    // Fields: imm16=3
    let encoding: u32 = 0xD4000062;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=4 (power of 2 (2^2 = 4))
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_3_2_d4000082() {
    // Encoding: 0xD4000082
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=4
    // Fields: imm16=4
    let encoding: u32 = 0xD4000082;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=7 (2^3 - 1 = 7)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_4_2_d40000e2() {
    // Encoding: 0xD40000E2
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=7
    // Fields: imm16=7
    let encoding: u32 = 0xD40000E2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=8 (power of 2 (2^3 = 8))
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_5_2_d4000102() {
    // Encoding: 0xD4000102
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=8
    // Fields: imm16=8
    let encoding: u32 = 0xD4000102;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=15 (2^4 - 1 = 15)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_6_2_d40001e2() {
    // Encoding: 0xD40001E2
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=15
    // Fields: imm16=15
    let encoding: u32 = 0xD40001E2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=16 (power of 2 (2^4 = 16))
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_7_2_d4000202() {
    // Encoding: 0xD4000202
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=16
    // Fields: imm16=16
    let encoding: u32 = 0xD4000202;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=31 (2^5 - 1 = 31)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_8_2_d40003e2() {
    // Encoding: 0xD40003E2
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=31
    // Fields: imm16=31
    let encoding: u32 = 0xD40003E2;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=32 (power of 2 (2^5 = 32))
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_combo_9_2_d4000402() {
    // Encoding: 0xD4000402
    // Test aarch64_system_exceptions_runtime_hvc field combination: imm16=32
    // Fields: imm16=32
    let encoding: u32 = 0xD4000402;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// zero value (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_0_5ac00020() {
    // Test RBIT 32-bit: zero value (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// zero value (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_0_dac00020() {
    // Test RBIT 64-bit: zero value (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (LSB) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_1_5ac00020() {
    // Test RBIT 32-bit: single bit set (LSB) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x80000000, "W0 should be 0x80000000");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (LSB) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_1_dac00020() {
    // Test RBIT 64-bit: single bit set (LSB) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x8000000000000000,
        "X0 should be 0x8000000000000000"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 64) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_2_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 64) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_2_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 32) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_3_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x1, "W0 should be 0x00000001");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 32) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_3_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x100000000,
        "X0 should be 0x0000000100000000"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// all ones (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_4_5ac00020() {
    // Test RBIT 32-bit: all ones (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFFFFFFFF, "W0 should be 0xFFFFFFFF");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// all ones (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_4_dac00020() {
    // Test RBIT 64-bit: all ones (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFF,
        "X0 should be 0xFFFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mixed pattern (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_5_5ac00020() {
    // Test RBIT 32-bit: mixed pattern (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x9ABCDEF0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF7B3D59, "W0 should be 0x0F7B3D59");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mixed pattern (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_5_dac00020() {
    // Test RBIT 64-bit: mixed pattern (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x123456789ABCDEF0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF7B3D591E6A2C48,
        "X0 should be 0x0F7B3D591E6A2C48"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// alternating nibbles (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_6_5ac00020() {
    // Test RBIT 32-bit: alternating nibbles (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF0F0F0F0, "W0 should be 0xF0F0F0F0");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// alternating nibbles (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_6_dac00020() {
    // Test RBIT 64-bit: alternating nibbles (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F0F0F0F0F);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF0F0F0F0F0F0F0F0,
        "X0 should be 0xF0F0F0F0F0F0F0F0"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// magic values (32)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_32_7_5ac00020() {
    // Test RBIT 32-bit: magic values (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xCAFEBABE);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x7D5D7F53, "W0 should be 0x7D5D7F53");
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// magic values (64)
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_rbit_oracle_64_7_dac00020() {
    // Test RBIT 64-bit: magic values (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x7D5D7F53F77DB57B,
        "X0 should be 0x7D5D7F53F77DB57B"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_hvc
/// ASL: `Unconditional`
/// Requirement: UndefinedEncoding { condition: "Unconditional" }
/// triggers Undefined
#[test]
fn test_aarch64_system_exceptions_runtime_hvc_exception_0_d4000002() {
    // Test aarch64_system_exceptions_runtime_hvc exception: Undefined
    // Encoding: 0xD4000002
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0xD4000002;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// aarch64_system_exceptions_runtime_svc Tests
// ============================================================================

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_0_zero_1_d4000001() {
    // Encoding: 0xD4000001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 0 (Zero)
    // Fields: imm16=0
    let encoding: u32 = 0xD4000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_1_poweroftwo_1_d4000021() {
    // Encoding: 0xD4000021
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 1 (PowerOfTwo)
    // Fields: imm16=1
    let encoding: u32 = 0xD4000021;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_3_poweroftwominusone_1_d4000061() {
    // Encoding: 0xD4000061
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 3 (PowerOfTwoMinusOne)
    // Fields: imm16=3
    let encoding: u32 = 0xD4000061;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_4_poweroftwo_1_d4000081() {
    // Encoding: 0xD4000081
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 4 (PowerOfTwo)
    // Fields: imm16=4
    let encoding: u32 = 0xD4000081;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_7_poweroftwominusone_1_d40000e1() {
    // Encoding: 0xD40000E1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 7 (PowerOfTwoMinusOne)
    // Fields: imm16=7
    let encoding: u32 = 0xD40000E1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_8_poweroftwo_1_d4000101() {
    // Encoding: 0xD4000101
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 8 (PowerOfTwo)
    // Fields: imm16=8
    let encoding: u32 = 0xD4000101;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_15_poweroftwominusone_1_d40001e1() {
    // Encoding: 0xD40001E1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 15 (PowerOfTwoMinusOne)
    // Fields: imm16=15
    let encoding: u32 = 0xD40001E1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_16_poweroftwo_1_d4000201() {
    // Encoding: 0xD4000201
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 16 (PowerOfTwo)
    // Fields: imm16=16
    let encoding: u32 = 0xD4000201;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_31_poweroftwominusone_1_d40003e1() {
    // Encoding: 0xD40003E1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 31 (PowerOfTwoMinusOne)
    // Fields: imm16=31
    let encoding: u32 = 0xD40003E1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_32_poweroftwo_1_d4000401() {
    // Encoding: 0xD4000401
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 32 (PowerOfTwo)
    // Fields: imm16=32
    let encoding: u32 = 0xD4000401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_63_poweroftwominusone_1_d40007e1() {
    // Encoding: 0xD40007E1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 63 (PowerOfTwoMinusOne)
    // Fields: imm16=63
    let encoding: u32 = 0xD40007E1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_64_poweroftwo_1_d4000801() {
    // Encoding: 0xD4000801
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 64 (PowerOfTwo)
    // Fields: imm16=64
    let encoding: u32 = 0xD4000801;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_127_poweroftwominusone_1_d4000fe1() {
    // Encoding: 0xD4000FE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 127 (PowerOfTwoMinusOne)
    // Fields: imm16=127
    let encoding: u32 = 0xD4000FE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_128_poweroftwo_1_d4001001() {
    // Encoding: 0xD4001001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 128 (PowerOfTwo)
    // Fields: imm16=128
    let encoding: u32 = 0xD4001001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_255_poweroftwominusone_1_d4001fe1() {
    // Encoding: 0xD4001FE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 255 (PowerOfTwoMinusOne)
    // Fields: imm16=255
    let encoding: u32 = 0xD4001FE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_256_poweroftwo_1_d4002001() {
    // Encoding: 0xD4002001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 256 (PowerOfTwo)
    // Fields: imm16=256
    let encoding: u32 = 0xD4002001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_511_poweroftwominusone_1_d4003fe1() {
    // Encoding: 0xD4003FE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 511 (PowerOfTwoMinusOne)
    // Fields: imm16=511
    let encoding: u32 = 0xD4003FE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_512_poweroftwo_1_d4004001() {
    // Encoding: 0xD4004001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 512 (PowerOfTwo)
    // Fields: imm16=512
    let encoding: u32 = 0xD4004001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_1023_poweroftwominusone_1_d4007fe1() {
    // Encoding: 0xD4007FE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 1023 (PowerOfTwoMinusOne)
    // Fields: imm16=1023
    let encoding: u32 = 0xD4007FE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_1024_poweroftwo_1_d4008001() {
    // Encoding: 0xD4008001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 1024 (PowerOfTwo)
    // Fields: imm16=1024
    let encoding: u32 = 0xD4008001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_2047_poweroftwominusone_1_d400ffe1() {
    // Encoding: 0xD400FFE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 2047 (PowerOfTwoMinusOne)
    // Fields: imm16=2047
    let encoding: u32 = 0xD400FFE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_2048_poweroftwo_1_d4010001() {
    // Encoding: 0xD4010001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 2048 (PowerOfTwo)
    // Fields: imm16=2048
    let encoding: u32 = 0xD4010001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4095, boundary: PowerOfTwoMinusOne }
/// 2^12 - 1 = 4095
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_4095_poweroftwominusone_1_d401ffe1() {
    // Encoding: 0xD401FFE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm16=4095
    let encoding: u32 = 0xD401FFE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_4096_poweroftwo_1_d4020001() {
    // Encoding: 0xD4020001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 4096 (PowerOfTwo)
    // Fields: imm16=4096
    let encoding: u32 = 0xD4020001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8191, boundary: PowerOfTwoMinusOne }
/// 2^13 - 1 = 8191
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_8191_poweroftwominusone_1_d403ffe1() {
    // Encoding: 0xD403FFE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 8191 (PowerOfTwoMinusOne)
    // Fields: imm16=8191
    let encoding: u32 = 0xD403FFE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8192, boundary: PowerOfTwo }
/// power of 2 (2^13 = 8192)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_8192_poweroftwo_1_d4040001() {
    // Encoding: 0xD4040001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 8192 (PowerOfTwo)
    // Fields: imm16=8192
    let encoding: u32 = 0xD4040001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16383, boundary: PowerOfTwoMinusOne }
/// 2^14 - 1 = 16383
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_16383_poweroftwominusone_1_d407ffe1() {
    // Encoding: 0xD407FFE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 16383 (PowerOfTwoMinusOne)
    // Fields: imm16=16383
    let encoding: u32 = 0xD407FFE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16384, boundary: PowerOfTwo }
/// power of 2 (2^14 = 16384)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_16384_poweroftwo_1_d4080001() {
    // Encoding: 0xD4080001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 16384 (PowerOfTwo)
    // Fields: imm16=16384
    let encoding: u32 = 0xD4080001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32767, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (32767)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_32767_poweroftwominusone_1_d40fffe1() {
    // Encoding: 0xD40FFFE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 32767 (PowerOfTwoMinusOne)
    // Fields: imm16=32767
    let encoding: u32 = 0xD40FFFE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32768, boundary: PowerOfTwo }
/// power of 2 (2^15 = 32768)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_32768_poweroftwo_1_d4100001() {
    // Encoding: 0xD4100001
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 32768 (PowerOfTwo)
    // Fields: imm16=32768
    let encoding: u32 = 0xD4100001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 65535, boundary: Max }
/// maximum immediate (65535)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_field_imm16_65535_max_1_d41fffe1() {
    // Encoding: 0xD41FFFE1
    // Test aarch64_system_exceptions_runtime_svc field imm16 = 65535 (Max)
    // Fields: imm16=65535
    let encoding: u32 = 0xD41FFFE1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=0 (immediate value 0)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_0_1_d4000001() {
    // Encoding: 0xD4000001
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=0
    // Fields: imm16=0
    let encoding: u32 = 0xD4000001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=1 (immediate value 1)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_1_1_d4000021() {
    // Encoding: 0xD4000021
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=1
    // Fields: imm16=1
    let encoding: u32 = 0xD4000021;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=3 (2^2 - 1 = 3)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_2_1_d4000061() {
    // Encoding: 0xD4000061
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=3
    // Fields: imm16=3
    let encoding: u32 = 0xD4000061;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=4 (power of 2 (2^2 = 4))
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_3_1_d4000081() {
    // Encoding: 0xD4000081
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=4
    // Fields: imm16=4
    let encoding: u32 = 0xD4000081;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=7 (2^3 - 1 = 7)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_4_1_d40000e1() {
    // Encoding: 0xD40000E1
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=7
    // Fields: imm16=7
    let encoding: u32 = 0xD40000E1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=8 (power of 2 (2^3 = 8))
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_5_1_d4000101() {
    // Encoding: 0xD4000101
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=8
    // Fields: imm16=8
    let encoding: u32 = 0xD4000101;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=15 (2^4 - 1 = 15)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_6_1_d40001e1() {
    // Encoding: 0xD40001E1
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=15
    // Fields: imm16=15
    let encoding: u32 = 0xD40001E1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=16 (power of 2 (2^4 = 16))
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_7_1_d4000201() {
    // Encoding: 0xD4000201
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=16
    // Fields: imm16=16
    let encoding: u32 = 0xD4000201;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=31 (2^5 - 1 = 31)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_8_1_d40003e1() {
    // Encoding: 0xD40003E1
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=31
    // Fields: imm16=31
    let encoding: u32 = 0xD40003E1;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=32 (power of 2 (2^5 = 32))
#[test]
fn test_aarch64_system_exceptions_runtime_svc_combo_9_1_d4000401() {
    // Encoding: 0xD4000401
    // Test aarch64_system_exceptions_runtime_svc field combination: imm16=32
    // Fields: imm16=32
    let encoding: u32 = 0xD4000401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// zero value (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_0_5ac00020() {
    // Test RBIT 32-bit: zero value (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// zero value (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_0_dac00020() {
    // Test RBIT 64-bit: zero value (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (LSB) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_1_5ac00020() {
    // Test RBIT 32-bit: single bit set (LSB) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x80000000, "W0 should be 0x80000000");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (LSB) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_1_dac00020() {
    // Test RBIT 64-bit: single bit set (LSB) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x8000000000000000,
        "X0 should be 0x8000000000000000"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 64) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_2_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 64) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_2_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 32) (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_3_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x1, "W0 should be 0x00000001");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 32) (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_3_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x100000000,
        "X0 should be 0x0000000100000000"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// all ones (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_4_5ac00020() {
    // Test RBIT 32-bit: all ones (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFFFFFFFF, "W0 should be 0xFFFFFFFF");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// all ones (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_4_dac00020() {
    // Test RBIT 64-bit: all ones (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFF,
        "X0 should be 0xFFFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mixed pattern (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_5_5ac00020() {
    // Test RBIT 32-bit: mixed pattern (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x9ABCDEF0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF7B3D59, "W0 should be 0x0F7B3D59");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mixed pattern (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_5_dac00020() {
    // Test RBIT 64-bit: mixed pattern (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x123456789ABCDEF0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF7B3D591E6A2C48,
        "X0 should be 0x0F7B3D591E6A2C48"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// alternating nibbles (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_6_5ac00020() {
    // Test RBIT 32-bit: alternating nibbles (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF0F0F0F0, "W0 should be 0xF0F0F0F0");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// alternating nibbles (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_6_dac00020() {
    // Test RBIT 64-bit: alternating nibbles (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F0F0F0F0F);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF0F0F0F0F0F0F0F0,
        "X0 should be 0xF0F0F0F0F0F0F0F0"
    );
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// magic values (32)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_32_7_5ac00020() {
    // Test RBIT 32-bit: magic values (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xCAFEBABE);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x7D5D7F53, "W0 should be 0x7D5D7F53");
}

/// Provenance: aarch64_system_exceptions_runtime_svc
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// magic values (64)
#[test]
fn test_aarch64_system_exceptions_runtime_svc_rbit_oracle_64_7_dac00020() {
    // Test RBIT 64-bit: magic values (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x7D5D7F53F77DB57B,
        "X0 should be 0x7D5D7F53F77DB57B"
    );
}

// ============================================================================
// aarch64_system_exceptions_debug_exception Tests
// ============================================================================

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 0, boundary: Zero }
/// immediate value 0
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_0_zero_0_d4a00000() {
    // Encoding: 0xD4A00000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 0 (Zero)
    // Fields: imm16=0, LL=0
    let encoding: u32 = 0xD4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1, boundary: PowerOfTwo }
/// immediate value 1
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_1_poweroftwo_0_d4a00020() {
    // Encoding: 0xD4A00020
    // Test aarch64_system_exceptions_debug_exception field imm16 = 1 (PowerOfTwo)
    // Fields: LL=0, imm16=1
    let encoding: u32 = 0xD4A00020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 3, boundary: PowerOfTwoMinusOne }
/// 2^2 - 1 = 3
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_3_poweroftwominusone_0_d4a00060() {
    // Encoding: 0xD4A00060
    // Test aarch64_system_exceptions_debug_exception field imm16 = 3 (PowerOfTwoMinusOne)
    // Fields: imm16=3, LL=0
    let encoding: u32 = 0xD4A00060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4, boundary: PowerOfTwo }
/// power of 2 (2^2 = 4)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_4_poweroftwo_0_d4a00080() {
    // Encoding: 0xD4A00080
    // Test aarch64_system_exceptions_debug_exception field imm16 = 4 (PowerOfTwo)
    // Fields: imm16=4, LL=0
    let encoding: u32 = 0xD4A00080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 7, boundary: PowerOfTwoMinusOne }
/// 2^3 - 1 = 7
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_7_poweroftwominusone_0_d4a000e0() {
    // Encoding: 0xD4A000E0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 7 (PowerOfTwoMinusOne)
    // Fields: imm16=7, LL=0
    let encoding: u32 = 0xD4A000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8, boundary: PowerOfTwo }
/// power of 2 (2^3 = 8)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_8_poweroftwo_0_d4a00100() {
    // Encoding: 0xD4A00100
    // Test aarch64_system_exceptions_debug_exception field imm16 = 8 (PowerOfTwo)
    // Fields: imm16=8, LL=0
    let encoding: u32 = 0xD4A00100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 15, boundary: PowerOfTwoMinusOne }
/// 2^4 - 1 = 15
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_15_poweroftwominusone_0_d4a001e0() {
    // Encoding: 0xD4A001E0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 15 (PowerOfTwoMinusOne)
    // Fields: imm16=15, LL=0
    let encoding: u32 = 0xD4A001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16, boundary: PowerOfTwo }
/// power of 2 (2^4 = 16)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_16_poweroftwo_0_d4a00200() {
    // Encoding: 0xD4A00200
    // Test aarch64_system_exceptions_debug_exception field imm16 = 16 (PowerOfTwo)
    // Fields: imm16=16, LL=0
    let encoding: u32 = 0xD4A00200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 31, boundary: PowerOfTwoMinusOne }
/// 2^5 - 1 = 31
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_31_poweroftwominusone_0_d4a003e0() {
    // Encoding: 0xD4A003E0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 31 (PowerOfTwoMinusOne)
    // Fields: imm16=31, LL=0
    let encoding: u32 = 0xD4A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32, boundary: PowerOfTwo }
/// power of 2 (2^5 = 32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_32_poweroftwo_0_d4a00400() {
    // Encoding: 0xD4A00400
    // Test aarch64_system_exceptions_debug_exception field imm16 = 32 (PowerOfTwo)
    // Fields: imm16=32, LL=0
    let encoding: u32 = 0xD4A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 63, boundary: PowerOfTwoMinusOne }
/// 2^6 - 1 = 63
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_63_poweroftwominusone_0_d4a007e0() {
    // Encoding: 0xD4A007E0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 63 (PowerOfTwoMinusOne)
    // Fields: imm16=63, LL=0
    let encoding: u32 = 0xD4A007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 64, boundary: PowerOfTwo }
/// power of 2 (2^6 = 64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_64_poweroftwo_0_d4a00800() {
    // Encoding: 0xD4A00800
    // Test aarch64_system_exceptions_debug_exception field imm16 = 64 (PowerOfTwo)
    // Fields: LL=0, imm16=64
    let encoding: u32 = 0xD4A00800;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 127, boundary: PowerOfTwoMinusOne }
/// 2^7 - 1 = 127
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_127_poweroftwominusone_0_d4a00fe0() {
    // Encoding: 0xD4A00FE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 127 (PowerOfTwoMinusOne)
    // Fields: imm16=127, LL=0
    let encoding: u32 = 0xD4A00FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 128, boundary: PowerOfTwo }
/// power of 2 (2^7 = 128)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_128_poweroftwo_0_d4a01000() {
    // Encoding: 0xD4A01000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 128 (PowerOfTwo)
    // Fields: imm16=128, LL=0
    let encoding: u32 = 0xD4A01000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 255, boundary: PowerOfTwoMinusOne }
/// 2^8 - 1 = 255
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_255_poweroftwominusone_0_d4a01fe0() {
    // Encoding: 0xD4A01FE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 255 (PowerOfTwoMinusOne)
    // Fields: imm16=255, LL=0
    let encoding: u32 = 0xD4A01FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 256, boundary: PowerOfTwo }
/// power of 2 (2^8 = 256)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_256_poweroftwo_0_d4a02000() {
    // Encoding: 0xD4A02000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 256 (PowerOfTwo)
    // Fields: imm16=256, LL=0
    let encoding: u32 = 0xD4A02000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 511, boundary: PowerOfTwoMinusOne }
/// 2^9 - 1 = 511
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_511_poweroftwominusone_0_d4a03fe0() {
    // Encoding: 0xD4A03FE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 511 (PowerOfTwoMinusOne)
    // Fields: imm16=511, LL=0
    let encoding: u32 = 0xD4A03FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 512, boundary: PowerOfTwo }
/// power of 2 (2^9 = 512)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_512_poweroftwo_0_d4a04000() {
    // Encoding: 0xD4A04000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 512 (PowerOfTwo)
    // Fields: imm16=512, LL=0
    let encoding: u32 = 0xD4A04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1023, boundary: PowerOfTwoMinusOne }
/// 2^10 - 1 = 1023
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_1023_poweroftwominusone_0_d4a07fe0() {
    // Encoding: 0xD4A07FE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 1023 (PowerOfTwoMinusOne)
    // Fields: LL=0, imm16=1023
    let encoding: u32 = 0xD4A07FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 1024, boundary: PowerOfTwo }
/// power of 2 (2^10 = 1024)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_1024_poweroftwo_0_d4a08000() {
    // Encoding: 0xD4A08000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 1024 (PowerOfTwo)
    // Fields: imm16=1024, LL=0
    let encoding: u32 = 0xD4A08000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2047, boundary: PowerOfTwoMinusOne }
/// 2^11 - 1 = 2047
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_2047_poweroftwominusone_0_d4a0ffe0() {
    // Encoding: 0xD4A0FFE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 2047 (PowerOfTwoMinusOne)
    // Fields: LL=0, imm16=2047
    let encoding: u32 = 0xD4A0FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 2048, boundary: PowerOfTwo }
/// power of 2 (2^11 = 2048)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_2048_poweroftwo_0_d4a10000() {
    // Encoding: 0xD4A10000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 2048 (PowerOfTwo)
    // Fields: imm16=2048, LL=0
    let encoding: u32 = 0xD4A10000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4095, boundary: PowerOfTwoMinusOne }
/// 2^12 - 1 = 4095
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_4095_poweroftwominusone_0_d4a1ffe0() {
    // Encoding: 0xD4A1FFE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 4095 (PowerOfTwoMinusOne)
    // Fields: imm16=4095, LL=0
    let encoding: u32 = 0xD4A1FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 4096, boundary: PowerOfTwo }
/// power of 2 (2^12 = 4096)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_4096_poweroftwo_0_d4a20000() {
    // Encoding: 0xD4A20000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 4096 (PowerOfTwo)
    // Fields: LL=0, imm16=4096
    let encoding: u32 = 0xD4A20000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8191, boundary: PowerOfTwoMinusOne }
/// 2^13 - 1 = 8191
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_8191_poweroftwominusone_0_d4a3ffe0() {
    // Encoding: 0xD4A3FFE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 8191 (PowerOfTwoMinusOne)
    // Fields: LL=0, imm16=8191
    let encoding: u32 = 0xD4A3FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 8192, boundary: PowerOfTwo }
/// power of 2 (2^13 = 8192)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_8192_poweroftwo_0_d4a40000() {
    // Encoding: 0xD4A40000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 8192 (PowerOfTwo)
    // Fields: imm16=8192, LL=0
    let encoding: u32 = 0xD4A40000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16383, boundary: PowerOfTwoMinusOne }
/// 2^14 - 1 = 16383
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_16383_poweroftwominusone_0_d4a7ffe0()
{
    // Encoding: 0xD4A7FFE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 16383 (PowerOfTwoMinusOne)
    // Fields: imm16=16383, LL=0
    let encoding: u32 = 0xD4A7FFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 16384, boundary: PowerOfTwo }
/// power of 2 (2^14 = 16384)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_16384_poweroftwo_0_d4a80000() {
    // Encoding: 0xD4A80000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 16384 (PowerOfTwo)
    // Fields: imm16=16384, LL=0
    let encoding: u32 = 0xD4A80000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32767, boundary: PowerOfTwoMinusOne }
/// immediate midpoint (32767)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_32767_poweroftwominusone_0_d4afffe0()
{
    // Encoding: 0xD4AFFFE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 32767 (PowerOfTwoMinusOne)
    // Fields: LL=0, imm16=32767
    let encoding: u32 = 0xD4AFFFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 32768, boundary: PowerOfTwo }
/// power of 2 (2^15 = 32768)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_32768_poweroftwo_0_d4b00000() {
    // Encoding: 0xD4B00000
    // Test aarch64_system_exceptions_debug_exception field imm16 = 32768 (PowerOfTwo)
    // Fields: LL=0, imm16=32768
    let encoding: u32 = 0xD4B00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field imm16 5 +: 16`
/// Requirement: FieldBoundary { field: "imm16", value: 65535, boundary: Max }
/// maximum immediate (65535)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_imm16_65535_max_0_d4bfffe0() {
    // Encoding: 0xD4BFFFE0
    // Test aarch64_system_exceptions_debug_exception field imm16 = 65535 (Max)
    // Fields: imm16=65535, LL=0
    let encoding: u32 = 0xD4BFFFE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field LL 0 +: 2`
/// Requirement: FieldBoundary { field: "LL", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_ll_0_min_0_d4a00000() {
    // Encoding: 0xD4A00000
    // Test aarch64_system_exceptions_debug_exception field LL = 0 (Min)
    // Fields: LL=0, imm16=0
    let encoding: u32 = 0xD4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field LL 0 +: 2`
/// Requirement: FieldBoundary { field: "LL", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_ll_1_poweroftwo_0_d4a00001() {
    // Encoding: 0xD4A00001
    // Test aarch64_system_exceptions_debug_exception field LL = 1 (PowerOfTwo)
    // Fields: imm16=0, LL=1
    let encoding: u32 = 0xD4A00001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field LL 0 +: 2`
/// Requirement: FieldBoundary { field: "LL", value: 3, boundary: Max }
/// maximum value (3)
#[test]
fn test_aarch64_system_exceptions_debug_exception_field_ll_3_max_0_d4a00003() {
    // Encoding: 0xD4A00003
    // Test aarch64_system_exceptions_debug_exception field LL = 3 (Max)
    // Fields: imm16=0, LL=3
    let encoding: u32 = 0xD4A00003;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=0 (immediate value 0)
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_0_0_d4a00000() {
    // Encoding: 0xD4A00000
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=0, LL=0
    // Fields: imm16=0, LL=0
    let encoding: u32 = 0xD4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 1`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=1 (immediate value 1)
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_1_0_d4a00020() {
    // Encoding: 0xD4A00020
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=1, LL=0
    // Fields: LL=0, imm16=1
    let encoding: u32 = 0xD4A00020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 2`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=3 (2^2 - 1 = 3)
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_2_0_d4a00060() {
    // Encoding: 0xD4A00060
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=3, LL=0
    // Fields: imm16=3, LL=0
    let encoding: u32 = 0xD4A00060;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 3`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=4 (power of 2 (2^2 = 4))
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_3_0_d4a00080() {
    // Encoding: 0xD4A00080
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=4, LL=0
    // Fields: imm16=4, LL=0
    let encoding: u32 = 0xD4A00080;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 4`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=7 (2^3 - 1 = 7)
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_4_0_d4a000e0() {
    // Encoding: 0xD4A000E0
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=7, LL=0
    // Fields: LL=0, imm16=7
    let encoding: u32 = 0xD4A000E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 5`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=8 (power of 2 (2^3 = 8))
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_5_0_d4a00100() {
    // Encoding: 0xD4A00100
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=8, LL=0
    // Fields: imm16=8, LL=0
    let encoding: u32 = 0xD4A00100;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 6`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=15 (2^4 - 1 = 15)
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_6_0_d4a001e0() {
    // Encoding: 0xD4A001E0
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=15, LL=0
    // Fields: imm16=15, LL=0
    let encoding: u32 = 0xD4A001E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 7`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=16 (power of 2 (2^4 = 16))
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_7_0_d4a00200() {
    // Encoding: 0xD4A00200
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=16, LL=0
    // Fields: LL=0, imm16=16
    let encoding: u32 = 0xD4A00200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 8`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=31 (2^5 - 1 = 31)
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_8_0_d4a003e0() {
    // Encoding: 0xD4A003E0
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=31, LL=0
    // Fields: imm16=31, LL=0
    let encoding: u32 = 0xD4A003E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `field combination 9`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// imm16=32 (power of 2 (2^5 = 32))
#[test]
fn test_aarch64_system_exceptions_debug_exception_combo_9_0_d4a00400() {
    // Encoding: 0xD4A00400
    // Test aarch64_system_exceptions_debug_exception field combination: imm16=32, LL=0
    // Fields: LL=0, imm16=32
    let encoding: u32 = 0xD4A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(
        exit,
        CpuExit::Continue,
        "instruction 0x{:08X} should execute successfully",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "LL" }), rhs: LitBits([false, false]) }`
/// Requirement: UndefinedEncoding { condition: "Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: \"LL\" }), rhs: LitBits([false, false]) }" }
/// triggers Undefined
#[test]
fn test_aarch64_system_exceptions_debug_exception_invalid_0_0_d4a00000() {
    // Encoding: 0xD4A00000
    // Test aarch64_system_exceptions_debug_exception invalid encoding: Binary { op: Eq, lhs: Var(QualifiedIdentifier { qualifier: Any, name: "LL" }), rhs: LitBits([false, false]) }
    // Fields: LL=0, imm16=0
    let encoding: u32 = 0xD4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_aarch64_system_exceptions_debug_exception_invalid_1_0_d4a00000() {
    // Encoding: 0xD4A00000
    // Test aarch64_system_exceptions_debug_exception invalid encoding: Unconditional UNDEFINED
    // Fields: imm16=0, LL=0
    let encoding: u32 = 0xD4A00000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(
        exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue),
        "expected UNDEFINED for encoding 0x{:08X}",
        encoding
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// zero value (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_0_5ac00020() {
    // Test RBIT 32-bit: zero value (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// zero value (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_0_dac00020() {
    // Test RBIT 64-bit: zero value (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x0, "X0 should be 0x0000000000000000");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (LSB) (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_1_5ac00020() {
    // Test RBIT 32-bit: single bit set (LSB) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x80000000, "W0 should be 0x80000000");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (LSB) (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_1_dac00020() {
    // Test RBIT 64-bit: single bit set (LSB) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x8000000000000000,
        "X0 should be 0x8000000000000000"
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 64) (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_2_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x0, "W0 should be 0x00000000");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 64) (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_2_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 64) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_x(&cpu, 0), 0x1, "X0 should be 0x0000000000000001");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// single bit set (MSB 32) (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_3_5ac00020() {
    // Test RBIT 32-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x1, "W0 should be 0x00000001");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// single bit set (MSB 32) (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_3_dac00020() {
    // Test RBIT 64-bit: single bit set (MSB 32) (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x80000000);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x100000000,
        "X0 should be 0x0000000100000000"
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// all ones (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_4_5ac00020() {
    // Test RBIT 32-bit: all ones (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFF);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xFFFFFFFF, "W0 should be 0xFFFFFFFF");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// all ones (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_4_dac00020() {
    // Test RBIT 64-bit: all ones (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xFFFFFFFFFFFFFFFF,
        "X0 should be 0xFFFFFFFFFFFFFFFF"
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// mixed pattern (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_5_5ac00020() {
    // Test RBIT 32-bit: mixed pattern (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x9ABCDEF0);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF7B3D59, "W0 should be 0x0F7B3D59");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// mixed pattern (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_5_dac00020() {
    // Test RBIT 64-bit: mixed pattern (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x123456789ABCDEF0);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF7B3D591E6A2C48,
        "X0 should be 0x0F7B3D591E6A2C48"
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// alternating nibbles (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_6_5ac00020() {
    // Test RBIT 32-bit: alternating nibbles (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0xF0F0F0F0, "W0 should be 0xF0F0F0F0");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// alternating nibbles (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_6_dac00020() {
    // Test RBIT 64-bit: alternating nibbles (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xF0F0F0F0F0F0F0F);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0xF0F0F0F0F0F0F0F0,
        "X0 should be 0xF0F0F0F0F0F0F0F0"
    );
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp32, dest_field: "Rd" }
/// magic values (32)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_32_7_5ac00020() {
    // Test RBIT 32-bit: magic values (oracle)
    // Encoding: 0x5AC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xCAFEBABE);
    let encoding: u32 = 0x5AC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(get_w(&cpu, 0), 0x7D5D7F53, "W0 should be 0x7D5D7F53");
}

/// Provenance: aarch64_system_exceptions_debug_exception
/// ASL: `RBIT X0, X1`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "Rd" }
/// magic values (64)
#[test]
fn test_aarch64_system_exceptions_debug_exception_rbit_oracle_64_7_dac00020() {
    // Test RBIT 64-bit: magic values (oracle)
    // Encoding: 0xDAC00020
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xDEADBEEFCAFEBABE);
    let encoding: u32 = 0xDAC00020;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(
        get_x(&cpu, 0),
        0x7D5D7F53F77DB57B,
        "X0 should be 0x7D5D7F53F77DB57B"
    );
}
