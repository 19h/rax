//! A64 sve predicate tests.
//!
//! Auto-generated from ARM ASL specifications.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::generated::test_helpers::*;

// ============================================================================
// RDFFR_P.P.F__ Tests
// ============================================================================

/// Provenance: RDFFR_P.P.F__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_rdffr_p_p_f_field_pg_0_min_f000_2518f000() {
    // Encoding: 0x2518F000
    // Test RDFFR_P.P.F__ field Pg = 0 (Min)
    // Fields: Pg=0, Pd=0
    let encoding: u32 = 0x2518F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_rdffr_p_p_f_field_pg_1_poweroftwo_f000_2518f020() {
    // Encoding: 0x2518F020
    // Test RDFFR_P.P.F__ field Pg = 1 (PowerOfTwo)
    // Fields: Pd=0, Pg=1
    let encoding: u32 = 0x2518F020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_rdffr_p_p_f_field_pd_0_min_f000_2518f000() {
    // Encoding: 0x2518F000
    // Test RDFFR_P.P.F__ field Pd = 0 (Min)
    // Fields: Pd=0, Pg=0
    let encoding: u32 = 0x2518F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_rdffr_p_p_f_field_pd_1_poweroftwo_f000_2518f001() {
    // Encoding: 0x2518F001
    // Test RDFFR_P.P.F__ field Pd = 1 (PowerOfTwo)
    // Fields: Pg=0, Pd=1
    let encoding: u32 = 0x2518F001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_rdffr_p_p_f_combo_0_f000_2518f000() {
    // Encoding: 0x2518F000
    // Test RDFFR_P.P.F__ field combination: Pg=0, Pd=0
    // Fields: Pd=0, Pg=0
    let encoding: u32 = 0x2518F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_rdffr_p_p_f_invalid_0_f000_2518f000() {
    // Encoding: 0x2518F000
    // Test RDFFR_P.P.F__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0, Pg=0
    let encoding: u32 = 0x2518F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_rdffr_p_p_f_invalid_1_f000_2518f000() {
    // Encoding: 0x2518F000
    // Test RDFFR_P.P.F__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pd=0
    let encoding: u32 = 0x2518F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_rdffrs_p_p_f_field_pg_0_min_f000_2558f000() {
    // Encoding: 0x2558F000
    // Test RDFFRS_P.P.F__ field Pg = 0 (Min)
    // Fields: Pd=0, Pg=0
    let encoding: u32 = 0x2558F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_rdffrs_p_p_f_field_pg_1_poweroftwo_f000_2558f020() {
    // Encoding: 0x2558F020
    // Test RDFFRS_P.P.F__ field Pg = 1 (PowerOfTwo)
    // Fields: Pd=0, Pg=1
    let encoding: u32 = 0x2558F020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_rdffrs_p_p_f_field_pd_0_min_f000_2558f000() {
    // Encoding: 0x2558F000
    // Test RDFFRS_P.P.F__ field Pd = 0 (Min)
    // Fields: Pg=0, Pd=0
    let encoding: u32 = 0x2558F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_rdffrs_p_p_f_field_pd_1_poweroftwo_f000_2558f001() {
    // Encoding: 0x2558F001
    // Test RDFFRS_P.P.F__ field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pg=0
    let encoding: u32 = 0x2558F001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_rdffrs_p_p_f_combo_0_f000_2558f000() {
    // Encoding: 0x2558F000
    // Test RDFFRS_P.P.F__ field combination: Pg=0, Pd=0
    // Fields: Pd=0, Pg=0
    let encoding: u32 = 0x2558F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_rdffrs_p_p_f_invalid_0_f000_2558f000() {
    // Encoding: 0x2558F000
    // Test RDFFRS_P.P.F__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Pd=0
    let encoding: u32 = 0x2558F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_rdffrs_p_p_f_invalid_1_f000_2558f000() {
    // Encoding: 0x2558F000
    // Test RDFFRS_P.P.F__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pd=0
    let encoding: u32 = 0x2558F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_rdffr_p_p_f_reg_write_0_2518f000() {
    // Test RDFFR_P.P.F__ register write: SimdFromField("Pd")
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_rdffr_p_p_f_flags_zeroresult_0_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: ZeroResult
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_rdffr_p_p_f_flags_zeroresult_1_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: ZeroResult
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_rdffr_p_p_f_flags_negativeresult_2_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: NegativeResult
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_rdffr_p_p_f_flags_unsignedoverflow_3_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: UnsignedOverflow
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_rdffr_p_p_f_flags_unsignedoverflow_4_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: UnsignedOverflow
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_rdffr_p_p_f_flags_signedoverflow_5_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: SignedOverflow
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_rdffr_p_p_f_flags_signedoverflow_6_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: SignedOverflow
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: RDFFR_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_rdffr_p_p_f_flags_positiveresult_7_2518f000() {
    // Test RDFFR_P.P.F__ flag computation: PositiveResult
    // Encoding: 0x2518F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x2518F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_rdffrs_p_p_f_reg_write_0_2558f000() {
    // Test RDFFRS_P.P.F__ register write: SimdFromField("Pd")
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_rdffrs_p_p_f_flags_zeroresult_0_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: ZeroResult
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_rdffrs_p_p_f_flags_zeroresult_1_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: ZeroResult
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_rdffrs_p_p_f_flags_negativeresult_2_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: NegativeResult
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_rdffrs_p_p_f_flags_unsignedoverflow_3_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: UnsignedOverflow
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_rdffrs_p_p_f_flags_unsignedoverflow_4_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: UnsignedOverflow
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_rdffrs_p_p_f_flags_signedoverflow_5_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: SignedOverflow
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_rdffrs_p_p_f_flags_signedoverflow_6_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: SignedOverflow
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: RDFFRS_P.P.F__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_rdffrs_p_p_f_flags_positiveresult_7_2558f000() {
    // Test RDFFRS_P.P.F__ flag computation: PositiveResult
    // Encoding: 0x2558F000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x2558F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// RDFFR_P.F__ Tests
// ============================================================================

/// Provenance: RDFFR_P.F__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_rdffr_p_f_field_pd_0_min_f000_2519f000() {
    // Encoding: 0x2519F000
    // Test RDFFR_P.F__ field Pd = 0 (Min)
    // Fields: Pd=0
    let encoding: u32 = 0x2519F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.F__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_rdffr_p_f_field_pd_1_poweroftwo_f000_2519f001() {
    // Encoding: 0x2519F001
    // Test RDFFR_P.F__ field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1
    let encoding: u32 = 0x2519F001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.F__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pd=0 (register index 0 (first register))
#[test]
fn test_rdffr_p_f_combo_0_f000_2519f000() {
    // Encoding: 0x2519F000
    // Test RDFFR_P.F__ field combination: Pd=0
    // Fields: Pd=0
    let encoding: u32 = 0x2519F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: RDFFR_P.F__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_rdffr_p_f_invalid_0_f000_2519f000() {
    // Encoding: 0x2519F000
    // Test RDFFR_P.F__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0
    let encoding: u32 = 0x2519F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: RDFFR_P.F__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_rdffr_p_f_invalid_1_f000_2519f000() {
    // Encoding: 0x2519F000
    // Test RDFFR_P.F__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pd=0
    let encoding: u32 = 0x2519F000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: RDFFR_P.F__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_rdffr_p_f_reg_write_0_2519f000() {
    // Test RDFFR_P.F__ register write: SimdFromField("Pd")
    // Encoding: 0x2519F000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2519F000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// NAND_P.P.PP_Z Tests
// ============================================================================

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nand_p_p_pp_z_field_pm_0_min_4210_25804210() {
    // Encoding: 0x25804210
    // Test NAND_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25804210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nand_p_p_pp_z_field_pm_1_poweroftwo_4210_25814210() {
    // Encoding: 0x25814210
    // Test NAND_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pn=0, Pm=1, Pd=0, Pg=0
    let encoding: u32 = 0x25814210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nand_p_p_pp_z_field_pg_0_min_4210_25804210() {
    // Encoding: 0x25804210
    // Test NAND_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pm=0, Pd=0, Pn=0, Pg=0
    let encoding: u32 = 0x25804210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nand_p_p_pp_z_field_pg_1_poweroftwo_4210_25804610() {
    // Encoding: 0x25804610
    // Test NAND_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pd=0, Pm=0, Pn=0, Pg=1
    let encoding: u32 = 0x25804610;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nand_p_p_pp_z_field_pn_0_min_4210_25804210() {
    // Encoding: 0x25804210
    // Test NAND_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pg=0, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25804210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nand_p_p_pp_z_field_pn_1_poweroftwo_4210_25804230() {
    // Encoding: 0x25804230
    // Test NAND_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=1, Pm=0, Pd=0
    let encoding: u32 = 0x25804230;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nand_p_p_pp_z_field_pd_0_min_4210_25804210() {
    // Encoding: 0x25804210
    // Test NAND_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pm=0, Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x25804210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nand_p_p_pp_z_field_pd_1_poweroftwo_4210_25804211() {
    // Encoding: 0x25804211
    // Test NAND_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pn=0, Pg=0, Pm=0
    let encoding: u32 = 0x25804211;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_nand_p_p_pp_z_combo_0_4210_25804210() {
    // Encoding: 0x25804210
    // Test NAND_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pd=0, Pg=0, Pm=0, Pn=0
    let encoding: u32 = 0x25804210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_nand_p_p_pp_z_invalid_0_4210_25804210() {
    // Encoding: 0x25804210
    // Test NAND_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25804210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_nand_p_p_pp_z_invalid_1_4210_25804210() {
    // Encoding: 0x25804210
    // Test NAND_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x25804210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nands_p_p_pp_z_field_pm_0_min_4210_25c04210() {
    // Encoding: 0x25C04210
    // Test NANDS_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pn=0, Pg=0, Pm=0, Pd=0
    let encoding: u32 = 0x25C04210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nands_p_p_pp_z_field_pm_1_poweroftwo_4210_25c14210() {
    // Encoding: 0x25C14210
    // Test NANDS_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pm=1, Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x25C14210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nands_p_p_pp_z_field_pg_0_min_4210_25c04210() {
    // Encoding: 0x25C04210
    // Test NANDS_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pn=0, Pm=0, Pd=0, Pg=0
    let encoding: u32 = 0x25C04210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nands_p_p_pp_z_field_pg_1_poweroftwo_4210_25c04610() {
    // Encoding: 0x25C04610
    // Test NANDS_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Pd=0, Pm=0, Pn=0
    let encoding: u32 = 0x25C04610;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nands_p_p_pp_z_field_pn_0_min_4210_25c04210() {
    // Encoding: 0x25C04210
    // Test NANDS_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25C04210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nands_p_p_pp_z_field_pn_1_poweroftwo_4210_25c04230() {
    // Encoding: 0x25C04230
    // Test NANDS_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pd=0, Pm=0, Pn=1, Pg=0
    let encoding: u32 = 0x25C04230;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nands_p_p_pp_z_field_pd_0_min_4210_25c04210() {
    // Encoding: 0x25C04210
    // Test NANDS_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25C04210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nands_p_p_pp_z_field_pd_1_poweroftwo_4210_25c04211() {
    // Encoding: 0x25C04211
    // Test NANDS_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pm=0, Pn=0, Pd=1, Pg=0
    let encoding: u32 = 0x25C04211;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_nands_p_p_pp_z_combo_0_4210_25c04210() {
    // Encoding: 0x25C04210
    // Test NANDS_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25C04210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_nands_p_p_pp_z_invalid_0_4210_25c04210() {
    // Encoding: 0x25C04210
    // Test NANDS_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25C04210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_nands_p_p_pp_z_invalid_1_4210_25c04210() {
    // Encoding: 0x25C04210
    // Test NANDS_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x25C04210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_nand_p_p_pp_z_reg_write_0_25804210() {
    // Test NAND_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_nand_p_p_pp_z_flags_zeroresult_0_25804210() {
    // Test NAND_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_nand_p_p_pp_z_flags_zeroresult_1_25804210() {
    // Test NAND_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_nand_p_p_pp_z_flags_negativeresult_2_25804210() {
    // Test NAND_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_nand_p_p_pp_z_flags_unsignedoverflow_3_25804210() {
    // Test NAND_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_nand_p_p_pp_z_flags_unsignedoverflow_4_25804210() {
    // Test NAND_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_nand_p_p_pp_z_flags_signedoverflow_5_25804210() {
    // Test NAND_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_nand_p_p_pp_z_flags_signedoverflow_6_25804210() {
    // Test NAND_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NAND_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_nand_p_p_pp_z_flags_positiveresult_7_25804210() {
    // Test NAND_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25804210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25804210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_nands_p_p_pp_z_reg_write_0_25c04210() {
    // Test NANDS_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_nands_p_p_pp_z_flags_zeroresult_0_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_nands_p_p_pp_z_flags_zeroresult_1_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_nands_p_p_pp_z_flags_negativeresult_2_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_nands_p_p_pp_z_flags_unsignedoverflow_3_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_nands_p_p_pp_z_flags_unsignedoverflow_4_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_nands_p_p_pp_z_flags_signedoverflow_5_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_nands_p_p_pp_z_flags_signedoverflow_6_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NANDS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_nands_p_p_pp_z_flags_positiveresult_7_25c04210() {
    // Test NANDS_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25C04210
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25C04210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// PFIRST_P.P.P__ Tests
// ============================================================================

/// Provenance: PFIRST_P.P.P__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_pfirst_p_p_p_field_pg_0_min_c000_2558c000() {
    // Encoding: 0x2558C000
    // Test PFIRST_P.P.P__ field Pg = 0 (Min)
    // Fields: Pg=0, Pdn=0
    let encoding: u32 = 0x2558C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_pfirst_p_p_p_field_pg_1_poweroftwo_c000_2558c020() {
    // Encoding: 0x2558C020
    // Test PFIRST_P.P.P__ field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Pdn=0
    let encoding: u32 = 0x2558C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_pfirst_p_p_p_field_pdn_0_min_c000_2558c000() {
    // Encoding: 0x2558C000
    // Test PFIRST_P.P.P__ field Pdn = 0 (Min)
    // Fields: Pg=0, Pdn=0
    let encoding: u32 = 0x2558C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_pfirst_p_p_p_field_pdn_1_poweroftwo_c000_2558c001() {
    // Encoding: 0x2558C001
    // Test PFIRST_P.P.P__ field Pdn = 1 (PowerOfTwo)
    // Fields: Pdn=1, Pg=0
    let encoding: u32 = 0x2558C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 7, boundary: PowerOfTwoMinusOne }
/// midpoint (7)
#[test]
fn test_pfirst_p_p_p_field_pdn_7_poweroftwominusone_c000_2558c007() {
    // Encoding: 0x2558C007
    // Test PFIRST_P.P.P__ field Pdn = 7 (PowerOfTwoMinusOne)
    // Fields: Pg=0, Pdn=7
    let encoding: u32 = 0x2558C007;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 15, boundary: Max }
/// maximum value (15)
#[test]
fn test_pfirst_p_p_p_field_pdn_15_max_c000_2558c00f() {
    // Encoding: 0x2558C00F
    // Test PFIRST_P.P.P__ field Pdn = 15 (Max)
    // Fields: Pg=0, Pdn=15
    let encoding: u32 = 0x2558C00F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_pfirst_p_p_p_combo_0_c000_2558c000() {
    // Encoding: 0x2558C000
    // Test PFIRST_P.P.P__ field combination: Pg=0, Pdn=0
    // Fields: Pdn=0, Pg=0
    let encoding: u32 = 0x2558C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_pfirst_p_p_p_invalid_0_c000_2558c000() {
    // Encoding: 0x2558C000
    // Test PFIRST_P.P.P__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Pdn=0
    let encoding: u32 = 0x2558C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_pfirst_p_p_p_invalid_1_c000_2558c000() {
    // Encoding: 0x2558C000
    // Test PFIRST_P.P.P__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pdn=0
    let encoding: u32 = 0x2558C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `SimdFromField("Pdn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pdn")
#[test]
fn test_pfirst_p_p_p_reg_write_0_2558c000() {
    // Test PFIRST_P.P.P__ register write: SimdFromField("Pdn")
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_pfirst_p_p_p_flags_zeroresult_0_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_pfirst_p_p_p_flags_zeroresult_1_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_pfirst_p_p_p_flags_negativeresult_2_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: NegativeResult
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_pfirst_p_p_p_flags_unsignedoverflow_3_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_pfirst_p_p_p_flags_unsignedoverflow_4_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_pfirst_p_p_p_flags_signedoverflow_5_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_pfirst_p_p_p_flags_signedoverflow_6_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PFIRST_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_pfirst_p_p_p_flags_positiveresult_7_2558c000() {
    // Test PFIRST_P.P.P__ flag computation: PositiveResult
    // Encoding: 0x2558C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x2558C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// BRKPB_P.P.PP__ Tests
// ============================================================================

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpb_p_p_pp_field_pm_0_min_c010_2500c010() {
    // Encoding: 0x2500C010
    // Test BRKPB_P.P.PP__ field Pm = 0 (Min)
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x2500C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpb_p_p_pp_field_pm_1_poweroftwo_c010_2501c010() {
    // Encoding: 0x2501C010
    // Test BRKPB_P.P.PP__ field Pm = 1 (PowerOfTwo)
    // Fields: Pn=0, Pg=0, Pd=0, Pm=1
    let encoding: u32 = 0x2501C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpb_p_p_pp_field_pg_0_min_c010_2500c010() {
    // Encoding: 0x2500C010
    // Test BRKPB_P.P.PP__ field Pg = 0 (Min)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x2500C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpb_p_p_pp_field_pg_1_poweroftwo_c010_2500c410() {
    // Encoding: 0x2500C410
    // Test BRKPB_P.P.PP__ field Pg = 1 (PowerOfTwo)
    // Fields: Pm=0, Pn=0, Pd=0, Pg=1
    let encoding: u32 = 0x2500C410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpb_p_p_pp_field_pn_0_min_c010_2500c010() {
    // Encoding: 0x2500C010
    // Test BRKPB_P.P.PP__ field Pn = 0 (Min)
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x2500C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpb_p_p_pp_field_pn_1_poweroftwo_c010_2500c030() {
    // Encoding: 0x2500C030
    // Test BRKPB_P.P.PP__ field Pn = 1 (PowerOfTwo)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=1
    let encoding: u32 = 0x2500C030;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpb_p_p_pp_field_pd_0_min_c010_2500c010() {
    // Encoding: 0x2500C010
    // Test BRKPB_P.P.PP__ field Pd = 0 (Min)
    // Fields: Pg=0, Pm=0, Pn=0, Pd=0
    let encoding: u32 = 0x2500C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpb_p_p_pp_field_pd_1_poweroftwo_c010_2500c011() {
    // Encoding: 0x2500C011
    // Test BRKPB_P.P.PP__ field Pd = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=0, Pm=0, Pd=1
    let encoding: u32 = 0x2500C011;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_brkpb_p_p_pp_combo_0_c010_2500c010() {
    // Encoding: 0x2500C010
    // Test BRKPB_P.P.PP__ field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pd=0, Pg=0, Pm=0, Pn=0
    let encoding: u32 = 0x2500C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkpb_p_p_pp_invalid_0_c010_2500c010() {
    // Encoding: 0x2500C010
    // Test BRKPB_P.P.PP__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0, Pn=0, Pm=0, Pg=0
    let encoding: u32 = 0x2500C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkpb_p_p_pp_invalid_1_c010_2500c010() {
    // Encoding: 0x2500C010
    // Test BRKPB_P.P.PP__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x2500C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpbs_p_p_pp_field_pm_0_min_c010_2540c010() {
    // Encoding: 0x2540C010
    // Test BRKPBS_P.P.PP__ field Pm = 0 (Min)
    // Fields: Pm=0, Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x2540C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpbs_p_p_pp_field_pm_1_poweroftwo_c010_2541c010() {
    // Encoding: 0x2541C010
    // Test BRKPBS_P.P.PP__ field Pm = 1 (PowerOfTwo)
    // Fields: Pm=1, Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x2541C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpbs_p_p_pp_field_pg_0_min_c010_2540c010() {
    // Encoding: 0x2540C010
    // Test BRKPBS_P.P.PP__ field Pg = 0 (Min)
    // Fields: Pd=0, Pg=0, Pn=0, Pm=0
    let encoding: u32 = 0x2540C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpbs_p_p_pp_field_pg_1_poweroftwo_c010_2540c410() {
    // Encoding: 0x2540C410
    // Test BRKPBS_P.P.PP__ field Pg = 1 (PowerOfTwo)
    // Fields: Pn=0, Pg=1, Pm=0, Pd=0
    let encoding: u32 = 0x2540C410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpbs_p_p_pp_field_pn_0_min_c010_2540c010() {
    // Encoding: 0x2540C010
    // Test BRKPBS_P.P.PP__ field Pn = 0 (Min)
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x2540C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpbs_p_p_pp_field_pn_1_poweroftwo_c010_2540c030() {
    // Encoding: 0x2540C030
    // Test BRKPBS_P.P.PP__ field Pn = 1 (PowerOfTwo)
    // Fields: Pm=0, Pg=0, Pn=1, Pd=0
    let encoding: u32 = 0x2540C030;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpbs_p_p_pp_field_pd_0_min_c010_2540c010() {
    // Encoding: 0x2540C010
    // Test BRKPBS_P.P.PP__ field Pd = 0 (Min)
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x2540C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpbs_p_p_pp_field_pd_1_poweroftwo_c010_2540c011() {
    // Encoding: 0x2540C011
    // Test BRKPBS_P.P.PP__ field Pd = 1 (PowerOfTwo)
    // Fields: Pg=0, Pd=1, Pn=0, Pm=0
    let encoding: u32 = 0x2540C011;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_brkpbs_p_p_pp_combo_0_c010_2540c010() {
    // Encoding: 0x2540C010
    // Test BRKPBS_P.P.PP__ field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pd=0, Pg=0, Pm=0, Pn=0
    let encoding: u32 = 0x2540C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkpbs_p_p_pp_invalid_0_c010_2540c010() {
    // Encoding: 0x2540C010
    // Test BRKPBS_P.P.PP__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0, Pm=0, Pn=0, Pg=0
    let encoding: u32 = 0x2540C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkpbs_p_p_pp_invalid_1_c010_2540c010() {
    // Encoding: 0x2540C010
    // Test BRKPBS_P.P.PP__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pd=0, Pm=0, Pn=0, Pg=0
    let encoding: u32 = 0x2540C010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brkpb_p_p_pp_reg_write_0_2500c010() {
    // Test BRKPB_P.P.PP__ register write: SimdFromField("Pd")
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkpb_p_p_pp_flags_zeroresult_0_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkpb_p_p_pp_flags_zeroresult_1_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkpb_p_p_pp_flags_negativeresult_2_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: NegativeResult
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkpb_p_p_pp_flags_unsignedoverflow_3_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkpb_p_p_pp_flags_unsignedoverflow_4_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkpb_p_p_pp_flags_signedoverflow_5_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkpb_p_p_pp_flags_signedoverflow_6_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPB_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkpb_p_p_pp_flags_positiveresult_7_2500c010() {
    // Test BRKPB_P.P.PP__ flag computation: PositiveResult
    // Encoding: 0x2500C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x2500C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brkpbs_p_p_pp_reg_write_0_2540c010() {
    // Test BRKPBS_P.P.PP__ register write: SimdFromField("Pd")
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkpbs_p_p_pp_flags_zeroresult_0_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkpbs_p_p_pp_flags_zeroresult_1_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkpbs_p_p_pp_flags_negativeresult_2_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: NegativeResult
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkpbs_p_p_pp_flags_unsignedoverflow_3_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkpbs_p_p_pp_flags_unsignedoverflow_4_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkpbs_p_p_pp_flags_signedoverflow_5_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkpbs_p_p_pp_flags_signedoverflow_6_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPBS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkpbs_p_p_pp_flags_positiveresult_7_2540c010() {
    // Test BRKPBS_P.P.PP__ flag computation: PositiveResult
    // Encoding: 0x2540C010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x2540C010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// SEL_P.P.PP__ Tests
// ============================================================================

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_sel_p_p_pp_field_pm_0_min_4210_25004210() {
    // Encoding: 0x25004210
    // Test SEL_P.P.PP__ field Pm = 0 (Min)
    // Fields: Pg=0, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25004210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_sel_p_p_pp_field_pm_1_poweroftwo_4210_25014210() {
    // Encoding: 0x25014210
    // Test SEL_P.P.PP__ field Pm = 1 (PowerOfTwo)
    // Fields: Pd=0, Pn=0, Pg=0, Pm=1
    let encoding: u32 = 0x25014210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_sel_p_p_pp_field_pg_0_min_4210_25004210() {
    // Encoding: 0x25004210
    // Test SEL_P.P.PP__ field Pg = 0 (Min)
    // Fields: Pg=0, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25004210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_sel_p_p_pp_field_pg_1_poweroftwo_4210_25004610() {
    // Encoding: 0x25004610
    // Test SEL_P.P.PP__ field Pg = 1 (PowerOfTwo)
    // Fields: Pm=0, Pn=0, Pd=0, Pg=1
    let encoding: u32 = 0x25004610;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_sel_p_p_pp_field_pn_0_min_4210_25004210() {
    // Encoding: 0x25004210
    // Test SEL_P.P.PP__ field Pn = 0 (Min)
    // Fields: Pg=0, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25004210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_sel_p_p_pp_field_pn_1_poweroftwo_4210_25004230() {
    // Encoding: 0x25004230
    // Test SEL_P.P.PP__ field Pn = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=1, Pm=0, Pd=0
    let encoding: u32 = 0x25004230;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_sel_p_p_pp_field_pd_0_min_4210_25004210() {
    // Encoding: 0x25004210
    // Test SEL_P.P.PP__ field Pd = 0 (Min)
    // Fields: Pd=0, Pn=0, Pg=0, Pm=0
    let encoding: u32 = 0x25004210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_sel_p_p_pp_field_pd_1_poweroftwo_4210_25004211() {
    // Encoding: 0x25004211
    // Test SEL_P.P.PP__ field Pd = 1 (PowerOfTwo)
    // Fields: Pg=0, Pm=0, Pn=0, Pd=1
    let encoding: u32 = 0x25004211;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_sel_p_p_pp_combo_0_4210_25004210() {
    // Encoding: 0x25004210
    // Test SEL_P.P.PP__ field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pg=0, Pm=0, Pd=0, Pn=0
    let encoding: u32 = 0x25004210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_sel_p_p_pp_invalid_0_4210_25004210() {
    // Encoding: 0x25004210
    // Test SEL_P.P.PP__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pm=0, Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25004210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_sel_p_p_pp_invalid_1_4210_25004210() {
    // Encoding: 0x25004210
    // Test SEL_P.P.PP__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25004210;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: SEL_P.P.PP__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_sel_p_p_pp_reg_write_0_25004210() {
    // Test SEL_P.P.PP__ register write: SimdFromField("Pd")
    // Encoding: 0x25004210
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25004210;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// PTRUE_P.S__ Tests
// ============================================================================

/// Provenance: PTRUE_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_ptrue_p_s_field_size_0_min_e000_2518e000() {
    // Encoding: 0x2518E000
    // Test PTRUE_P.S__ field size = 0 (Min)
    // Fields: Pd=0, size=0, pattern=0
    let encoding: u32 = 0x2518E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_ptrue_p_s_field_size_1_poweroftwo_e000_2558e000() {
    // Encoding: 0x2558E000
    // Test PTRUE_P.S__ field size = 1 (PowerOfTwo)
    // Fields: Pd=0, size=1, pattern=0
    let encoding: u32 = 0x2558E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_ptrue_p_s_field_size_2_poweroftwo_e000_2598e000() {
    // Encoding: 0x2598E000
    // Test PTRUE_P.S__ field size = 2 (PowerOfTwo)
    // Fields: pattern=0, size=2, Pd=0
    let encoding: u32 = 0x2598E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_ptrue_p_s_field_size_3_max_e000_25d8e000() {
    // Encoding: 0x25D8E000
    // Test PTRUE_P.S__ field size = 3 (Max)
    // Fields: Pd=0, size=3, pattern=0
    let encoding: u32 = 0x25D8E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ptrue_p_s_field_pattern_0_min_e000_2518e000() {
    // Encoding: 0x2518E000
    // Test PTRUE_P.S__ field pattern = 0 (Min)
    // Fields: pattern=0, Pd=0, size=0
    let encoding: u32 = 0x2518E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_ptrue_p_s_field_pattern_1_poweroftwo_e000_2518e020() {
    // Encoding: 0x2518E020
    // Test PTRUE_P.S__ field pattern = 1 (PowerOfTwo)
    // Fields: size=0, pattern=1, Pd=0
    let encoding: u32 = 0x2518E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_ptrue_p_s_field_pattern_15_poweroftwominusone_e000_2518e1e0() {
    // Encoding: 0x2518E1E0
    // Test PTRUE_P.S__ field pattern = 15 (PowerOfTwoMinusOne)
    // Fields: pattern=15, Pd=0, size=0
    let encoding: u32 = 0x2518E1E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_ptrue_p_s_field_pattern_31_max_e000_2518e3e0() {
    // Encoding: 0x2518E3E0
    // Test PTRUE_P.S__ field pattern = 31 (Max)
    // Fields: pattern=31, Pd=0, size=0
    let encoding: u32 = 0x2518E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ptrue_p_s_field_pd_0_min_e000_2518e000() {
    // Encoding: 0x2518E000
    // Test PTRUE_P.S__ field Pd = 0 (Min)
    // Fields: size=0, pattern=0, Pd=0
    let encoding: u32 = 0x2518E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ptrue_p_s_field_pd_1_poweroftwo_e000_2518e001() {
    // Encoding: 0x2518E001
    // Test PTRUE_P.S__ field Pd = 1 (PowerOfTwo)
    // Fields: size=0, pattern=0, Pd=1
    let encoding: u32 = 0x2518E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_ptrue_p_s_combo_0_e000_2518e000() {
    // Encoding: 0x2518E000
    // Test PTRUE_P.S__ field combination: size=0, pattern=0, Pd=0
    // Fields: size=0, pattern=0, Pd=0
    let encoding: u32 = 0x2518E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_ptrue_p_s_special_size_0_size_variant_0_57344_2518e000() {
    // Encoding: 0x2518E000
    // Test PTRUE_P.S__ special value size = 0 (Size variant 0)
    // Fields: pattern=0, size=0, Pd=0
    let encoding: u32 = 0x2518E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_ptrue_p_s_special_size_1_size_variant_1_57344_2558e000() {
    // Encoding: 0x2558E000
    // Test PTRUE_P.S__ special value size = 1 (Size variant 1)
    // Fields: pattern=0, size=1, Pd=0
    let encoding: u32 = 0x2558E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_ptrue_p_s_special_size_2_size_variant_2_57344_2598e000() {
    // Encoding: 0x2598E000
    // Test PTRUE_P.S__ special value size = 2 (Size variant 2)
    // Fields: Pd=0, pattern=0, size=2
    let encoding: u32 = 0x2598E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_ptrue_p_s_special_size_3_size_variant_3_57344_25d8e000() {
    // Encoding: 0x25D8E000
    // Test PTRUE_P.S__ special value size = 3 (Size variant 3)
    // Fields: size=3, pattern=0, Pd=0
    let encoding: u32 = 0x25D8E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ptrue_p_s_invalid_0_e000_2518e000() {
    // Encoding: 0x2518E000
    // Test PTRUE_P.S__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: pattern=0, Pd=0, size=0
    let encoding: u32 = 0x2518E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ptrue_p_s_invalid_1_e000_2518e000() {
    // Encoding: 0x2518E000
    // Test PTRUE_P.S__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pd=0, size=0, pattern=0
    let encoding: u32 = 0x2518E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_ptrues_p_s_field_size_0_min_e000_2519e000() {
    // Encoding: 0x2519E000
    // Test PTRUES_P.S__ field size = 0 (Min)
    // Fields: pattern=0, Pd=0, size=0
    let encoding: u32 = 0x2519E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_ptrues_p_s_field_size_1_poweroftwo_e000_2559e000() {
    // Encoding: 0x2559E000
    // Test PTRUES_P.S__ field size = 1 (PowerOfTwo)
    // Fields: pattern=0, Pd=0, size=1
    let encoding: u32 = 0x2559E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_ptrues_p_s_field_size_2_poweroftwo_e000_2599e000() {
    // Encoding: 0x2599E000
    // Test PTRUES_P.S__ field size = 2 (PowerOfTwo)
    // Fields: pattern=0, Pd=0, size=2
    let encoding: u32 = 0x2599E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_ptrues_p_s_field_size_3_max_e000_25d9e000() {
    // Encoding: 0x25D9E000
    // Test PTRUES_P.S__ field size = 3 (Max)
    // Fields: pattern=0, Pd=0, size=3
    let encoding: u32 = 0x25D9E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_ptrues_p_s_field_pattern_0_min_e000_2519e000() {
    // Encoding: 0x2519E000
    // Test PTRUES_P.S__ field pattern = 0 (Min)
    // Fields: pattern=0, Pd=0, size=0
    let encoding: u32 = 0x2519E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_ptrues_p_s_field_pattern_1_poweroftwo_e000_2519e020() {
    // Encoding: 0x2519E020
    // Test PTRUES_P.S__ field pattern = 1 (PowerOfTwo)
    // Fields: Pd=0, size=0, pattern=1
    let encoding: u32 = 0x2519E020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 15, boundary: PowerOfTwoMinusOne }
/// midpoint (15)
#[test]
fn test_ptrues_p_s_field_pattern_15_poweroftwominusone_e000_2519e1e0() {
    // Encoding: 0x2519E1E0
    // Test PTRUES_P.S__ field pattern = 15 (PowerOfTwoMinusOne)
    // Fields: size=0, pattern=15, Pd=0
    let encoding: u32 = 0x2519E1E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field pattern 5 +: 5`
/// Requirement: FieldBoundary { field: "pattern", value: 31, boundary: Max }
/// maximum value (31)
#[test]
fn test_ptrues_p_s_field_pattern_31_max_e000_2519e3e0() {
    // Encoding: 0x2519E3E0
    // Test PTRUES_P.S__ field pattern = 31 (Max)
    // Fields: pattern=31, Pd=0, size=0
    let encoding: u32 = 0x2519E3E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ptrues_p_s_field_pd_0_min_e000_2519e000() {
    // Encoding: 0x2519E000
    // Test PTRUES_P.S__ field Pd = 0 (Min)
    // Fields: size=0, pattern=0, Pd=0
    let encoding: u32 = 0x2519E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ptrues_p_s_field_pd_1_poweroftwo_e000_2519e001() {
    // Encoding: 0x2519E001
    // Test PTRUES_P.S__ field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, size=0, pattern=0
    let encoding: u32 = 0x2519E001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_ptrues_p_s_combo_0_e000_2519e000() {
    // Encoding: 0x2519E000
    // Test PTRUES_P.S__ field combination: size=0, pattern=0, Pd=0
    // Fields: size=0, Pd=0, pattern=0
    let encoding: u32 = 0x2519E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_ptrues_p_s_special_size_0_size_variant_0_57344_2519e000() {
    // Encoding: 0x2519E000
    // Test PTRUES_P.S__ special value size = 0 (Size variant 0)
    // Fields: size=0, pattern=0, Pd=0
    let encoding: u32 = 0x2519E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_ptrues_p_s_special_size_1_size_variant_1_57344_2559e000() {
    // Encoding: 0x2559E000
    // Test PTRUES_P.S__ special value size = 1 (Size variant 1)
    // Fields: pattern=0, Pd=0, size=1
    let encoding: u32 = 0x2559E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_ptrues_p_s_special_size_2_size_variant_2_57344_2599e000() {
    // Encoding: 0x2599E000
    // Test PTRUES_P.S__ special value size = 2 (Size variant 2)
    // Fields: pattern=0, size=2, Pd=0
    let encoding: u32 = 0x2599E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_ptrues_p_s_special_size_3_size_variant_3_57344_25d9e000() {
    // Encoding: 0x25D9E000
    // Test PTRUES_P.S__ special value size = 3 (Size variant 3)
    // Fields: size=3, pattern=0, Pd=0
    let encoding: u32 = 0x25D9E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ptrues_p_s_invalid_0_e000_2519e000() {
    // Encoding: 0x2519E000
    // Test PTRUES_P.S__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: size=0, Pd=0, pattern=0
    let encoding: u32 = 0x2519E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PTRUES_P.S__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ptrues_p_s_invalid_1_e000_2519e000() {
    // Encoding: 0x2519E000
    // Test PTRUES_P.S__ invalid encoding: Unconditional UNDEFINED
    // Fields: size=0, pattern=0, Pd=0
    let encoding: u32 = 0x2519E000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PTRUE_P.S__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_ptrue_p_s_reg_write_0_2518e000() {
    // Test PTRUE_P.S__ register write: SimdFromField("Pd")
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_ptrue_p_s_flags_zeroresult_0_2518e000() {
    // Test PTRUE_P.S__ flag computation: ZeroResult
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_ptrue_p_s_flags_zeroresult_1_2518e000() {
    // Test PTRUE_P.S__ flag computation: ZeroResult
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_ptrue_p_s_flags_negativeresult_2_2518e000() {
    // Test PTRUE_P.S__ flag computation: NegativeResult
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_ptrue_p_s_flags_unsignedoverflow_3_2518e000() {
    // Test PTRUE_P.S__ flag computation: UnsignedOverflow
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_ptrue_p_s_flags_unsignedoverflow_4_2518e000() {
    // Test PTRUE_P.S__ flag computation: UnsignedOverflow
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_ptrue_p_s_flags_signedoverflow_5_2518e000() {
    // Test PTRUE_P.S__ flag computation: SignedOverflow
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_ptrue_p_s_flags_signedoverflow_6_2518e000() {
    // Test PTRUE_P.S__ flag computation: SignedOverflow
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PTRUE_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_ptrue_p_s_flags_positiveresult_7_2518e000() {
    // Test PTRUE_P.S__ flag computation: PositiveResult
    // Encoding: 0x2518E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x2518E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUES_P.S__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_ptrues_p_s_reg_write_0_2519e000() {
    // Test PTRUES_P.S__ register write: SimdFromField("Pd")
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_ptrues_p_s_flags_zeroresult_0_2519e000() {
    // Test PTRUES_P.S__ flag computation: ZeroResult
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_ptrues_p_s_flags_zeroresult_1_2519e000() {
    // Test PTRUES_P.S__ flag computation: ZeroResult
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_ptrues_p_s_flags_negativeresult_2_2519e000() {
    // Test PTRUES_P.S__ flag computation: NegativeResult
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_ptrues_p_s_flags_unsignedoverflow_3_2519e000() {
    // Test PTRUES_P.S__ flag computation: UnsignedOverflow
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_ptrues_p_s_flags_unsignedoverflow_4_2519e000() {
    // Test PTRUES_P.S__ flag computation: UnsignedOverflow
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_ptrues_p_s_flags_signedoverflow_5_2519e000() {
    // Test PTRUES_P.S__ flag computation: SignedOverflow
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_ptrues_p_s_flags_signedoverflow_6_2519e000() {
    // Test PTRUES_P.S__ flag computation: SignedOverflow
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PTRUES_P.S__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_ptrues_p_s_flags_positiveresult_7_2519e000() {
    // Test PTRUES_P.S__ flag computation: PositiveResult
    // Encoding: 0x2519E000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x2519E000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// BRKA_P.P.P__ Tests
// ============================================================================

/// Provenance: BRKA_P.P.P__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brka_p_p_p_field_pg_0_min_4000_25104000() {
    // Encoding: 0x25104000
    // Test BRKA_P.P.P__ field Pg = 0 (Min)
    // Fields: Pn=0, Pd=0, Pg=0, M=0
    let encoding: u32 = 0x25104000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brka_p_p_p_field_pg_1_poweroftwo_4000_25104400() {
    // Encoding: 0x25104400
    // Test BRKA_P.P.P__ field Pg = 1 (PowerOfTwo)
    // Fields: Pn=0, M=0, Pg=1, Pd=0
    let encoding: u32 = 0x25104400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brka_p_p_p_field_pn_0_min_4000_25104000() {
    // Encoding: 0x25104000
    // Test BRKA_P.P.P__ field Pn = 0 (Min)
    // Fields: Pg=0, Pd=0, M=0, Pn=0
    let encoding: u32 = 0x25104000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brka_p_p_p_field_pn_1_poweroftwo_4000_25104020() {
    // Encoding: 0x25104020
    // Test BRKA_P.P.P__ field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, Pd=0, Pg=0, M=0
    let encoding: u32 = 0x25104020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field M 4 +: 1`
/// Requirement: FieldBoundary { field: "M", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_brka_p_p_p_field_m_0_min_4000_25104000() {
    // Encoding: 0x25104000
    // Test BRKA_P.P.P__ field M = 0 (Min)
    // Fields: M=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25104000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field M 4 +: 1`
/// Requirement: FieldBoundary { field: "M", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_brka_p_p_p_field_m_1_max_4000_25104010() {
    // Encoding: 0x25104010
    // Test BRKA_P.P.P__ field M = 1 (Max)
    // Fields: Pg=0, M=1, Pd=0, Pn=0
    let encoding: u32 = 0x25104010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brka_p_p_p_field_pd_0_min_4000_25104000() {
    // Encoding: 0x25104000
    // Test BRKA_P.P.P__ field Pd = 0 (Min)
    // Fields: Pg=0, Pd=0, Pn=0, M=0
    let encoding: u32 = 0x25104000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brka_p_p_p_field_pd_1_poweroftwo_4000_25104001() {
    // Encoding: 0x25104001
    // Test BRKA_P.P.P__ field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, M=0, Pn=0, Pg=0
    let encoding: u32 = 0x25104001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_brka_p_p_p_combo_0_4000_25104000() {
    // Encoding: 0x25104000
    // Test BRKA_P.P.P__ field combination: Pg=0, Pn=0, M=0, Pd=0
    // Fields: Pd=0, M=0, Pg=0, Pn=0
    let encoding: u32 = 0x25104000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brka_p_p_p_invalid_0_4000_25104000() {
    // Encoding: 0x25104000
    // Test BRKA_P.P.P__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: M=0, Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x25104000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brka_p_p_p_invalid_1_4000_25104000() {
    // Encoding: 0x25104000
    // Test BRKA_P.P.P__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pd=0, Pg=0, Pn=0, M=0
    let encoding: u32 = 0x25104000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkas_p_p_p_z_field_pg_0_min_4000_25504000() {
    // Encoding: 0x25504000
    // Test BRKAS_P.P.P_Z field Pg = 0 (Min)
    // Fields: Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25504000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkas_p_p_p_z_field_pg_1_poweroftwo_4000_25504400() {
    // Encoding: 0x25504400
    // Test BRKAS_P.P.P_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Pn=0, Pd=0
    let encoding: u32 = 0x25504400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkas_p_p_p_z_field_pn_0_min_4000_25504000() {
    // Encoding: 0x25504000
    // Test BRKAS_P.P.P_Z field Pn = 0 (Min)
    // Fields: Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25504000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkas_p_p_p_z_field_pn_1_poweroftwo_4000_25504020() {
    // Encoding: 0x25504020
    // Test BRKAS_P.P.P_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=1, Pd=0
    let encoding: u32 = 0x25504020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkas_p_p_p_z_field_pd_0_min_4000_25504000() {
    // Encoding: 0x25504000
    // Test BRKAS_P.P.P_Z field Pd = 0 (Min)
    // Fields: Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x25504000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkas_p_p_p_z_field_pd_1_poweroftwo_4000_25504001() {
    // Encoding: 0x25504001
    // Test BRKAS_P.P.P_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pn=0, Pd=1, Pg=0
    let encoding: u32 = 0x25504001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_brkas_p_p_p_z_combo_0_4000_25504000() {
    // Encoding: 0x25504000
    // Test BRKAS_P.P.P_Z field combination: Pg=0, Pn=0, Pd=0
    // Fields: Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25504000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkas_p_p_p_z_invalid_0_4000_25504000() {
    // Encoding: 0x25504000
    // Test BRKAS_P.P.P_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25504000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkas_p_p_p_z_invalid_1_4000_25504000() {
    // Encoding: 0x25504000
    // Test BRKAS_P.P.P_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25504000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKA_P.P.P__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brka_p_p_p_reg_write_0_25104000() {
    // Test BRKA_P.P.P__ register write: SimdFromField("Pd")
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brka_p_p_p_flags_zeroresult_0_25104000() {
    // Test BRKA_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brka_p_p_p_flags_zeroresult_1_25104000() {
    // Test BRKA_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brka_p_p_p_flags_negativeresult_2_25104000() {
    // Test BRKA_P.P.P__ flag computation: NegativeResult
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brka_p_p_p_flags_unsignedoverflow_3_25104000() {
    // Test BRKA_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brka_p_p_p_flags_unsignedoverflow_4_25104000() {
    // Test BRKA_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brka_p_p_p_flags_signedoverflow_5_25104000() {
    // Test BRKA_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brka_p_p_p_flags_signedoverflow_6_25104000() {
    // Test BRKA_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKA_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brka_p_p_p_flags_positiveresult_7_25104000() {
    // Test BRKA_P.P.P__ flag computation: PositiveResult
    // Encoding: 0x25104000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25104000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brkas_p_p_p_z_reg_write_0_25504000() {
    // Test BRKAS_P.P.P_Z register write: SimdFromField("Pd")
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkas_p_p_p_z_flags_zeroresult_0_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: ZeroResult
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkas_p_p_p_z_flags_zeroresult_1_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: ZeroResult
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkas_p_p_p_z_flags_negativeresult_2_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: NegativeResult
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkas_p_p_p_z_flags_unsignedoverflow_3_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: UnsignedOverflow
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkas_p_p_p_z_flags_unsignedoverflow_4_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: UnsignedOverflow
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkas_p_p_p_z_flags_signedoverflow_5_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: SignedOverflow
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkas_p_p_p_z_flags_signedoverflow_6_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: SignedOverflow
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKAS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkas_p_p_p_z_flags_positiveresult_7_25504000() {
    // Test BRKAS_P.P.P_Z flag computation: PositiveResult
    // Encoding: 0x25504000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25504000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// PTEST_.P.P__ Tests
// ============================================================================

/// Provenance: PTEST_.P.P__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ptest_p_p_field_pg_0_min_c000_2550c000() {
    // Encoding: 0x2550C000
    // Test PTEST_.P.P__ field Pg = 0 (Min)
    // Fields: Pn=0, Pg=0
    let encoding: u32 = 0x2550C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTEST_.P.P__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ptest_p_p_field_pg_1_poweroftwo_c000_2550c400() {
    // Encoding: 0x2550C400
    // Test PTEST_.P.P__ field Pg = 1 (PowerOfTwo)
    // Fields: Pn=0, Pg=1
    let encoding: u32 = 0x2550C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTEST_.P.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_ptest_p_p_field_pn_0_min_c000_2550c000() {
    // Encoding: 0x2550C000
    // Test PTEST_.P.P__ field Pn = 0 (Min)
    // Fields: Pn=0, Pg=0
    let encoding: u32 = 0x2550C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTEST_.P.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_ptest_p_p_field_pn_1_poweroftwo_c000_2550c020() {
    // Encoding: 0x2550C020
    // Test PTEST_.P.P__ field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, Pg=0
    let encoding: u32 = 0x2550C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTEST_.P.P__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_ptest_p_p_combo_0_c000_2550c000() {
    // Encoding: 0x2550C000
    // Test PTEST_.P.P__ field combination: Pg=0, Pn=0
    // Fields: Pg=0, Pn=0
    let encoding: u32 = 0x2550C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PTEST_.P.P__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_ptest_p_p_invalid_0_c000_2550c000() {
    // Encoding: 0x2550C000
    // Test PTEST_.P.P__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0, Pg=0
    let encoding: u32 = 0x2550C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PTEST_.P.P__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_ptest_p_p_invalid_1_c000_2550c000() {
    // Encoding: 0x2550C000
    // Test PTEST_.P.P__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pn=0
    let encoding: u32 = 0x2550C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_ptest_p_p_flags_zeroresult_0_2550c000() {
    // Test PTEST_.P.P__ flag computation: ZeroResult
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_ptest_p_p_flags_zeroresult_1_2550c000() {
    // Test PTEST_.P.P__ flag computation: ZeroResult
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_ptest_p_p_flags_negativeresult_2_2550c000() {
    // Test PTEST_.P.P__ flag computation: NegativeResult
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_ptest_p_p_flags_unsignedoverflow_3_2550c000() {
    // Test PTEST_.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_ptest_p_p_flags_unsignedoverflow_4_2550c000() {
    // Test PTEST_.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_ptest_p_p_flags_signedoverflow_5_2550c000() {
    // Test PTEST_.P.P__ flag computation: SignedOverflow
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_ptest_p_p_flags_signedoverflow_6_2550c000() {
    // Test PTEST_.P.P__ flag computation: SignedOverflow
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PTEST_.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_ptest_p_p_flags_positiveresult_7_2550c000() {
    // Test PTEST_.P.P__ flag computation: PositiveResult
    // Encoding: 0x2550C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x2550C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// ORN_P.P.PP_Z Tests
// ============================================================================

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orn_p_p_pp_z_field_pm_0_min_4010_25804010() {
    // Encoding: 0x25804010
    // Test ORN_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pd=0, Pm=0, Pn=0, Pg=0
    let encoding: u32 = 0x25804010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orn_p_p_pp_z_field_pm_1_poweroftwo_4010_25814010() {
    // Encoding: 0x25814010
    // Test ORN_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pm=1, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25814010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orn_p_p_pp_z_field_pg_0_min_4010_25804010() {
    // Encoding: 0x25804010
    // Test ORN_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25804010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orn_p_p_pp_z_field_pg_1_poweroftwo_4010_25804410() {
    // Encoding: 0x25804410
    // Test ORN_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pm=0, Pd=0, Pg=1, Pn=0
    let encoding: u32 = 0x25804410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orn_p_p_pp_z_field_pn_0_min_4010_25804010() {
    // Encoding: 0x25804010
    // Test ORN_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pg=0, Pn=0, Pd=0, Pm=0
    let encoding: u32 = 0x25804010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orn_p_p_pp_z_field_pn_1_poweroftwo_4010_25804030() {
    // Encoding: 0x25804030
    // Test ORN_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pm=0, Pn=1, Pd=0, Pg=0
    let encoding: u32 = 0x25804030;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orn_p_p_pp_z_field_pd_0_min_4010_25804010() {
    // Encoding: 0x25804010
    // Test ORN_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pm=0, Pn=0, Pg=0, Pd=0
    let encoding: u32 = 0x25804010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orn_p_p_pp_z_field_pd_1_poweroftwo_4010_25804011() {
    // Encoding: 0x25804011
    // Test ORN_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pn=0, Pm=0, Pg=0
    let encoding: u32 = 0x25804011;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_orn_p_p_pp_z_combo_0_4010_25804010() {
    // Encoding: 0x25804010
    // Test ORN_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pd=0, Pg=0, Pn=0, Pm=0
    let encoding: u32 = 0x25804010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_orn_p_p_pp_z_invalid_0_4010_25804010() {
    // Encoding: 0x25804010
    // Test ORN_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0, Pn=0, Pg=0, Pm=0
    let encoding: u32 = 0x25804010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_orn_p_p_pp_z_invalid_1_4010_25804010() {
    // Encoding: 0x25804010
    // Test ORN_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pg=0, Pd=0, Pm=0
    let encoding: u32 = 0x25804010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orns_p_p_pp_z_field_pm_0_min_4010_25c04010() {
    // Encoding: 0x25C04010
    // Test ORNS_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pg=0, Pm=0, Pd=0, Pn=0
    let encoding: u32 = 0x25C04010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orns_p_p_pp_z_field_pm_1_poweroftwo_4010_25c14010() {
    // Encoding: 0x25C14010
    // Test ORNS_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pm=1, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25C14010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orns_p_p_pp_z_field_pg_0_min_4010_25c04010() {
    // Encoding: 0x25C04010
    // Test ORNS_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pd=0, Pn=0, Pg=0, Pm=0
    let encoding: u32 = 0x25C04010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orns_p_p_pp_z_field_pg_1_poweroftwo_4010_25c04410() {
    // Encoding: 0x25C04410
    // Test ORNS_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pn=0, Pm=0, Pd=0, Pg=1
    let encoding: u32 = 0x25C04410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orns_p_p_pp_z_field_pn_0_min_4010_25c04010() {
    // Encoding: 0x25C04010
    // Test ORNS_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pd=0, Pg=0, Pn=0, Pm=0
    let encoding: u32 = 0x25C04010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orns_p_p_pp_z_field_pn_1_poweroftwo_4010_25c04030() {
    // Encoding: 0x25C04030
    // Test ORNS_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pd=0, Pn=1, Pm=0, Pg=0
    let encoding: u32 = 0x25C04030;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_orns_p_p_pp_z_field_pd_0_min_4010_25c04010() {
    // Encoding: 0x25C04010
    // Test ORNS_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pg=0, Pm=0, Pd=0, Pn=0
    let encoding: u32 = 0x25C04010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_orns_p_p_pp_z_field_pd_1_poweroftwo_4010_25c04011() {
    // Encoding: 0x25C04011
    // Test ORNS_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pm=0, Pd=1, Pg=0, Pn=0
    let encoding: u32 = 0x25C04011;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_orns_p_p_pp_z_combo_0_4010_25c04010() {
    // Encoding: 0x25C04010
    // Test ORNS_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pn=0, Pm=0, Pg=0, Pd=0
    let encoding: u32 = 0x25C04010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_orns_p_p_pp_z_invalid_0_4010_25c04010() {
    // Encoding: 0x25C04010
    // Test ORNS_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0, Pg=0, Pm=0, Pn=0
    let encoding: u32 = 0x25C04010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_orns_p_p_pp_z_invalid_1_4010_25c04010() {
    // Encoding: 0x25C04010
    // Test ORNS_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25C04010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_orn_p_p_pp_z_reg_write_0_25804010() {
    // Test ORN_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_orn_p_p_pp_z_flags_zeroresult_0_25804010() {
    // Test ORN_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_orn_p_p_pp_z_flags_zeroresult_1_25804010() {
    // Test ORN_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_orn_p_p_pp_z_flags_negativeresult_2_25804010() {
    // Test ORN_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_orn_p_p_pp_z_flags_unsignedoverflow_3_25804010() {
    // Test ORN_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_orn_p_p_pp_z_flags_unsignedoverflow_4_25804010() {
    // Test ORN_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_orn_p_p_pp_z_flags_signedoverflow_5_25804010() {
    // Test ORN_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_orn_p_p_pp_z_flags_signedoverflow_6_25804010() {
    // Test ORN_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORN_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_orn_p_p_pp_z_flags_positiveresult_7_25804010() {
    // Test ORN_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25804010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25804010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_orns_p_p_pp_z_reg_write_0_25c04010() {
    // Test ORNS_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_orns_p_p_pp_z_flags_zeroresult_0_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_orns_p_p_pp_z_flags_zeroresult_1_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_orns_p_p_pp_z_flags_negativeresult_2_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_orns_p_p_pp_z_flags_unsignedoverflow_3_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_orns_p_p_pp_z_flags_unsignedoverflow_4_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_orns_p_p_pp_z_flags_signedoverflow_5_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_orns_p_p_pp_z_flags_signedoverflow_6_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: ORNS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_orns_p_p_pp_z_flags_positiveresult_7_25c04010() {
    // Test ORNS_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25C04010
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25C04010;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// PFALSE_P__ Tests
// ============================================================================

/// Provenance: PFALSE_P__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_pfalse_p_field_pd_0_min_e400_2518e400() {
    // Encoding: 0x2518E400
    // Test PFALSE_P__ field Pd = 0 (Min)
    // Fields: Pd=0
    let encoding: u32 = 0x2518E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFALSE_P__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_pfalse_p_field_pd_1_poweroftwo_e400_2518e401() {
    // Encoding: 0x2518E401
    // Test PFALSE_P__ field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1
    let encoding: u32 = 0x2518E401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFALSE_P__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pd=0 (register index 0 (first register))
#[test]
fn test_pfalse_p_combo_0_e400_2518e400() {
    // Encoding: 0x2518E400
    // Test PFALSE_P__ field combination: Pd=0
    // Fields: Pd=0
    let encoding: u32 = 0x2518E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PFALSE_P__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_pfalse_p_invalid_0_e400_2518e400() {
    // Encoding: 0x2518E400
    // Test PFALSE_P__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0
    let encoding: u32 = 0x2518E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PFALSE_P__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_pfalse_p_invalid_1_e400_2518e400() {
    // Encoding: 0x2518E400
    // Test PFALSE_P__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pd=0
    let encoding: u32 = 0x2518E400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PFALSE_P__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_pfalse_p_reg_write_0_2518e400() {
    // Test PFALSE_P__ register write: SimdFromField("Pd")
    // Encoding: 0x2518E400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2518E400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

// ============================================================================
// WHILELE_P.P.RR__ Tests
// ============================================================================

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilele_p_p_rr_field_size_0_min_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ field size = 0 (Min)
    // Fields: sf=0, Rn=0, Pd=0, size=0, Rm=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_whilele_p_p_rr_field_size_1_poweroftwo_410_25600410() {
    // Encoding: 0x25600410
    // Test WHILELE_P.P.RR__ field size = 1 (PowerOfTwo)
    // Fields: size=1, Rm=0, sf=0, Rn=0, Pd=0
    let encoding: u32 = 0x25600410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_whilele_p_p_rr_field_size_2_poweroftwo_410_25a00410() {
    // Encoding: 0x25A00410
    // Test WHILELE_P.P.RR__ field size = 2 (PowerOfTwo)
    // Fields: size=2, Rm=0, Pd=0, Rn=0, sf=0
    let encoding: u32 = 0x25A00410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_whilele_p_p_rr_field_size_3_max_410_25e00410() {
    // Encoding: 0x25E00410
    // Test WHILELE_P.P.RR__ field size = 3 (Max)
    // Fields: Rm=0, sf=0, Rn=0, Pd=0, size=3
    let encoding: u32 = 0x25E00410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilele_p_p_rr_field_rm_0_min_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ field Rm = 0 (Min)
    // Fields: Rn=0, Rm=0, Pd=0, sf=0, size=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilele_p_p_rr_field_rm_1_poweroftwo_410_25210410() {
    // Encoding: 0x25210410
    // Test WHILELE_P.P.RR__ field Rm = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=1, Pd=0, sf=0, size=0
    let encoding: u32 = 0x25210410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilele_p_p_rr_field_rm_30_poweroftwominusone_410_253e0410() {
    // Encoding: 0x253E0410
    // Test WHILELE_P.P.RR__ field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: Pd=0, Rm=30, Rn=0, sf=0, size=0
    let encoding: u32 = 0x253E0410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_whilele_p_p_rr_field_rm_31_max_410_253f0410() {
    // Encoding: 0x253F0410
    // Test WHILELE_P.P.RR__ field Rm = 31 (Max)
    // Fields: Rm=31, sf=0, size=0, Rn=0, Pd=0
    let encoding: u32 = 0x253F0410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilele_p_p_rr_field_sf_0_min_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ field sf = 0 (Min)
    // Fields: size=0, Rn=0, Pd=0, sf=0, Rm=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_whilele_p_p_rr_field_sf_1_max_410_25201410() {
    // Encoding: 0x25201410
    // Test WHILELE_P.P.RR__ field sf = 1 (Max)
    // Fields: Rm=0, Rn=0, sf=1, size=0, Pd=0
    let encoding: u32 = 0x25201410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilele_p_p_rr_field_rn_0_min_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ field Rn = 0 (Min)
    // Fields: size=0, sf=0, Pd=0, Rm=0, Rn=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilele_p_p_rr_field_rn_1_poweroftwo_410_25200430() {
    // Encoding: 0x25200430
    // Test WHILELE_P.P.RR__ field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, Rm=0, sf=0, Pd=0, size=0
    let encoding: u32 = 0x25200430;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilele_p_p_rr_field_rn_30_poweroftwominusone_410_252007d0() {
    // Encoding: 0x252007D0
    // Test WHILELE_P.P.RR__ field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Rm=0, size=0, sf=0, Pd=0, Rn=30
    let encoding: u32 = 0x252007D0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_whilele_p_p_rr_field_rn_31_max_410_252007f0() {
    // Encoding: 0x252007F0
    // Test WHILELE_P.P.RR__ field Rn = 31 (Max)
    // Fields: Rm=0, Pd=0, size=0, sf=0, Rn=31
    let encoding: u32 = 0x252007F0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilele_p_p_rr_field_pd_0_min_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ field Pd = 0 (Min)
    // Fields: Pd=0, Rm=0, Rn=0, sf=0, size=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilele_p_p_rr_field_pd_1_poweroftwo_410_25200411() {
    // Encoding: 0x25200411
    // Test WHILELE_P.P.RR__ field Pd = 1 (PowerOfTwo)
    // Fields: sf=0, Pd=1, size=0, Rm=0, Rn=0
    let encoding: u32 = 0x25200411;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_whilele_p_p_rr_combo_0_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ field combination: size=0, Rm=0, sf=0, Rn=0, Pd=0
    // Fields: Rn=0, size=0, Rm=0, Pd=0, sf=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilele_p_p_rr_special_size_0_size_variant_0_1040_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ special value size = 0 (Size variant 0)
    // Fields: Rn=0, Pd=0, Rm=0, size=0, sf=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilele_p_p_rr_special_size_1_size_variant_1_1040_25600410() {
    // Encoding: 0x25600410
    // Test WHILELE_P.P.RR__ special value size = 1 (Size variant 1)
    // Fields: Rn=0, sf=0, Pd=0, size=1, Rm=0
    let encoding: u32 = 0x25600410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_whilele_p_p_rr_special_size_2_size_variant_2_1040_25a00410() {
    // Encoding: 0x25A00410
    // Test WHILELE_P.P.RR__ special value size = 2 (Size variant 2)
    // Fields: Rm=0, size=2, sf=0, Rn=0, Pd=0
    let encoding: u32 = 0x25A00410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_whilele_p_p_rr_special_size_3_size_variant_3_1040_25e00410() {
    // Encoding: 0x25E00410
    // Test WHILELE_P.P.RR__ special value size = 3 (Size variant 3)
    // Fields: size=3, sf=0, Rn=0, Rm=0, Pd=0
    let encoding: u32 = 0x25E00410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilele_p_p_rr_special_sf_0_size_variant_0_1040_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ special value sf = 0 (Size variant 0)
    // Fields: size=0, Rm=0, Rn=0, Pd=0, sf=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilele_p_p_rr_special_sf_1_size_variant_1_1040_25201410() {
    // Encoding: 0x25201410
    // Test WHILELE_P.P.RR__ special value sf = 1 (Size variant 1)
    // Fields: size=0, Rm=0, Pd=0, Rn=0, sf=1
    let encoding: u32 = 0x25201410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_whilele_p_p_rr_special_rn_31_stack_pointer_sp_may_require_alignment_1040_252007f0() {
    // Encoding: 0x252007F0
    // Test WHILELE_P.P.RR__ special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Pd=0, size=0, sf=0, Rm=0, Rn=31
    let encoding: u32 = 0x252007F0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_whilele_p_p_rr_invalid_0_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Rm=0, sf=0, Rn=0, size=0, Pd=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_whilele_p_p_rr_invalid_1_410_25200410() {
    // Encoding: 0x25200410
    // Test WHILELE_P.P.RR__ invalid encoding: Unconditional UNDEFINED
    // Fields: Rn=0, size=0, sf=0, Rm=0, Pd=0
    let encoding: u32 = 0x25200410;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_whilele_p_p_rr_reg_write_0_25200410() {
    // Test WHILELE_P.P.RR__ register write: SimdFromField("Pd")
    // Encoding: 0x25200410
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25200410;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_whilele_p_p_rr_sp_rn_252007f0() {
    // Test WHILELE_P.P.RR__ with Rn = SP (31)
    // Encoding: 0x252007F0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x252007F0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_whilele_p_p_rr_flags_zeroresult_0_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_whilele_p_p_rr_flags_zeroresult_1_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_whilele_p_p_rr_flags_negativeresult_2_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: NegativeResult
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_whilele_p_p_rr_flags_unsignedoverflow_3_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_whilele_p_p_rr_flags_unsignedoverflow_4_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_whilele_p_p_rr_flags_signedoverflow_5_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_whilele_p_p_rr_flags_signedoverflow_6_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELE_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_whilele_p_p_rr_flags_positiveresult_7_25220430() {
    // Test WHILELE_P.P.RR__ flag computation: PositiveResult
    // Encoding: 0x25220430
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25220430;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// BRKN_P.P.PP__ Tests
// ============================================================================

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkn_p_p_pp_field_pg_0_min_4000_25184000() {
    // Encoding: 0x25184000
    // Test BRKN_P.P.PP__ field Pg = 0 (Min)
    // Fields: Pn=0, Pdm=0, Pg=0
    let encoding: u32 = 0x25184000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkn_p_p_pp_field_pg_1_poweroftwo_4000_25184400() {
    // Encoding: 0x25184400
    // Test BRKN_P.P.PP__ field Pg = 1 (PowerOfTwo)
    // Fields: Pdm=0, Pn=0, Pg=1
    let encoding: u32 = 0x25184400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkn_p_p_pp_field_pn_0_min_4000_25184000() {
    // Encoding: 0x25184000
    // Test BRKN_P.P.PP__ field Pn = 0 (Min)
    // Fields: Pn=0, Pdm=0, Pg=0
    let encoding: u32 = 0x25184000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkn_p_p_pp_field_pn_1_poweroftwo_4000_25184020() {
    // Encoding: 0x25184020
    // Test BRKN_P.P.PP__ field Pn = 1 (PowerOfTwo)
    // Fields: Pdm=0, Pg=0, Pn=1
    let encoding: u32 = 0x25184020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_brkn_p_p_pp_field_pdm_0_min_4000_25184000() {
    // Encoding: 0x25184000
    // Test BRKN_P.P.PP__ field Pdm = 0 (Min)
    // Fields: Pg=0, Pn=0, Pdm=0
    let encoding: u32 = 0x25184000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_brkn_p_p_pp_field_pdm_1_poweroftwo_4000_25184001() {
    // Encoding: 0x25184001
    // Test BRKN_P.P.PP__ field Pdm = 1 (PowerOfTwo)
    // Fields: Pdm=1, Pg=0, Pn=0
    let encoding: u32 = 0x25184001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 7, boundary: PowerOfTwoMinusOne }
/// midpoint (7)
#[test]
fn test_brkn_p_p_pp_field_pdm_7_poweroftwominusone_4000_25184007() {
    // Encoding: 0x25184007
    // Test BRKN_P.P.PP__ field Pdm = 7 (PowerOfTwoMinusOne)
    // Fields: Pdm=7, Pn=0, Pg=0
    let encoding: u32 = 0x25184007;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 15, boundary: Max }
/// maximum value (15)
#[test]
fn test_brkn_p_p_pp_field_pdm_15_max_4000_2518400f() {
    // Encoding: 0x2518400F
    // Test BRKN_P.P.PP__ field Pdm = 15 (Max)
    // Fields: Pg=0, Pn=0, Pdm=15
    let encoding: u32 = 0x2518400F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_brkn_p_p_pp_combo_0_4000_25184000() {
    // Encoding: 0x25184000
    // Test BRKN_P.P.PP__ field combination: Pg=0, Pn=0, Pdm=0
    // Fields: Pdm=0, Pn=0, Pg=0
    let encoding: u32 = 0x25184000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkn_p_p_pp_invalid_0_4000_25184000() {
    // Encoding: 0x25184000
    // Test BRKN_P.P.PP__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pdm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25184000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkn_p_p_pp_invalid_1_4000_25184000() {
    // Encoding: 0x25184000
    // Test BRKN_P.P.PP__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pdm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25184000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkns_p_p_pp_field_pg_0_min_4000_25584000() {
    // Encoding: 0x25584000
    // Test BRKNS_P.P.PP__ field Pg = 0 (Min)
    // Fields: Pg=0, Pn=0, Pdm=0
    let encoding: u32 = 0x25584000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkns_p_p_pp_field_pg_1_poweroftwo_4000_25584400() {
    // Encoding: 0x25584400
    // Test BRKNS_P.P.PP__ field Pg = 1 (PowerOfTwo)
    // Fields: Pdm=0, Pn=0, Pg=1
    let encoding: u32 = 0x25584400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkns_p_p_pp_field_pn_0_min_4000_25584000() {
    // Encoding: 0x25584000
    // Test BRKNS_P.P.PP__ field Pn = 0 (Min)
    // Fields: Pg=0, Pdm=0, Pn=0
    let encoding: u32 = 0x25584000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkns_p_p_pp_field_pn_1_poweroftwo_4000_25584020() {
    // Encoding: 0x25584020
    // Test BRKNS_P.P.PP__ field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, Pdm=0, Pg=0
    let encoding: u32 = 0x25584020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_brkns_p_p_pp_field_pdm_0_min_4000_25584000() {
    // Encoding: 0x25584000
    // Test BRKNS_P.P.PP__ field Pdm = 0 (Min)
    // Fields: Pn=0, Pdm=0, Pg=0
    let encoding: u32 = 0x25584000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_brkns_p_p_pp_field_pdm_1_poweroftwo_4000_25584001() {
    // Encoding: 0x25584001
    // Test BRKNS_P.P.PP__ field Pdm = 1 (PowerOfTwo)
    // Fields: Pdm=1, Pn=0, Pg=0
    let encoding: u32 = 0x25584001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 7, boundary: PowerOfTwoMinusOne }
/// midpoint (7)
#[test]
fn test_brkns_p_p_pp_field_pdm_7_poweroftwominusone_4000_25584007() {
    // Encoding: 0x25584007
    // Test BRKNS_P.P.PP__ field Pdm = 7 (PowerOfTwoMinusOne)
    // Fields: Pdm=7, Pn=0, Pg=0
    let encoding: u32 = 0x25584007;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field Pdm 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdm", value: 15, boundary: Max }
/// maximum value (15)
#[test]
fn test_brkns_p_p_pp_field_pdm_15_max_4000_2558400f() {
    // Encoding: 0x2558400F
    // Test BRKNS_P.P.PP__ field Pdm = 15 (Max)
    // Fields: Pg=0, Pdm=15, Pn=0
    let encoding: u32 = 0x2558400F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_brkns_p_p_pp_combo_0_4000_25584000() {
    // Encoding: 0x25584000
    // Test BRKNS_P.P.PP__ field combination: Pg=0, Pn=0, Pdm=0
    // Fields: Pg=0, Pn=0, Pdm=0
    let encoding: u32 = 0x25584000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkns_p_p_pp_invalid_0_4000_25584000() {
    // Encoding: 0x25584000
    // Test BRKNS_P.P.PP__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Pdm=0, Pn=0
    let encoding: u32 = 0x25584000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkns_p_p_pp_invalid_1_4000_25584000() {
    // Encoding: 0x25584000
    // Test BRKNS_P.P.PP__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pdm=0, Pg=0
    let encoding: u32 = 0x25584000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `SimdFromField("Pdm") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pdm")
#[test]
fn test_brkn_p_p_pp_reg_write_0_25184000() {
    // Test BRKN_P.P.PP__ register write: SimdFromField("Pdm")
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkn_p_p_pp_flags_zeroresult_0_25184000() {
    // Test BRKN_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkn_p_p_pp_flags_zeroresult_1_25184000() {
    // Test BRKN_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkn_p_p_pp_flags_negativeresult_2_25184000() {
    // Test BRKN_P.P.PP__ flag computation: NegativeResult
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkn_p_p_pp_flags_unsignedoverflow_3_25184000() {
    // Test BRKN_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkn_p_p_pp_flags_unsignedoverflow_4_25184000() {
    // Test BRKN_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkn_p_p_pp_flags_signedoverflow_5_25184000() {
    // Test BRKN_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkn_p_p_pp_flags_signedoverflow_6_25184000() {
    // Test BRKN_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKN_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkn_p_p_pp_flags_positiveresult_7_25184000() {
    // Test BRKN_P.P.PP__ flag computation: PositiveResult
    // Encoding: 0x25184000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25184000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `SimdFromField("Pdm") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pdm")
#[test]
fn test_brkns_p_p_pp_reg_write_0_25584000() {
    // Test BRKNS_P.P.PP__ register write: SimdFromField("Pdm")
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkns_p_p_pp_flags_zeroresult_0_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkns_p_p_pp_flags_zeroresult_1_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkns_p_p_pp_flags_negativeresult_2_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: NegativeResult
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkns_p_p_pp_flags_unsignedoverflow_3_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkns_p_p_pp_flags_unsignedoverflow_4_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkns_p_p_pp_flags_signedoverflow_5_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkns_p_p_pp_flags_signedoverflow_6_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKNS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkns_p_p_pp_flags_positiveresult_7_25584000() {
    // Test BRKNS_P.P.PP__ flag computation: PositiveResult
    // Encoding: 0x25584000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25584000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// WRFFR_F.P__ Tests
// ============================================================================

/// Provenance: WRFFR_F.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_wrffr_f_p_field_pn_0_min_9000_25289000() {
    // Encoding: 0x25289000
    // Test WRFFR_F.P__ field Pn = 0 (Min)
    // Fields: Pn=0
    let encoding: u32 = 0x25289000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WRFFR_F.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_wrffr_f_p_field_pn_1_poweroftwo_9000_25289020() {
    // Encoding: 0x25289020
    // Test WRFFR_F.P__ field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1
    let encoding: u32 = 0x25289020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WRFFR_F.P__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pn=0 (register index 0 (first register))
#[test]
fn test_wrffr_f_p_combo_0_9000_25289000() {
    // Encoding: 0x25289000
    // Test WRFFR_F.P__ field combination: Pn=0
    // Fields: Pn=0
    let encoding: u32 = 0x25289000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WRFFR_F.P__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_wrffr_f_p_invalid_0_9000_25289000() {
    // Encoding: 0x25289000
    // Test WRFFR_F.P__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0
    let encoding: u32 = 0x25289000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WRFFR_F.P__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_wrffr_f_p_invalid_1_9000_25289000() {
    // Encoding: 0x25289000
    // Test WRFFR_F.P__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0
    let encoding: u32 = 0x25289000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

// ============================================================================
// BRKB_P.P.P__ Tests
// ============================================================================

/// Provenance: BRKB_P.P.P__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkb_p_p_p_field_pg_0_min_4000_25904000() {
    // Encoding: 0x25904000
    // Test BRKB_P.P.P__ field Pg = 0 (Min)
    // Fields: Pn=0, Pd=0, Pg=0, M=0
    let encoding: u32 = 0x25904000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkb_p_p_p_field_pg_1_poweroftwo_4000_25904400() {
    // Encoding: 0x25904400
    // Test BRKB_P.P.P__ field Pg = 1 (PowerOfTwo)
    // Fields: Pn=0, Pg=1, M=0, Pd=0
    let encoding: u32 = 0x25904400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkb_p_p_p_field_pn_0_min_4000_25904000() {
    // Encoding: 0x25904000
    // Test BRKB_P.P.P__ field Pn = 0 (Min)
    // Fields: Pg=0, Pd=0, M=0, Pn=0
    let encoding: u32 = 0x25904000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkb_p_p_p_field_pn_1_poweroftwo_4000_25904020() {
    // Encoding: 0x25904020
    // Test BRKB_P.P.P__ field Pn = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=1, M=0, Pd=0
    let encoding: u32 = 0x25904020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field M 4 +: 1`
/// Requirement: FieldBoundary { field: "M", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_brkb_p_p_p_field_m_0_min_4000_25904000() {
    // Encoding: 0x25904000
    // Test BRKB_P.P.P__ field M = 0 (Min)
    // Fields: Pg=0, M=0, Pn=0, Pd=0
    let encoding: u32 = 0x25904000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field M 4 +: 1`
/// Requirement: FieldBoundary { field: "M", value: 1, boundary: Max }
/// maximum value (1)
#[test]
fn test_brkb_p_p_p_field_m_1_max_4000_25904010() {
    // Encoding: 0x25904010
    // Test BRKB_P.P.P__ field M = 1 (Max)
    // Fields: Pn=0, Pd=0, Pg=0, M=1
    let encoding: u32 = 0x25904010;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkb_p_p_p_field_pd_0_min_4000_25904000() {
    // Encoding: 0x25904000
    // Test BRKB_P.P.P__ field Pd = 0 (Min)
    // Fields: Pn=0, M=0, Pg=0, Pd=0
    let encoding: u32 = 0x25904000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkb_p_p_p_field_pd_1_poweroftwo_4000_25904001() {
    // Encoding: 0x25904001
    // Test BRKB_P.P.P__ field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pn=0, Pg=0, M=0
    let encoding: u32 = 0x25904001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_brkb_p_p_p_combo_0_4000_25904000() {
    // Encoding: 0x25904000
    // Test BRKB_P.P.P__ field combination: Pg=0, Pn=0, M=0, Pd=0
    // Fields: Pg=0, M=0, Pd=0, Pn=0
    let encoding: u32 = 0x25904000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkb_p_p_p_invalid_0_4000_25904000() {
    // Encoding: 0x25904000
    // Test BRKB_P.P.P__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: M=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25904000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkb_p_p_p_invalid_1_4000_25904000() {
    // Encoding: 0x25904000
    // Test BRKB_P.P.P__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pd=0, Pn=0, Pg=0, M=0
    let encoding: u32 = 0x25904000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkbs_p_p_p_z_field_pg_0_min_4000_25d04000() {
    // Encoding: 0x25D04000
    // Test BRKBS_P.P.P_Z field Pg = 0 (Min)
    // Fields: Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25D04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkbs_p_p_p_z_field_pg_1_poweroftwo_4000_25d04400() {
    // Encoding: 0x25D04400
    // Test BRKBS_P.P.P_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pd=0, Pg=1, Pn=0
    let encoding: u32 = 0x25D04400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkbs_p_p_p_z_field_pn_0_min_4000_25d04000() {
    // Encoding: 0x25D04000
    // Test BRKBS_P.P.P_Z field Pn = 0 (Min)
    // Fields: Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x25D04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkbs_p_p_p_z_field_pn_1_poweroftwo_4000_25d04020() {
    // Encoding: 0x25D04020
    // Test BRKBS_P.P.P_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pd=0, Pn=1, Pg=0
    let encoding: u32 = 0x25D04020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkbs_p_p_p_z_field_pd_0_min_4000_25d04000() {
    // Encoding: 0x25D04000
    // Test BRKBS_P.P.P_Z field Pd = 0 (Min)
    // Fields: Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25D04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkbs_p_p_p_z_field_pd_1_poweroftwo_4000_25d04001() {
    // Encoding: 0x25D04001
    // Test BRKBS_P.P.P_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pg=0, Pn=0, Pd=1
    let encoding: u32 = 0x25D04001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pg=0 (register index 0 (first register))
#[test]
fn test_brkbs_p_p_p_z_combo_0_4000_25d04000() {
    // Encoding: 0x25D04000
    // Test BRKBS_P.P.P_Z field combination: Pg=0, Pn=0, Pd=0
    // Fields: Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25D04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkbs_p_p_p_z_invalid_0_4000_25d04000() {
    // Encoding: 0x25D04000
    // Test BRKBS_P.P.P_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25D04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkbs_p_p_p_z_invalid_1_4000_25d04000() {
    // Encoding: 0x25D04000
    // Test BRKBS_P.P.P_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pd=0, Pg=0
    let encoding: u32 = 0x25D04000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKB_P.P.P__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brkb_p_p_p_reg_write_0_25904000() {
    // Test BRKB_P.P.P__ register write: SimdFromField("Pd")
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkb_p_p_p_flags_zeroresult_0_25904000() {
    // Test BRKB_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkb_p_p_p_flags_zeroresult_1_25904000() {
    // Test BRKB_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkb_p_p_p_flags_negativeresult_2_25904000() {
    // Test BRKB_P.P.P__ flag computation: NegativeResult
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkb_p_p_p_flags_unsignedoverflow_3_25904000() {
    // Test BRKB_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkb_p_p_p_flags_unsignedoverflow_4_25904000() {
    // Test BRKB_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkb_p_p_p_flags_signedoverflow_5_25904000() {
    // Test BRKB_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkb_p_p_p_flags_signedoverflow_6_25904000() {
    // Test BRKB_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKB_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkb_p_p_p_flags_positiveresult_7_25904000() {
    // Test BRKB_P.P.P__ flag computation: PositiveResult
    // Encoding: 0x25904000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25904000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brkbs_p_p_p_z_reg_write_0_25d04000() {
    // Test BRKBS_P.P.P_Z register write: SimdFromField("Pd")
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkbs_p_p_p_z_flags_zeroresult_0_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: ZeroResult
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkbs_p_p_p_z_flags_zeroresult_1_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: ZeroResult
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkbs_p_p_p_z_flags_negativeresult_2_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: NegativeResult
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkbs_p_p_p_z_flags_unsignedoverflow_3_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: UnsignedOverflow
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkbs_p_p_p_z_flags_unsignedoverflow_4_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: UnsignedOverflow
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkbs_p_p_p_z_flags_signedoverflow_5_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: SignedOverflow
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkbs_p_p_p_z_flags_signedoverflow_6_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: SignedOverflow
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKBS_P.P.P_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkbs_p_p_p_z_flags_positiveresult_7_25d04000() {
    // Test BRKBS_P.P.P_Z flag computation: PositiveResult
    // Encoding: 0x25D04000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25D04000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// BRKPA_P.P.PP__ Tests
// ============================================================================

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpa_p_p_pp_field_pm_0_min_c000_2500c000() {
    // Encoding: 0x2500C000
    // Test BRKPA_P.P.PP__ field Pm = 0 (Min)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x2500C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpa_p_p_pp_field_pm_1_poweroftwo_c000_2501c000() {
    // Encoding: 0x2501C000
    // Test BRKPA_P.P.PP__ field Pm = 1 (PowerOfTwo)
    // Fields: Pd=0, Pn=0, Pg=0, Pm=1
    let encoding: u32 = 0x2501C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpa_p_p_pp_field_pg_0_min_c000_2500c000() {
    // Encoding: 0x2500C000
    // Test BRKPA_P.P.PP__ field Pg = 0 (Min)
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x2500C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpa_p_p_pp_field_pg_1_poweroftwo_c000_2500c400() {
    // Encoding: 0x2500C400
    // Test BRKPA_P.P.PP__ field Pg = 1 (PowerOfTwo)
    // Fields: Pg=1, Pn=0, Pd=0, Pm=0
    let encoding: u32 = 0x2500C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpa_p_p_pp_field_pn_0_min_c000_2500c000() {
    // Encoding: 0x2500C000
    // Test BRKPA_P.P.PP__ field Pn = 0 (Min)
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x2500C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpa_p_p_pp_field_pn_1_poweroftwo_c000_2500c020() {
    // Encoding: 0x2500C020
    // Test BRKPA_P.P.PP__ field Pn = 1 (PowerOfTwo)
    // Fields: Pm=0, Pg=0, Pn=1, Pd=0
    let encoding: u32 = 0x2500C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpa_p_p_pp_field_pd_0_min_c000_2500c000() {
    // Encoding: 0x2500C000
    // Test BRKPA_P.P.PP__ field Pd = 0 (Min)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x2500C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpa_p_p_pp_field_pd_1_poweroftwo_c000_2500c001() {
    // Encoding: 0x2500C001
    // Test BRKPA_P.P.PP__ field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pn=0, Pm=0, Pg=0
    let encoding: u32 = 0x2500C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_brkpa_p_p_pp_combo_0_c000_2500c000() {
    // Encoding: 0x2500C000
    // Test BRKPA_P.P.PP__ field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pn=0, Pm=0, Pg=0, Pd=0
    let encoding: u32 = 0x2500C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkpa_p_p_pp_invalid_0_c000_2500c000() {
    // Encoding: 0x2500C000
    // Test BRKPA_P.P.PP__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0, Pm=0, Pg=0, Pd=0
    let encoding: u32 = 0x2500C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkpa_p_p_pp_invalid_1_c000_2500c000() {
    // Encoding: 0x2500C000
    // Test BRKPA_P.P.PP__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pn=0, Pd=0, Pg=0, Pm=0
    let encoding: u32 = 0x2500C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpas_p_p_pp_field_pm_0_min_c000_2540c000() {
    // Encoding: 0x2540C000
    // Test BRKPAS_P.P.PP__ field Pm = 0 (Min)
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x2540C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpas_p_p_pp_field_pm_1_poweroftwo_c000_2541c000() {
    // Encoding: 0x2541C000
    // Test BRKPAS_P.P.PP__ field Pm = 1 (PowerOfTwo)
    // Fields: Pn=0, Pd=0, Pg=0, Pm=1
    let encoding: u32 = 0x2541C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpas_p_p_pp_field_pg_0_min_c000_2540c000() {
    // Encoding: 0x2540C000
    // Test BRKPAS_P.P.PP__ field Pg = 0 (Min)
    // Fields: Pg=0, Pn=0, Pm=0, Pd=0
    let encoding: u32 = 0x2540C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpas_p_p_pp_field_pg_1_poweroftwo_c000_2540c400() {
    // Encoding: 0x2540C400
    // Test BRKPAS_P.P.PP__ field Pg = 1 (PowerOfTwo)
    // Fields: Pm=0, Pg=1, Pn=0, Pd=0
    let encoding: u32 = 0x2540C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpas_p_p_pp_field_pn_0_min_c000_2540c000() {
    // Encoding: 0x2540C000
    // Test BRKPAS_P.P.PP__ field Pn = 0 (Min)
    // Fields: Pg=0, Pn=0, Pd=0, Pm=0
    let encoding: u32 = 0x2540C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpas_p_p_pp_field_pn_1_poweroftwo_c000_2540c020() {
    // Encoding: 0x2540C020
    // Test BRKPAS_P.P.PP__ field Pn = 1 (PowerOfTwo)
    // Fields: Pg=0, Pm=0, Pd=0, Pn=1
    let encoding: u32 = 0x2540C020;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_brkpas_p_p_pp_field_pd_0_min_c000_2540c000() {
    // Encoding: 0x2540C000
    // Test BRKPAS_P.P.PP__ field Pd = 0 (Min)
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x2540C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_brkpas_p_p_pp_field_pd_1_poweroftwo_c000_2540c001() {
    // Encoding: 0x2540C001
    // Test BRKPAS_P.P.PP__ field Pd = 1 (PowerOfTwo)
    // Fields: Pn=0, Pm=0, Pd=1, Pg=0
    let encoding: u32 = 0x2540C001;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_brkpas_p_p_pp_combo_0_c000_2540c000() {
    // Encoding: 0x2540C000
    // Test BRKPAS_P.P.PP__ field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x2540C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_brkpas_p_p_pp_invalid_0_c000_2540c000() {
    // Encoding: 0x2540C000
    // Test BRKPAS_P.P.PP__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, Pm=0, Pn=0, Pd=0
    let encoding: u32 = 0x2540C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_brkpas_p_p_pp_invalid_1_c000_2540c000() {
    // Encoding: 0x2540C000
    // Test BRKPAS_P.P.PP__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pg=0, Pd=0, Pm=0, Pn=0
    let encoding: u32 = 0x2540C000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brkpa_p_p_pp_reg_write_0_2500c000() {
    // Test BRKPA_P.P.PP__ register write: SimdFromField("Pd")
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkpa_p_p_pp_flags_zeroresult_0_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkpa_p_p_pp_flags_zeroresult_1_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkpa_p_p_pp_flags_negativeresult_2_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: NegativeResult
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkpa_p_p_pp_flags_unsignedoverflow_3_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkpa_p_p_pp_flags_unsignedoverflow_4_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkpa_p_p_pp_flags_signedoverflow_5_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkpa_p_p_pp_flags_signedoverflow_6_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPA_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkpa_p_p_pp_flags_positiveresult_7_2500c000() {
    // Test BRKPA_P.P.PP__ flag computation: PositiveResult
    // Encoding: 0x2500C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x2500C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_brkpas_p_p_pp_reg_write_0_2540c000() {
    // Test BRKPAS_P.P.PP__ register write: SimdFromField("Pd")
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_brkpas_p_p_pp_flags_zeroresult_0_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_brkpas_p_p_pp_flags_zeroresult_1_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: ZeroResult
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_brkpas_p_p_pp_flags_negativeresult_2_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: NegativeResult
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_brkpas_p_p_pp_flags_unsignedoverflow_3_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_brkpas_p_p_pp_flags_unsignedoverflow_4_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: UnsignedOverflow
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_brkpas_p_p_pp_flags_signedoverflow_5_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_brkpas_p_p_pp_flags_signedoverflow_6_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: SignedOverflow
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: BRKPAS_P.P.PP__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_brkpas_p_p_pp_flags_positiveresult_7_2540c000() {
    // Test BRKPAS_P.P.PP__ flag computation: PositiveResult
    // Encoding: 0x2540C000
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x2540C000;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// WHILELS_P.P.RR__ Tests
// ============================================================================

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilels_p_p_rr_field_size_0_min_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ field size = 0 (Min)
    // Fields: Rn=0, size=0, Pd=0, sf=0, Rm=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_whilels_p_p_rr_field_size_1_poweroftwo_c10_25600c10() {
    // Encoding: 0x25600C10
    // Test WHILELS_P.P.RR__ field size = 1 (PowerOfTwo)
    // Fields: Pd=0, size=1, Rn=0, sf=0, Rm=0
    let encoding: u32 = 0x25600C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_whilels_p_p_rr_field_size_2_poweroftwo_c10_25a00c10() {
    // Encoding: 0x25A00C10
    // Test WHILELS_P.P.RR__ field size = 2 (PowerOfTwo)
    // Fields: Pd=0, Rn=0, sf=0, Rm=0, size=2
    let encoding: u32 = 0x25A00C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_whilels_p_p_rr_field_size_3_max_c10_25e00c10() {
    // Encoding: 0x25E00C10
    // Test WHILELS_P.P.RR__ field size = 3 (Max)
    // Fields: Pd=0, Rm=0, Rn=0, size=3, sf=0
    let encoding: u32 = 0x25E00C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilels_p_p_rr_field_rm_0_min_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ field Rm = 0 (Min)
    // Fields: sf=0, Rm=0, Rn=0, size=0, Pd=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilels_p_p_rr_field_rm_1_poweroftwo_c10_25210c10() {
    // Encoding: 0x25210C10
    // Test WHILELS_P.P.RR__ field Rm = 1 (PowerOfTwo)
    // Fields: size=0, sf=0, Rm=1, Rn=0, Pd=0
    let encoding: u32 = 0x25210C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilels_p_p_rr_field_rm_30_poweroftwominusone_c10_253e0c10() {
    // Encoding: 0x253E0C10
    // Test WHILELS_P.P.RR__ field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: sf=0, Rn=0, size=0, Pd=0, Rm=30
    let encoding: u32 = 0x253E0C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_whilels_p_p_rr_field_rm_31_max_c10_253f0c10() {
    // Encoding: 0x253F0C10
    // Test WHILELS_P.P.RR__ field Rm = 31 (Max)
    // Fields: Rm=31, Pd=0, Rn=0, size=0, sf=0
    let encoding: u32 = 0x253F0C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilels_p_p_rr_field_sf_0_min_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ field sf = 0 (Min)
    // Fields: sf=0, size=0, Rm=0, Rn=0, Pd=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_whilels_p_p_rr_field_sf_1_max_c10_25201c10() {
    // Encoding: 0x25201C10
    // Test WHILELS_P.P.RR__ field sf = 1 (Max)
    // Fields: Rn=0, Pd=0, size=0, Rm=0, sf=1
    let encoding: u32 = 0x25201C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilels_p_p_rr_field_rn_0_min_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ field Rn = 0 (Min)
    // Fields: size=0, Pd=0, Rm=0, sf=0, Rn=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilels_p_p_rr_field_rn_1_poweroftwo_c10_25200c30() {
    // Encoding: 0x25200C30
    // Test WHILELS_P.P.RR__ field Rn = 1 (PowerOfTwo)
    // Fields: Pd=0, Rm=0, Rn=1, size=0, sf=0
    let encoding: u32 = 0x25200C30;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilels_p_p_rr_field_rn_30_poweroftwominusone_c10_25200fd0() {
    // Encoding: 0x25200FD0
    // Test WHILELS_P.P.RR__ field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rm=0, sf=0, Rn=30, Pd=0
    let encoding: u32 = 0x25200FD0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_whilels_p_p_rr_field_rn_31_max_c10_25200ff0() {
    // Encoding: 0x25200FF0
    // Test WHILELS_P.P.RR__ field Rn = 31 (Max)
    // Fields: Pd=0, sf=0, size=0, Rm=0, Rn=31
    let encoding: u32 = 0x25200FF0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilels_p_p_rr_field_pd_0_min_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ field Pd = 0 (Min)
    // Fields: size=0, Rm=0, sf=0, Rn=0, Pd=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilels_p_p_rr_field_pd_1_poweroftwo_c10_25200c11() {
    // Encoding: 0x25200C11
    // Test WHILELS_P.P.RR__ field Pd = 1 (PowerOfTwo)
    // Fields: Rn=0, Rm=0, size=0, sf=0, Pd=1
    let encoding: u32 = 0x25200C11;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_whilels_p_p_rr_combo_0_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ field combination: size=0, Rm=0, sf=0, Rn=0, Pd=0
    // Fields: Pd=0, Rm=0, Rn=0, size=0, sf=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilels_p_p_rr_special_size_0_size_variant_0_3088_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ special value size = 0 (Size variant 0)
    // Fields: sf=0, Rm=0, size=0, Rn=0, Pd=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilels_p_p_rr_special_size_1_size_variant_1_3088_25600c10() {
    // Encoding: 0x25600C10
    // Test WHILELS_P.P.RR__ special value size = 1 (Size variant 1)
    // Fields: sf=0, Pd=0, size=1, Rm=0, Rn=0
    let encoding: u32 = 0x25600C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_whilels_p_p_rr_special_size_2_size_variant_2_3088_25a00c10() {
    // Encoding: 0x25A00C10
    // Test WHILELS_P.P.RR__ special value size = 2 (Size variant 2)
    // Fields: size=2, sf=0, Rm=0, Rn=0, Pd=0
    let encoding: u32 = 0x25A00C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_whilels_p_p_rr_special_size_3_size_variant_3_3088_25e00c10() {
    // Encoding: 0x25E00C10
    // Test WHILELS_P.P.RR__ special value size = 3 (Size variant 3)
    // Fields: sf=0, Pd=0, Rn=0, size=3, Rm=0
    let encoding: u32 = 0x25E00C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilels_p_p_rr_special_sf_0_size_variant_0_3088_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ special value sf = 0 (Size variant 0)
    // Fields: sf=0, Rn=0, Pd=0, size=0, Rm=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilels_p_p_rr_special_sf_1_size_variant_1_3088_25201c10() {
    // Encoding: 0x25201C10
    // Test WHILELS_P.P.RR__ special value sf = 1 (Size variant 1)
    // Fields: Pd=0, sf=1, Rm=0, Rn=0, size=0
    let encoding: u32 = 0x25201C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_whilels_p_p_rr_special_rn_31_stack_pointer_sp_may_require_alignment_3088_25200ff0() {
    // Encoding: 0x25200FF0
    // Test WHILELS_P.P.RR__ special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: size=0, Rm=0, Rn=31, sf=0, Pd=0
    let encoding: u32 = 0x25200FF0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_whilels_p_p_rr_invalid_0_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: sf=0, Pd=0, size=0, Rm=0, Rn=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_whilels_p_p_rr_invalid_1_c10_25200c10() {
    // Encoding: 0x25200C10
    // Test WHILELS_P.P.RR__ invalid encoding: Unconditional UNDEFINED
    // Fields: size=0, Rn=0, Pd=0, sf=0, Rm=0
    let encoding: u32 = 0x25200C10;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_whilels_p_p_rr_reg_write_0_25200c10() {
    // Test WHILELS_P.P.RR__ register write: SimdFromField("Pd")
    // Encoding: 0x25200C10
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25200C10;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_whilels_p_p_rr_sp_rn_25200ff0() {
    // Test WHILELS_P.P.RR__ with Rn = SP (31)
    // Encoding: 0x25200FF0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25200FF0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_whilels_p_p_rr_flags_zeroresult_0_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_whilels_p_p_rr_flags_zeroresult_1_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_whilels_p_p_rr_flags_negativeresult_2_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: NegativeResult
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_whilels_p_p_rr_flags_unsignedoverflow_3_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_whilels_p_p_rr_flags_unsignedoverflow_4_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_whilels_p_p_rr_flags_signedoverflow_5_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_whilels_p_p_rr_flags_signedoverflow_6_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELS_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_whilels_p_p_rr_flags_positiveresult_7_25220c30() {
    // Test WHILELS_P.P.RR__ flag computation: PositiveResult
    // Encoding: 0x25220C30
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25220C30;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// SETFFR_F__ Tests
// ============================================================================

/// Provenance: SETFFR_F__
/// ASL: `fixed encoding (no variable fields)`
/// Requirement: BasicEncoding
/// instruction with no variable fields
#[test]
fn test_setffr_f_basic_encoding_252c9000() {
    // Encoding: 0x252C9000
    // Test SETFFR_F__ basic encoding
    let encoding: u32 = 0x252C9000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: SETFFR_F__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_setffr_f_invalid_0_9000_252c9000() {
    // Encoding: 0x252C9000
    // Test SETFFR_F__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    let encoding: u32 = 0x252C9000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: SETFFR_F__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_setffr_f_invalid_1_9000_252c9000() {
    // Encoding: 0x252C9000
    // Test SETFFR_F__ invalid encoding: Unconditional UNDEFINED
    let encoding: u32 = 0x252C9000;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

// ============================================================================
// WHILELT_P.P.RR__ Tests
// ============================================================================

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilelt_p_p_rr_field_size_0_min_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ field size = 0 (Min)
    // Fields: size=0, Rn=0, Rm=0, Pd=0, sf=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_whilelt_p_p_rr_field_size_1_poweroftwo_400_25600400() {
    // Encoding: 0x25600400
    // Test WHILELT_P.P.RR__ field size = 1 (PowerOfTwo)
    // Fields: size=1, Pd=0, Rn=0, Rm=0, sf=0
    let encoding: u32 = 0x25600400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_whilelt_p_p_rr_field_size_2_poweroftwo_400_25a00400() {
    // Encoding: 0x25A00400
    // Test WHILELT_P.P.RR__ field size = 2 (PowerOfTwo)
    // Fields: size=2, Rm=0, sf=0, Rn=0, Pd=0
    let encoding: u32 = 0x25A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_whilelt_p_p_rr_field_size_3_max_400_25e00400() {
    // Encoding: 0x25E00400
    // Test WHILELT_P.P.RR__ field size = 3 (Max)
    // Fields: Pd=0, Rm=0, sf=0, Rn=0, size=3
    let encoding: u32 = 0x25E00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilelt_p_p_rr_field_rm_0_min_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ field Rm = 0 (Min)
    // Fields: size=0, Rn=0, sf=0, Rm=0, Pd=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilelt_p_p_rr_field_rm_1_poweroftwo_400_25210400() {
    // Encoding: 0x25210400
    // Test WHILELT_P.P.RR__ field Rm = 1 (PowerOfTwo)
    // Fields: size=0, Pd=0, Rm=1, sf=0, Rn=0
    let encoding: u32 = 0x25210400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilelt_p_p_rr_field_rm_30_poweroftwominusone_400_253e0400() {
    // Encoding: 0x253E0400
    // Test WHILELT_P.P.RR__ field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rn=0, Rm=30, sf=0, Pd=0
    let encoding: u32 = 0x253E0400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_whilelt_p_p_rr_field_rm_31_max_400_253f0400() {
    // Encoding: 0x253F0400
    // Test WHILELT_P.P.RR__ field Rm = 31 (Max)
    // Fields: Rn=0, size=0, Pd=0, Rm=31, sf=0
    let encoding: u32 = 0x253F0400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilelt_p_p_rr_field_sf_0_min_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ field sf = 0 (Min)
    // Fields: Rm=0, sf=0, Pd=0, Rn=0, size=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_whilelt_p_p_rr_field_sf_1_max_400_25201400() {
    // Encoding: 0x25201400
    // Test WHILELT_P.P.RR__ field sf = 1 (Max)
    // Fields: Rn=0, sf=1, size=0, Pd=0, Rm=0
    let encoding: u32 = 0x25201400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilelt_p_p_rr_field_rn_0_min_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ field Rn = 0 (Min)
    // Fields: Rm=0, size=0, Pd=0, Rn=0, sf=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilelt_p_p_rr_field_rn_1_poweroftwo_400_25200420() {
    // Encoding: 0x25200420
    // Test WHILELT_P.P.RR__ field Rn = 1 (PowerOfTwo)
    // Fields: size=0, Pd=0, Rm=0, Rn=1, sf=0
    let encoding: u32 = 0x25200420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilelt_p_p_rr_field_rn_30_poweroftwominusone_400_252007c0() {
    // Encoding: 0x252007C0
    // Test WHILELT_P.P.RR__ field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: Pd=0, sf=0, Rn=30, Rm=0, size=0
    let encoding: u32 = 0x252007C0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_whilelt_p_p_rr_field_rn_31_max_400_252007e0() {
    // Encoding: 0x252007E0
    // Test WHILELT_P.P.RR__ field Rn = 31 (Max)
    // Fields: size=0, Pd=0, Rm=0, Rn=31, sf=0
    let encoding: u32 = 0x252007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilelt_p_p_rr_field_pd_0_min_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ field Pd = 0 (Min)
    // Fields: sf=0, size=0, Rm=0, Rn=0, Pd=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilelt_p_p_rr_field_pd_1_poweroftwo_400_25200401() {
    // Encoding: 0x25200401
    // Test WHILELT_P.P.RR__ field Pd = 1 (PowerOfTwo)
    // Fields: sf=0, size=0, Pd=1, Rm=0, Rn=0
    let encoding: u32 = 0x25200401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_whilelt_p_p_rr_combo_0_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ field combination: size=0, Rm=0, sf=0, Rn=0, Pd=0
    // Fields: size=0, sf=0, Rn=0, Pd=0, Rm=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilelt_p_p_rr_special_size_0_size_variant_0_1024_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ special value size = 0 (Size variant 0)
    // Fields: size=0, Pd=0, sf=0, Rm=0, Rn=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilelt_p_p_rr_special_size_1_size_variant_1_1024_25600400() {
    // Encoding: 0x25600400
    // Test WHILELT_P.P.RR__ special value size = 1 (Size variant 1)
    // Fields: Pd=0, Rn=0, Rm=0, sf=0, size=1
    let encoding: u32 = 0x25600400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_whilelt_p_p_rr_special_size_2_size_variant_2_1024_25a00400() {
    // Encoding: 0x25A00400
    // Test WHILELT_P.P.RR__ special value size = 2 (Size variant 2)
    // Fields: sf=0, Rm=0, size=2, Rn=0, Pd=0
    let encoding: u32 = 0x25A00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_whilelt_p_p_rr_special_size_3_size_variant_3_1024_25e00400() {
    // Encoding: 0x25E00400
    // Test WHILELT_P.P.RR__ special value size = 3 (Size variant 3)
    // Fields: Pd=0, Rn=0, sf=0, size=3, Rm=0
    let encoding: u32 = 0x25E00400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilelt_p_p_rr_special_sf_0_size_variant_0_1024_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ special value sf = 0 (Size variant 0)
    // Fields: size=0, Rm=0, sf=0, Pd=0, Rn=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilelt_p_p_rr_special_sf_1_size_variant_1_1024_25201400() {
    // Encoding: 0x25201400
    // Test WHILELT_P.P.RR__ special value sf = 1 (Size variant 1)
    // Fields: Rm=0, sf=1, Rn=0, size=0, Pd=0
    let encoding: u32 = 0x25201400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_whilelt_p_p_rr_special_rn_31_stack_pointer_sp_may_require_alignment_1024_252007e0() {
    // Encoding: 0x252007E0
    // Test WHILELT_P.P.RR__ special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: size=0, sf=0, Rn=31, Pd=0, Rm=0
    let encoding: u32 = 0x252007E0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_whilelt_p_p_rr_invalid_0_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: size=0, sf=0, Rm=0, Rn=0, Pd=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_whilelt_p_p_rr_invalid_1_400_25200400() {
    // Encoding: 0x25200400
    // Test WHILELT_P.P.RR__ invalid encoding: Unconditional UNDEFINED
    // Fields: sf=0, size=0, Rm=0, Rn=0, Pd=0
    let encoding: u32 = 0x25200400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_whilelt_p_p_rr_reg_write_0_25200400() {
    // Test WHILELT_P.P.RR__ register write: SimdFromField("Pd")
    // Encoding: 0x25200400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25200400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_whilelt_p_p_rr_sp_rn_252007e0() {
    // Test WHILELT_P.P.RR__ with Rn = SP (31)
    // Encoding: 0x252007E0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x252007E0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_whilelt_p_p_rr_flags_zeroresult_0_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x0);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_whilelt_p_p_rr_flags_zeroresult_1_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_whilelt_p_p_rr_flags_negativeresult_2_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: NegativeResult
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_whilelt_p_p_rr_flags_unsignedoverflow_3_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_whilelt_p_p_rr_flags_unsignedoverflow_4_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_whilelt_p_p_rr_flags_signedoverflow_5_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_whilelt_p_p_rr_flags_signedoverflow_6_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELT_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_whilelt_p_p_rr_flags_positiveresult_7_25220420() {
    // Test WHILELT_P.P.RR__ flag computation: PositiveResult
    // Encoding: 0x25220420
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x25220420;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// NOR_P.P.PP_Z Tests
// ============================================================================

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nor_p_p_pp_z_field_pm_0_min_4200_25804200() {
    // Encoding: 0x25804200
    // Test NOR_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pg=0, Pm=0, Pd=0, Pn=0
    let encoding: u32 = 0x25804200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nor_p_p_pp_z_field_pm_1_poweroftwo_4200_25814200() {
    // Encoding: 0x25814200
    // Test NOR_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pd=0, Pm=1, Pn=0, Pg=0
    let encoding: u32 = 0x25814200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nor_p_p_pp_z_field_pg_0_min_4200_25804200() {
    // Encoding: 0x25804200
    // Test NOR_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pn=0, Pg=0, Pd=0, Pm=0
    let encoding: u32 = 0x25804200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nor_p_p_pp_z_field_pg_1_poweroftwo_4200_25804600() {
    // Encoding: 0x25804600
    // Test NOR_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pn=0, Pg=1, Pm=0, Pd=0
    let encoding: u32 = 0x25804600;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nor_p_p_pp_z_field_pn_0_min_4200_25804200() {
    // Encoding: 0x25804200
    // Test NOR_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pg=0, Pn=0, Pm=0, Pd=0
    let encoding: u32 = 0x25804200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nor_p_p_pp_z_field_pn_1_poweroftwo_4200_25804220() {
    // Encoding: 0x25804220
    // Test NOR_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=1
    let encoding: u32 = 0x25804220;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nor_p_p_pp_z_field_pd_0_min_4200_25804200() {
    // Encoding: 0x25804200
    // Test NOR_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pm=0, Pd=0, Pg=0, Pn=0
    let encoding: u32 = 0x25804200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nor_p_p_pp_z_field_pd_1_poweroftwo_4200_25804201() {
    // Encoding: 0x25804201
    // Test NOR_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25804201;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_nor_p_p_pp_z_combo_0_4200_25804200() {
    // Encoding: 0x25804200
    // Test NOR_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pm=0, Pg=0, Pn=0, Pd=0
    let encoding: u32 = 0x25804200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_nor_p_p_pp_z_invalid_0_4200_25804200() {
    // Encoding: 0x25804200
    // Test NOR_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pd=0, Pn=0, Pg=0, Pm=0
    let encoding: u32 = 0x25804200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_nor_p_p_pp_z_invalid_1_4200_25804200() {
    // Encoding: 0x25804200
    // Test NOR_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25804200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nors_p_p_pp_z_field_pm_0_min_4200_25c04200() {
    // Encoding: 0x25C04200
    // Test NORS_P.P.PP_Z field Pm = 0 (Min)
    // Fields: Pn=0, Pd=0, Pm=0, Pg=0
    let encoding: u32 = 0x25C04200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pm 16 +: 4`
/// Requirement: FieldBoundary { field: "Pm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nors_p_p_pp_z_field_pm_1_poweroftwo_4200_25c14200() {
    // Encoding: 0x25C14200
    // Test NORS_P.P.PP_Z field Pm = 1 (PowerOfTwo)
    // Fields: Pg=0, Pd=0, Pn=0, Pm=1
    let encoding: u32 = 0x25C14200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nors_p_p_pp_z_field_pg_0_min_4200_25c04200() {
    // Encoding: 0x25C04200
    // Test NORS_P.P.PP_Z field Pg = 0 (Min)
    // Fields: Pd=0, Pm=0, Pg=0, Pn=0
    let encoding: u32 = 0x25C04200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pg 10 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nors_p_p_pp_z_field_pg_1_poweroftwo_4200_25c04600() {
    // Encoding: 0x25C04600
    // Test NORS_P.P.PP_Z field Pg = 1 (PowerOfTwo)
    // Fields: Pn=0, Pm=0, Pd=0, Pg=1
    let encoding: u32 = 0x25C04600;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nors_p_p_pp_z_field_pn_0_min_4200_25c04200() {
    // Encoding: 0x25C04200
    // Test NORS_P.P.PP_Z field Pn = 0 (Min)
    // Fields: Pg=0, Pd=0, Pm=0, Pn=0
    let encoding: u32 = 0x25C04200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pn 5 +: 4`
/// Requirement: FieldBoundary { field: "Pn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nors_p_p_pp_z_field_pn_1_poweroftwo_4200_25c04220() {
    // Encoding: 0x25C04220
    // Test NORS_P.P.PP_Z field Pn = 1 (PowerOfTwo)
    // Fields: Pn=1, Pg=0, Pm=0, Pd=0
    let encoding: u32 = 0x25C04220;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_nors_p_p_pp_z_field_pd_0_min_4200_25c04200() {
    // Encoding: 0x25C04200
    // Test NORS_P.P.PP_Z field Pd = 0 (Min)
    // Fields: Pm=0, Pd=0, Pn=0, Pg=0
    let encoding: u32 = 0x25C04200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_nors_p_p_pp_z_field_pd_1_poweroftwo_4200_25c04201() {
    // Encoding: 0x25C04201
    // Test NORS_P.P.PP_Z field Pd = 1 (PowerOfTwo)
    // Fields: Pd=1, Pg=0, Pm=0, Pn=0
    let encoding: u32 = 0x25C04201;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// Pm=0 (register index 0 (first register))
#[test]
fn test_nors_p_p_pp_z_combo_0_4200_25c04200() {
    // Encoding: 0x25C04200
    // Test NORS_P.P.PP_Z field combination: Pm=0, Pg=0, Pn=0, Pd=0
    // Fields: Pg=0, Pd=0, Pn=0, Pm=0
    let encoding: u32 = 0x25C04200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_nors_p_p_pp_z_invalid_0_4200_25c04200() {
    // Encoding: 0x25C04200
    // Test NORS_P.P.PP_Z invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pn=0, Pm=0, Pd=0, Pg=0
    let encoding: u32 = 0x25C04200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_nors_p_p_pp_z_invalid_1_4200_25c04200() {
    // Encoding: 0x25C04200
    // Test NORS_P.P.PP_Z invalid encoding: Unconditional UNDEFINED
    // Fields: Pm=0, Pg=0, Pd=0, Pn=0
    let encoding: u32 = 0x25C04200;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_nor_p_p_pp_z_reg_write_0_25804200() {
    // Test NOR_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_nor_p_p_pp_z_flags_zeroresult_0_25804200() {
    // Test NOR_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_nor_p_p_pp_z_flags_zeroresult_1_25804200() {
    // Test NOR_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_nor_p_p_pp_z_flags_negativeresult_2_25804200() {
    // Test NOR_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_nor_p_p_pp_z_flags_unsignedoverflow_3_25804200() {
    // Test NOR_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_nor_p_p_pp_z_flags_unsignedoverflow_4_25804200() {
    // Test NOR_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_nor_p_p_pp_z_flags_signedoverflow_5_25804200() {
    // Test NOR_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_nor_p_p_pp_z_flags_signedoverflow_6_25804200() {
    // Test NOR_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NOR_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_nor_p_p_pp_z_flags_positiveresult_7_25804200() {
    // Test NOR_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25804200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25804200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_nors_p_p_pp_z_reg_write_0_25c04200() {
    // Test NORS_P.P.PP_Z register write: SimdFromField("Pd")
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_nors_p_p_pp_z_flags_zeroresult_0_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_nors_p_p_pp_z_flags_zeroresult_1_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: ZeroResult
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x1);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_nors_p_p_pp_z_flags_negativeresult_2_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: NegativeResult
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_nors_p_p_pp_z_flags_unsignedoverflow_3_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_nors_p_p_pp_z_flags_unsignedoverflow_4_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: UnsignedOverflow
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_nors_p_p_pp_z_flags_signedoverflow_5_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_nors_p_p_pp_z_flags_signedoverflow_6_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: SignedOverflow
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: NORS_P.P.PP_Z
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_nors_p_p_pp_z_flags_positiveresult_7_25c04200() {
    // Test NORS_P.P.PP_Z flag computation: PositiveResult
    // Encoding: 0x25C04200
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25C04200;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// PNEXT_P.P.P__ Tests
// ============================================================================

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_pnext_p_p_p_field_size_0_min_c400_2519c400() {
    // Encoding: 0x2519C400
    // Test PNEXT_P.P.P__ field size = 0 (Min)
    // Fields: Pg=0, Pdn=0, size=0
    let encoding: u32 = 0x2519C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_pnext_p_p_p_field_size_1_poweroftwo_c400_2559c400() {
    // Encoding: 0x2559C400
    // Test PNEXT_P.P.P__ field size = 1 (PowerOfTwo)
    // Fields: size=1, Pg=0, Pdn=0
    let encoding: u32 = 0x2559C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_pnext_p_p_p_field_size_2_poweroftwo_c400_2599c400() {
    // Encoding: 0x2599C400
    // Test PNEXT_P.P.P__ field size = 2 (PowerOfTwo)
    // Fields: Pdn=0, size=2, Pg=0
    let encoding: u32 = 0x2599C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_pnext_p_p_p_field_size_3_max_c400_25d9c400() {
    // Encoding: 0x25D9C400
    // Test PNEXT_P.P.P__ field size = 3 (Max)
    // Fields: size=3, Pg=0, Pdn=0
    let encoding: u32 = 0x25D9C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_pnext_p_p_p_field_pg_0_min_c400_2519c400() {
    // Encoding: 0x2519C400
    // Test PNEXT_P.P.P__ field Pg = 0 (Min)
    // Fields: size=0, Pdn=0, Pg=0
    let encoding: u32 = 0x2519C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field Pg 5 +: 4`
/// Requirement: FieldBoundary { field: "Pg", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_pnext_p_p_p_field_pg_1_poweroftwo_c400_2519c420() {
    // Encoding: 0x2519C420
    // Test PNEXT_P.P.P__ field Pg = 1 (PowerOfTwo)
    // Fields: size=0, Pg=1, Pdn=0
    let encoding: u32 = 0x2519C420;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 0, boundary: Min }
/// minimum value
#[test]
fn test_pnext_p_p_p_field_pdn_0_min_c400_2519c400() {
    // Encoding: 0x2519C400
    // Test PNEXT_P.P.P__ field Pdn = 0 (Min)
    // Fields: Pg=0, Pdn=0, size=0
    let encoding: u32 = 0x2519C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 1, boundary: PowerOfTwo }
/// value 1
#[test]
fn test_pnext_p_p_p_field_pdn_1_poweroftwo_c400_2519c401() {
    // Encoding: 0x2519C401
    // Test PNEXT_P.P.P__ field Pdn = 1 (PowerOfTwo)
    // Fields: Pdn=1, Pg=0, size=0
    let encoding: u32 = 0x2519C401;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 7, boundary: PowerOfTwoMinusOne }
/// midpoint (7)
#[test]
fn test_pnext_p_p_p_field_pdn_7_poweroftwominusone_c400_2519c407() {
    // Encoding: 0x2519C407
    // Test PNEXT_P.P.P__ field Pdn = 7 (PowerOfTwoMinusOne)
    // Fields: size=0, Pg=0, Pdn=7
    let encoding: u32 = 0x2519C407;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field Pdn 0 +: 4`
/// Requirement: FieldBoundary { field: "Pdn", value: 15, boundary: Max }
/// maximum value (15)
#[test]
fn test_pnext_p_p_p_field_pdn_15_max_c400_2519c40f() {
    // Encoding: 0x2519C40F
    // Test PNEXT_P.P.P__ field Pdn = 15 (Max)
    // Fields: Pg=0, Pdn=15, size=0
    let encoding: u32 = 0x2519C40F;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_pnext_p_p_p_combo_0_c400_2519c400() {
    // Encoding: 0x2519C400
    // Test PNEXT_P.P.P__ field combination: size=0, Pg=0, Pdn=0
    // Fields: Pg=0, Pdn=0, size=0
    let encoding: u32 = 0x2519C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_pnext_p_p_p_special_size_0_size_variant_0_50176_2519c400() {
    // Encoding: 0x2519C400
    // Test PNEXT_P.P.P__ special value size = 0 (Size variant 0)
    // Fields: Pg=0, size=0, Pdn=0
    let encoding: u32 = 0x2519C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_pnext_p_p_p_special_size_1_size_variant_1_50176_2559c400() {
    // Encoding: 0x2559C400
    // Test PNEXT_P.P.P__ special value size = 1 (Size variant 1)
    // Fields: Pg=0, Pdn=0, size=1
    let encoding: u32 = 0x2559C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_pnext_p_p_p_special_size_2_size_variant_2_50176_2599c400() {
    // Encoding: 0x2599C400
    // Test PNEXT_P.P.P__ special value size = 2 (Size variant 2)
    // Fields: size=2, Pdn=0, Pg=0
    let encoding: u32 = 0x2599C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_pnext_p_p_p_special_size_3_size_variant_3_50176_25d9c400() {
    // Encoding: 0x25D9C400
    // Test PNEXT_P.P.P__ special value size = 3 (Size variant 3)
    // Fields: size=3, Pg=0, Pdn=0
    let encoding: u32 = 0x25D9C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_pnext_p_p_p_invalid_0_c400_2519c400() {
    // Encoding: 0x2519C400
    // Test PNEXT_P.P.P__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: Pg=0, size=0, Pdn=0
    let encoding: u32 = 0x2519C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_pnext_p_p_p_invalid_1_c400_2519c400() {
    // Encoding: 0x2519C400
    // Test PNEXT_P.P.P__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pdn=0, size=0, Pg=0
    let encoding: u32 = 0x2519C400;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `SimdFromField("Pdn") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pdn")
#[test]
fn test_pnext_p_p_p_reg_write_0_2519c400() {
    // Test PNEXT_P.P.P__ register write: SimdFromField("Pdn")
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_pnext_p_p_p_flags_zeroresult_0_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_pnext_p_p_p_flags_zeroresult_1_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: ZeroResult
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_pnext_p_p_p_flags_negativeresult_2_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: NegativeResult
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_pnext_p_p_p_flags_unsignedoverflow_3_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_pnext_p_p_p_flags_unsignedoverflow_4_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: UnsignedOverflow
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x2);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_pnext_p_p_p_flags_signedoverflow_5_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_pnext_p_p_p_flags_signedoverflow_6_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: SignedOverflow
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: PNEXT_P.P.P__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_pnext_p_p_p_flags_positiveresult_7_2519c400() {
    // Test PNEXT_P.P.P__ flag computation: PositiveResult
    // Encoding: 0x2519C400
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x32);
    set_x(&mut cpu, 1, 0x64);
    let encoding: u32 = 0x2519C400;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

// ============================================================================
// WHILELO_P.P.RR__ Tests
// ============================================================================

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilelo_p_p_rr_field_size_0_min_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ field size = 0 (Min)
    // Fields: Pd=0, sf=0, size=0, Rm=0, Rn=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 1, boundary: PowerOfTwo }
/// 16-bit / halfword size
#[test]
fn test_whilelo_p_p_rr_field_size_1_poweroftwo_c00_25600c00() {
    // Encoding: 0x25600C00
    // Test WHILELO_P.P.RR__ field size = 1 (PowerOfTwo)
    // Fields: size=1, Rm=0, Rn=0, Pd=0, sf=0
    let encoding: u32 = 0x25600C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 2, boundary: PowerOfTwo }
/// 32-bit / word size
#[test]
fn test_whilelo_p_p_rr_field_size_2_poweroftwo_c00_25a00c00() {
    // Encoding: 0x25A00C00
    // Test WHILELO_P.P.RR__ field size = 2 (PowerOfTwo)
    // Fields: size=2, Rn=0, Rm=0, sf=0, Pd=0
    let encoding: u32 = 0x25A00C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size 22 +: 2`
/// Requirement: FieldBoundary { field: "size", value: 3, boundary: Max }
/// 64-bit / doubleword size
#[test]
fn test_whilelo_p_p_rr_field_size_3_max_c00_25e00c00() {
    // Encoding: 0x25E00C00
    // Test WHILELO_P.P.RR__ field size = 3 (Max)
    // Fields: Pd=0, size=3, Rm=0, sf=0, Rn=0
    let encoding: u32 = 0x25E00C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilelo_p_p_rr_field_rm_0_min_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ field Rm = 0 (Min)
    // Fields: sf=0, size=0, Rm=0, Pd=0, Rn=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilelo_p_p_rr_field_rm_1_poweroftwo_c00_25210c00() {
    // Encoding: 0x25210C00
    // Test WHILELO_P.P.RR__ field Rm = 1 (PowerOfTwo)
    // Fields: size=0, sf=0, Rm=1, Rn=0, Pd=0
    let encoding: u32 = 0x25210C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilelo_p_p_rr_field_rm_30_poweroftwominusone_c00_253e0c00() {
    // Encoding: 0x253E0C00
    // Test WHILELO_P.P.RR__ field Rm = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, Rn=0, Pd=0, Rm=30, sf=0
    let encoding: u32 = 0x253E0C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rm 16 +: 5`
/// Requirement: FieldBoundary { field: "Rm", value: 31, boundary: Max }
/// register index 31 (special)
#[test]
fn test_whilelo_p_p_rr_field_rm_31_max_c00_253f0c00() {
    // Encoding: 0x253F0C00
    // Test WHILELO_P.P.RR__ field Rm = 31 (Max)
    // Fields: Rn=0, Rm=31, size=0, Pd=0, sf=0
    let encoding: u32 = 0x253F0C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 0, boundary: Min }
/// 8-bit / byte size
#[test]
fn test_whilelo_p_p_rr_field_sf_0_min_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ field sf = 0 (Min)
    // Fields: sf=0, Rm=0, size=0, Rn=0, Pd=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field sf 12 +: 1`
/// Requirement: FieldBoundary { field: "sf", value: 1, boundary: Max }
/// 16-bit / halfword size
#[test]
fn test_whilelo_p_p_rr_field_sf_1_max_c00_25201c00() {
    // Encoding: 0x25201C00
    // Test WHILELO_P.P.RR__ field sf = 1 (Max)
    // Fields: Rm=0, sf=1, Pd=0, size=0, Rn=0
    let encoding: u32 = 0x25201C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilelo_p_p_rr_field_rn_0_min_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ field Rn = 0 (Min)
    // Fields: size=0, sf=0, Rn=0, Pd=0, Rm=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilelo_p_p_rr_field_rn_1_poweroftwo_c00_25200c20() {
    // Encoding: 0x25200C20
    // Test WHILELO_P.P.RR__ field Rn = 1 (PowerOfTwo)
    // Fields: Rn=1, size=0, sf=0, Rm=0, Pd=0
    let encoding: u32 = 0x25200C20;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 30, boundary: PowerOfTwoMinusOne }
/// register index 30 (LR in some contexts)
#[test]
fn test_whilelo_p_p_rr_field_rn_30_poweroftwominusone_c00_25200fc0() {
    // Encoding: 0x25200FC0
    // Test WHILELO_P.P.RR__ field Rn = 30 (PowerOfTwoMinusOne)
    // Fields: size=0, sf=0, Rm=0, Pd=0, Rn=30
    let encoding: u32 = 0x25200FC0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rn 5 +: 5`
/// Requirement: FieldBoundary { field: "Rn", value: 31, boundary: Max }
/// register index 31 (SP - stack pointer)
#[test]
fn test_whilelo_p_p_rr_field_rn_31_max_c00_25200fe0() {
    // Encoding: 0x25200FE0
    // Test WHILELO_P.P.RR__ field Rn = 31 (Max)
    // Fields: sf=0, Rn=31, Rm=0, size=0, Pd=0
    let encoding: u32 = 0x25200FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 0, boundary: Min }
/// register index 0 (first register)
#[test]
fn test_whilelo_p_p_rr_field_pd_0_min_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ field Pd = 0 (Min)
    // Fields: size=0, Pd=0, Rm=0, sf=0, Rn=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Pd 0 +: 4`
/// Requirement: FieldBoundary { field: "Pd", value: 1, boundary: PowerOfTwo }
/// register index 1 (second register)
#[test]
fn test_whilelo_p_p_rr_field_pd_1_poweroftwo_c00_25200c01() {
    // Encoding: 0x25200C01
    // Test WHILELO_P.P.RR__ field Pd = 1 (PowerOfTwo)
    // Fields: Rm=0, Rn=0, sf=0, Pd=1, size=0
    let encoding: u32 = 0x25200C01;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field combination 0`
/// Requirement: FieldExtraction { field: "combination", bit_start: 0, bit_width: 32 }
/// size=0 (8-bit / byte size)
#[test]
fn test_whilelo_p_p_rr_combo_0_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ field combination: size=0, Rm=0, sf=0, Rn=0, Pd=0
    // Fields: sf=0, Pd=0, Rn=0, Rm=0, size=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "size", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilelo_p_p_rr_special_size_0_size_variant_0_3072_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ special value size = 0 (Size variant 0)
    // Fields: Rm=0, Pd=0, size=0, sf=0, Rn=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "size", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilelo_p_p_rr_special_size_1_size_variant_1_3072_25600c00() {
    // Encoding: 0x25600C00
    // Test WHILELO_P.P.RR__ special value size = 1 (Size variant 1)
    // Fields: size=1, sf=0, Rn=0, Pd=0, Rm=0
    let encoding: u32 = 0x25600C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size = 2 (Size variant 2)`
/// Requirement: FieldSpecial { field: "size", value: 2, meaning: "Size variant 2" }
/// Size variant 2
#[test]
fn test_whilelo_p_p_rr_special_size_2_size_variant_2_3072_25a00c00() {
    // Encoding: 0x25A00C00
    // Test WHILELO_P.P.RR__ special value size = 2 (Size variant 2)
    // Fields: size=2, Rn=0, sf=0, Rm=0, Pd=0
    let encoding: u32 = 0x25A00C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field size = 3 (Size variant 3)`
/// Requirement: FieldSpecial { field: "size", value: 3, meaning: "Size variant 3" }
/// Size variant 3
#[test]
fn test_whilelo_p_p_rr_special_size_3_size_variant_3_3072_25e00c00() {
    // Encoding: 0x25E00C00
    // Test WHILELO_P.P.RR__ special value size = 3 (Size variant 3)
    // Fields: Pd=0, size=3, Rm=0, sf=0, Rn=0
    let encoding: u32 = 0x25E00C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field sf = 0 (Size variant 0)`
/// Requirement: FieldSpecial { field: "sf", value: 0, meaning: "Size variant 0" }
/// Size variant 0
#[test]
fn test_whilelo_p_p_rr_special_sf_0_size_variant_0_3072_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ special value sf = 0 (Size variant 0)
    // Fields: size=0, Rm=0, Rn=0, sf=0, Pd=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field sf = 1 (Size variant 1)`
/// Requirement: FieldSpecial { field: "sf", value: 1, meaning: "Size variant 1" }
/// Size variant 1
#[test]
fn test_whilelo_p_p_rr_special_sf_1_size_variant_1_3072_25201c00() {
    // Encoding: 0x25201C00
    // Test WHILELO_P.P.RR__ special value sf = 1 (Size variant 1)
    // Fields: sf=1, size=0, Pd=0, Rm=0, Rn=0
    let encoding: u32 = 0x25201C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `field Rn = 31 (Stack pointer (SP) - may require alignment)`
/// Requirement: FieldSpecial { field: "Rn", value: 31, meaning: "Stack pointer (SP) - may require alignment" }
/// Stack pointer (SP) - may require alignment
#[test]
fn test_whilelo_p_p_rr_special_rn_31_stack_pointer_sp_may_require_alignment_3072_25200fe0() {
    // Encoding: 0x25200FE0
    // Test WHILELO_P.P.RR__ special value Rn = 31 (Stack pointer (SP) - may require alignment)
    // Fields: Pd=0, size=0, Rm=0, sf=0, Rn=31
    let encoding: u32 = 0x25200FE0;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction 0x{:08X} should execute successfully", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }`
/// Requirement: UndefinedEncoding { condition: "Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: \"HaveSVE\" }, args: [] } }" }
/// triggers Undefined
#[test]
fn test_whilelo_p_p_rr_invalid_0_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ invalid encoding: Unary { op: Not, operand: Call { name: QualifiedIdentifier { qualifier: Any, name: "HaveSVE" }, args: [] } }
    // Fields: size=0, Pd=0, Rm=0, sf=0, Rn=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `Unconditional UNDEFINED`
/// Requirement: UndefinedEncoding { condition: "Unconditional UNDEFINED" }
/// triggers Undefined
#[test]
fn test_whilelo_p_p_rr_invalid_1_c00_25200c00() {
    // Encoding: 0x25200C00
    // Test WHILELO_P.P.RR__ invalid encoding: Unconditional UNDEFINED
    // Fields: Pd=0, sf=0, Rm=0, size=0, Rn=0
    let encoding: u32 = 0x25200C00;
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step();
    assert!(exit.is_err() || !matches!(exit.unwrap(), CpuExit::Continue), "expected UNDEFINED for encoding 0x{:08X}", encoding);
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `SimdFromField("Pd") write`
/// Requirement: RegisterWrite { reg_type: Gp64, dest_field: "unknown" }
/// verify register write to SimdFromField("Pd")
#[test]
fn test_whilelo_p_p_rr_reg_write_0_25200c00() {
    // Test WHILELO_P.P.RR__ register write: SimdFromField("Pd")
    // Encoding: 0x25200C00
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25200C00;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `Rn = 31 (SP)`
/// Requirement: RegisterSpecial { reg: Sp, behavior: "stack pointer with alignment requirements" }
/// stack pointer (Rn = 31)
#[test]
fn test_whilelo_p_p_rr_sp_rn_25200fe0() {
    // Test WHILELO_P.P.RR__ with Rn = SP (31)
    // Encoding: 0x25200FE0
    let mut cpu = create_test_cpu();
    let encoding: u32 = 0x25200FE0;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 0 + 0 = 0 (Z=1)
#[test]
fn test_whilelo_p_p_rr_flags_zeroresult_0_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x0);
    set_x(&mut cpu, 2, 0x0);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: ZeroResult }
/// 1 + (-1) = 0 (Z=1, C=1)
#[test]
fn test_whilelo_p_p_rr_flags_zeroresult_1_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: ZeroResult
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 1, 0x1);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: NegativeResult }
/// negative value (N=1)
#[test]
fn test_whilelo_p_p_rr_flags_negativeresult_2_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: NegativeResult
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x0);
    set_x(&mut cpu, 1, 0x8000000000000000);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 1 = 0 (C=1, Z=1)
#[test]
fn test_whilelo_p_p_rr_flags_unsignedoverflow_3_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    set_x(&mut cpu, 2, 0x1);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, true, "Z should be true");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: UnsignedOverflow }
/// max + 2 = 1 (C=1)
#[test]
fn test_whilelo_p_p_rr_flags_unsignedoverflow_4_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: UnsignedOverflow
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x2);
    set_x(&mut cpu, 1, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// max_signed + 1 = min_signed (V=1, N=1)
#[test]
fn test_whilelo_p_p_rr_flags_signedoverflow_5_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 2, 0x1);
    set_x(&mut cpu, 1, 0x7FFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, true, "N should be true");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: SignedOverflow }
/// min_signed + (-1) = max_signed (V=1)
#[test]
fn test_whilelo_p_p_rr_flags_signedoverflow_6_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: SignedOverflow
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x8000000000000000);
    set_x(&mut cpu, 2, 0xFFFFFFFFFFFFFFFF);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, true, "C should be true");
    assert_eq!(cpu.get_pstate().v, true, "V should be true");
}

/// Provenance: WHILELO_P.P.RR__
/// ASL: `if setflags then PSTATE.<N,Z,C,V> = nzcv`
/// Requirement: FlagComputation { flag: N, scenario: PositiveResult }
/// 100 + 50 = 150 (no flags)
#[test]
fn test_whilelo_p_p_rr_flags_positiveresult_7_25220c20() {
    // Test WHILELO_P.P.RR__ flag computation: PositiveResult
    // Encoding: 0x25220C20
    let mut cpu = create_test_cpu();
    set_x(&mut cpu, 1, 0x64);
    set_x(&mut cpu, 2, 0x32);
    let encoding: u32 = 0x25220C20;
    write_insn(&mut cpu, 0, encoding);
    let exit = cpu.step().unwrap();
    assert_eq!(exit, CpuExit::Continue, "instruction should execute");
    assert_eq!(cpu.get_pstate().n, false, "N should be false");
    assert_eq!(cpu.get_pstate().z, false, "Z should be false");
    assert_eq!(cpu.get_pstate().c, false, "C should be false");
    assert_eq!(cpu.get_pstate().v, false, "V should be false");
}

