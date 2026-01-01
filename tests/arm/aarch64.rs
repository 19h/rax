//! AArch64 ASL compliance tests.
//!
//! This module provides testing for AArch64 instruction decode and execution
//! against ASL specifications. Tests focus on data processing immediate instructions:
//! - ADD/SUB immediate
//! - Logical immediate (AND, ORR, EOR, ANDS)
//! - Move wide immediate (MOVN, MOVZ, MOVK)
//! - Bitfield (SBFM, BFM, UBFM and aliases like SXTB, SXTH, etc.)

use crate::arm::aarch64::cpu::AArch64Cpu;
use crate::arm::decoder::aarch64::Aarch64Decoder;
use crate::arm::decoder::{DecodedInsn, ExecutionState, Mnemonic};
use crate::arm::memory::FlatMemory;
use crate::cpu::exit::CpuExit;

// ============================================================================
// ADD/SUB Immediate Decode Tests
// ============================================================================

#[test]
fn test_decode_add_immediate_64bit() {
    // ADD X0, X1, #42
    // Encoding: sf=1, op=0, S=0, sh=0, imm12=42, rn=1, rd=0
    let raw = 0x91002A20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ADD);
    assert_eq!(decoded.execution_state, ExecutionState::Aarch64);
    assert_eq!(decoded.raw, raw);
}

#[test]
fn test_decode_sub_immediate_64bit() {
    // SUB X0, X1, #10
    // Encoding: sf=1, op=1, S=0, sh=0, imm12=10, rn=1, rd=0
    let raw = 0xD1002A20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::SUB);
    assert_eq!(decoded.execution_state, ExecutionState::Aarch64);
}

#[test]
fn test_decode_add_immediate_with_shift() {
    // ADD X0, X1, #4096 (shifted by 12)
    // Encoding: sf=1, op=0, S=0, sh=1, imm12=1, rn=1, rd=0
    let raw = 0x91400220;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ADD);
}

#[test]
fn test_execute_add_immediate_64bit() {
    // ADD X0, X1, #42
    let raw = 0x91002A20;

    let mut cpu = AArch64Cpu::new();
    let mut memory = FlatMemory::new();

    // Set X1 = 100
    cpu.set_reg(1, 100);

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    let result = cpu.execute(&decoded, &mut memory);

    assert!(matches!(result, Ok(CpuExit::Continue)));
    // X0 should be 100 + 42 = 142
    assert_eq!(cpu.get_reg(0), 142);
}

// ============================================================================
// Move Wide Immediate Decode Tests
// ============================================================================

#[test]
fn test_decode_movz_64bit() {
    // MOVZ X0, #0x1234, LSL #16
    // Encoding: sf=1, opc=2, hw=1, imm16=0x1234, rd=0
    let raw = 0xA0246800;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::MOVZ);
}

#[test]
fn test_decode_movz_32bit() {
    // MOVZ W0, #0xABCD
    // Encoding: sf=0, opc=2, hw=0, imm16=0xABCD, rd=0
    let raw = 0x5280AB80;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::MOVZ);
}

#[test]
fn test_decode_movn_64bit() {
    // MOVN X0, #0xFFFF
    // Encoding: sf=1, opc=0, hw=0, imm16=0xFFFF, rd=0
    let raw = 0x9280FF80;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::MOVN);
}

#[test]
fn test_decode_movk_64bit() {
    // MOVK X0, #0x1234, LSL #32
    // Encoding: sf=1, opc=3, hw=2, imm16=0x1234, rd=0
    let raw = 0xF2C02480;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::MOVK);
}

// ============================================================================
// PC-Relative Addressing Tests (ADR/ADRP)
// ASL Reference: arch_decode.asl PC-rel addressing, arch.tag aarch64/instrs/adr
// ============================================================================

#[test]
fn test_decode_adr_64bit() {
    // ADR X0, label
    // Encoding: op=0 (bit 31), immlo (bits 30:29), immhi (bits 23:5), Rd (bits 4:0)
    // ASL: op=0 → ADR (forms PC + sign_extend(immhi:immlo))
    // 0x10008000 = 0 00 10000 0000000000001000 00000
    //              op immlo            immhi      rd
    let raw = 0x10008000; // ADR X0, #0x1000

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ADR);
    assert_eq!(decoded.execution_state, ExecutionState::Aarch64);
}

#[test]
fn test_decode_adr_negative_offset() {
    // ADR X1, #-0x100 (negative offset)
    // immhi:immlo = -0x100 = 0x1FFF00 (21-bit two's complement)
    let raw = 0x17FFFC01; // ADR X1, #-0x100

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ADR);
}

#[test]
fn test_decode_adrp_64bit() {
    // ADRP X0, label
    // Encoding: op=1 (bit 31), immlo (bits 30:29), immhi (bits 23:5), Rd (bits 4:0)
    // ASL: op=1 → ADRP (forms (PC & ~0xFFF) + sign_extend(immhi:immlo) << 12)
    // 0x90008000 = 1 00 10000 0000000000001000 00000
    let raw = 0x90008000; // ADRP X0, #0x1000000

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ADRP);
    assert_eq!(decoded.execution_state, ExecutionState::Aarch64);
}

#[test]
fn test_decode_adrp_different_registers() {
    // ADRP X15, label
    let raw = 0x9000800F; // ADRP X15, #0x1000000

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ADRP);
}

// ============================================================================
// Logical Immediate Decode Tests
// ============================================================================

#[test]
fn test_decode_and_immediate() {
    // AND X0, X1, #0xFFFF0000FFFF0000
    // Encoding: sf=1, opc=0, N=1, immr=0, imms=15, Rn=1, Rd=0
    let raw = 0x92000C20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::AND);
}

#[test]
fn test_decode_orr_immediate() {
    // ORR X0, X1, #0xFF00FF00FF00FF00
    // Encoding: sf=1, opc=1, N=1, immr=8, imms=23, Rn=1, Rd=0
    let raw = 0xB218CC20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ORR);
}

#[test]
fn test_decode_eor_immediate() {
    // EOR X0, X1, #0xAAAAAAAAAAAAAAAA
    // Encoding: sf=1, opc=2, N=1, immr=1, imms=30, Rn=1, Rd=0
    let raw = 0xCA01FC20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::EOR);
}

#[test]
fn test_decode_ands_immediate() {
    // ANDS X0, X1, #0x5555555555555555
    // Encoding: sf=1, opc=3, N=1, immr=1, imms=30, Rn=1, Rd=0
    let raw = 0xEA01FC20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ANDS);
}

// ============================================================================
// Bitfield Decode Tests
// ============================================================================

#[test]
fn test_decode_sbfm_sxtb() {
    // SXTB X0, W1
    // Encoding: sf=1, opc=0, N=0, immr=0, imms=7, Rn=1, Rd=0
    let raw = 0x93401C20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::SXTB);
}

#[test]
fn test_decode_ubfm_lsl() {
    // LSL X0, X1, #1
    // Encoding: sf=1, opc=2, N=1, immr=63, imms=62, Rn=1, Rd=0
    let raw = 0xD37FFC20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::LSL);
}

#[test]
fn test_decode_bfm_bfi() {
    // BFI X0, X1, #16, #8
    // Encoding: sf=1, opc=1, N=1, immr=48, imms=55, Rn=1, Rd=0
    let raw = 0xB2C86C20;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::BFI);
}

// ============================================================================
// Branch Decode Tests
// ============================================================================

#[test]
fn test_decode_b_unconditional() {
    // B #0x1000 (forward branch)
    // Encoding: op=0, imm26=0x1000
    let raw = 0x14004000;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::B);
}

#[test]
fn test_decode_bl_link() {
    // BL #0x800 (subroutine call)
    // Encoding: op=1, imm26=0x800
    let raw = 0x94002000;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::BL);
}

#[test]
fn test_decode_cbz_compare_zero() {
    // CBZ X0, #0x100
    // Encoding: sf=1, op=0, imm19=0x100, Rt=0
    let raw = 0xB4008000;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::CBZ);
}

#[test]
fn test_decode_cbnz_compare_nonzero() {
    // CBNZ X0, #0x100
    // Encoding: sf=1, op=1, imm19=0x100, Rt=0
    let raw = 0xB5008000;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::CBNZ);
}

#[test]
fn test_decode_tbz_test_bit_zero() {
    // TBZ X0, #10, #0x100
    // Encoding: b5=0, op=0, b40=10, imm14=0x100, Rt=0
    let raw = 0x36428280;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::TBZ);
}

#[test]
fn test_decode_tbnz_test_bit_nonzero() {
    // TBNZ X0, #15, #0x100
    // Encoding: b5=0, op=1, b40=15, imm14=0x100, Rt=0
    let raw = 0x3742C280;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::TBNZ);
}

// ============================================================================
// Load/Store Decode Tests
// ============================================================================

#[test]
fn test_decode_ldr_unsigned_offset() {
    // LDR X0, [X1, #8]
    // Encoding: size=3, opc=1, imm12=1, Rn=1, Rt=0
    let raw = 0xF8410020;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::LDR);
}

#[test]
fn test_decode_str_unsigned_offset() {
    // STR X0, [X1, #16]
    // Encoding: size=3, opc=0, imm12=2, Rn=1, Rt=0
    let raw = 0xF8010040;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::STR);
}

#[test]
fn test_decode_ldr_byte() {
    // LDRB W0, [X1, #4]
    // Encoding: size=0, opc=1, imm12=4, Rn=1, Rt=0
    let raw = 0x39401020;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::LDRB);
}

#[test]
fn test_decode_str_halfword() {
    // STRH W0, [X1, #2]
    // Encoding: size=1, opc=0, imm12=1, Rn=1, Rt=0
    let raw = 0x79000420;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::STRH);
}

// ============================================================================
// Data Processing Register Decode Tests
// ============================================================================

#[test]
fn test_decode_add_shifted_register() {
    // ADD X0, X1, X2, LSL #2
    // Encoding: sf=1, op=0, S=0, shift=0, Rm=2, imm6=2, Rn=1, Rd=0
    let raw = 0x8B020820;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ADD);
}

#[test]
fn test_decode_sub_shifted_register() {
    // SUB X0, X1, X2, ASR #1
    // Encoding: sf=1, op=1, S=0, shift=2, Rm=2, imm6=1, Rn=1, Rd=0
    let raw = 0xCB410820;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::SUB);
}

#[test]
fn test_decode_and_shifted_register() {
    // AND X0, X1, X2, LSR #3
    // Encoding: sf=1, opc=0, shift=1, N=0, Rm=2, imm6=3, Rn=1, Rd=0
    let raw = 0x8A410820;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::AND);
}

#[test]
fn test_decode_orr_shifted_register() {
    // ORR X0, X1, X2, ROR #4
    // Encoding: sf=1, opc=1, shift=3, N=0, Rm=2, imm6=4, Rn=1, Rd=0
    let raw = 0xAA810820;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    assert_eq!(decoded.mnemonic, Mnemonic::ORR);
}

// ============================================================================
// ASL Compliance Framework
// ============================================================================

/// Framework for testing ASL compliance.
///
/// This framework allows testing decode against ASL specifications.
/// For each instruction category, we:
/// 1. Decode the binary encoding
/// 2. Verify the decoded instruction matches expected mnemonic and operands
/// 3. TODO: Execute the instruction and verify against ASL semantics
struct AslComplianceTest;

impl AslComplianceTest {
    /// Test data processing immediate instructions from ASL.
    fn test_dp_immediate_decode() {
        // Test cases based on ASL specifications
        // - ADD/SUB immediate with various shifts and flags
        // - Logical immediate with bitmask encoding
        // - Move wide with LSL shifts
        // - Bitfield operations
    }

    /// Verify instruction decoding matches ASL expectations.
    fn verify_decode(raw: u32, expected_mnemonic: Mnemonic) {
        let decoded = Aarch64Decoder::decode(raw).unwrap();
        assert_eq!(decoded.mnemonic, expected_mnemonic);
    }
}

// ============================================================================
// SVE Instruction Tests
// ============================================================================

#[test]
fn test_sve_decode_arithmetic() {
    // Example SVE ADD vector (placeholder - SVE not implemented)
    // ADD Z0.B, Z1.B, Z2.B
    // This should decode to UNKNOWN since SVE is not implemented
    let raw = 0x04202000; // Hypothetical SVE encoding

    let decoded = Aarch64Decoder::decode(raw);
    // Should return error or UNKNOWN mnemonic
    assert!(decoded.is_err() || decoded.unwrap().mnemonic == Mnemonic::UNKNOWN);
}

#[test]
fn test_sve_decode_predicate() {
    // Example SVE predicate operation
    // PTRUE P0.B
    let raw = 0x2518E3E0; // Hypothetical SVE predicate encoding

    let decoded = Aarch64Decoder::decode(raw);
    assert!(decoded.is_err() || decoded.unwrap().mnemonic == Mnemonic::UNKNOWN);
}

#[test]
fn test_sve_decode_load_store() {
    // Example SVE load
    // LD1B Z0.B, P0/Z, [X0, X1]
    let raw = 0xA4000000; // Hypothetical SVE load encoding

    let decoded = Aarch64Decoder::decode(raw);
    assert!(decoded.is_err() || decoded.unwrap().mnemonic == Mnemonic::UNKNOWN);
}

#[test]
fn test_sve_decode_scalar_fp() {
    // Example SVE scalar FP (but should be handled by regular FP if implemented)
    // FADD D0, D1, D2 (this is regular FP, not SVE)
    let raw = 0x1E622820;

    let decoded = Aarch64Decoder::decode(raw).unwrap();
    // This should decode properly as it's scalar FP, not SVE
    assert_eq!(decoded.mnemonic, Mnemonic::FADD);
}

#[test]
fn test_asl_compliance_framework() {
    // Basic framework test - decode verification
    AslComplianceTest::verify_decode(0x91002A20, Mnemonic::ADD);
    AslComplianceTest::verify_decode(0xD1002A20, Mnemonic::SUB);
    AslComplianceTest::verify_decode(0xA0246800, Mnemonic::MOVZ);
}
