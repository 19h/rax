//! tests::data tests

use super::*;
use crate::isa::arm::aarch64::cpu::*;

#[test]
fn casp32_effective_addresses_wrap() {
    let mut cpu = create_wrapping_memory_cpu();
    let base = u64::MAX - 3;
    let lo = cpu.mem_read_u32(base).unwrap();
    let hi = cpu.mem_read_u32(base.wrapping_add(4)).unwrap();

    cpu.set_x(10, base);
    cpu.set_x(0, lo as u64);
    cpu.set_x(1, hi as u64);
    cpu.set_x(2, 0xAABB_CCDD);
    cpu.set_x(3, 0x1122_3344);

    assert_eq!(
        cpu.exec_ldst_exclusive(encode_casp(0, 10, 0, 2)).unwrap(),
        CpuExit::Continue
    );
    assert_eq!(cpu.get_w(0), lo);
    assert_eq!(cpu.get_w(1), hi);
    assert_eq!(cpu.mem_read_u32(base).unwrap(), 0xAABB_CCDD);
    assert_eq!(cpu.mem_read_u32(base.wrapping_add(4)).unwrap(), 0x1122_3344);
}
#[test]
fn casp64_unaligned_wrap_address_faults() {
    let mut cpu = create_wrapping_memory_cpu();
    let base = u64::MAX - 7;
    let lo = cpu.mem_read_u64(base).unwrap();
    let hi = cpu.mem_read_u64(base.wrapping_add(8)).unwrap();
    let new_lo = 0xAABB_CCDD_EEFF_0011;
    let new_hi = 0x2233_4455_6677_8899;

    cpu.set_x(10, base);
    cpu.set_x(4, lo);
    cpu.set_x(5, hi);
    cpu.set_x(6, new_lo);
    cpu.set_x(7, new_hi);

    match cpu.exec_ldst_exclusive(encode_casp(1, 10, 4, 6)) {
        Err(ArmError::MemoryError(info)) => {
            assert_eq!(info.address, base);
            assert_eq!(info.fault_type, MemoryFaultType::Alignment);
        }
        other => panic!("expected CASP64 alignment fault, got {other:?}"),
    }
    assert_eq!(cpu.get_x(4), lo);
    assert_eq!(cpu.get_x(5), hi);
    assert_eq!(cpu.mem_read_u64(base).unwrap(), lo);
    assert_eq!(cpu.mem_read_u64(base.wrapping_add(8)).unwrap(), hi);
}
#[test]
fn sve_multiply_immediate_uses_signed_imm8() {
    let initial = 0x0f0e_0d0c_0b0a_0908_0706_0504_0302_0100u128;
    let cases = [
        // MUL Z0.B, Z0.B, #0
        (0x2530_c000, 0),
        // MUL Z0.B, Z0.B, #1
        (0x2530_c020, initial),
        // MUL Z0.B, Z0.B, #-1
        (0x2530_dfe0, 0xf1f2_f3f4_f5f6_f7f8_f9fa_fbfc_fdfe_ff00u128),
        // MUL Z0.H, Z0.H, #-1
        (0x2570_dfe0, 0xf0f2_f2f4_f4f6_f6f8_f8fa_fafc_fcfe_ff00u128),
    ];

    for (insn, expected) in cases {
        let mut cpu = create_cpu_with_insn(insn);
        cpu.sysregs.el1.cpacr |= 0b11 << 16; // ZEN
        cpu.set_simd(0, initial);

        assert_eq!(cpu.step().unwrap(), CpuExit::Continue, "{insn:#010x}");
        assert_eq!(cpu.get_simd(0), expected, "{insn:#010x}");
    }
}
#[test]
fn simd_f64_fma_zero_addend_sets_underflow_for_tiny_product() {
    let mut cpu = create_cpu_with_insn(0x5fc0_101f); // FMLA D31, D0, V0.D[0]
    cpu.sysregs.el1.cpacr |= 0b11 << 20; // FPEN
    cpu.v[0] = 0x0000_0000_0000_0001; // smallest positive f64 subnormal
    cpu.v[31] = 0;
    assert_eq!(cpu.step().unwrap(), CpuExit::Continue);
    assert_eq!(cpu.fpsr & (FPSR_UFC | FPSR_IXC), FPSR_UFC | FPSR_IXC);
}
// -------------------------------------------------------------------------
// Data Processing Immediate - Add/Subtract
// -------------------------------------------------------------------------

#[test]
fn test_add_imm_64() {
    // ADD X0, X1, #0x123
    // sf=1, op=0, S=0, shift=0, imm12=0x123, Rn=1, Rd=0
    // [1 0 0 10001 00 imm12 Rn Rd]
    let insn = 0x91048C20; // ADD X0, X1, #0x123
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x1000);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x1123);
}
#[test]
fn test_add_imm_32() {
    // ADD W0, W1, #0x50
    // sf=0
    let insn = 0x11014020; // ADD W0, W1, #0x50
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF_FFFF_0000_0100);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x150); // 32-bit result, zero-extended
}
#[test]
fn test_adds_imm_sets_flags() {
    // ADDS X0, X1, #1 (result will be 0, sets Z flag)
    let insn = 0xB1000420; // ADDS X0, X1, #1
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0);
    assert!(cpu.get_z()); // Zero flag
    assert!(cpu.get_c()); // Carry flag (overflow from addition)
}
#[test]
fn test_sub_imm() {
    // SUB X0, X1, #0x100
    let insn = 0xD1040020; // SUB X0, X1, #0x100
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x500);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x400);
}
#[test]
fn test_subs_imm_negative() {
    // SUBS X0, X1, #0x100 (result negative)
    let insn = 0xF1040020; // SUBS X0, X1, #0x100
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x50);
    cpu.step().unwrap();
    assert!(cpu.get_n()); // Negative
    assert!(!cpu.get_c()); // No borrow = C clear
}
#[test]
fn test_add_imm_shifted() {
    // ADD X0, X1, #0x1, LSL #12
    // shift=1 means LSL #12
    let insn = 0x91400420; // ADD X0, X1, #1, LSL #12
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x1000);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x2000);
}
// -------------------------------------------------------------------------
// Data Processing Immediate - Logical
// -------------------------------------------------------------------------

#[test]
fn test_and_imm() {
    // AND X0, X1, #0xFF (bitmask for low 8 bits)
    // For AND imm, the immediate is encoded as bitmask
    // N=1, immr=0, imms=7 gives 0xFF mask for 64-bit
    let insn = 0x92401C20; // AND X0, X1, #0xFF
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x1234_5678);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x78);
}
#[test]
fn test_orr_imm() {
    // ORR X0, X1, #0x1
    // N=1, immr=0, imms=0 -> single bit pattern
    // sf=1, opc=01, 100100, N=1, immr=000000, imms=000000, Rn=1, Rd=0
    // = 1 01 100100 1 000000 000000 00001 00000
    // = 0xB2400020
    let insn = 0xB2400020; // ORR X0, X1, #0x1
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x1234_5678);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x1234_5679); // 0x1234_5678 | 0x1 = 0x1234_5679
}
#[test]
fn test_eor_imm() {
    // EOR X0, X1, #1
    let insn = 0xD2400020; // EOR X0, X1, #1
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xAAAA);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xAAAB);
}
#[test]
fn test_ands_imm() {
    // ANDS X0, X1, #0xFF (sets flags)
    let insn = 0xF2401C20; // ANDS X0, X1, #0xFF
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0);
    assert!(cpu.get_z()); // Zero flag set
}
// -------------------------------------------------------------------------
// Data Processing Immediate - Move Wide
// -------------------------------------------------------------------------

#[test]
fn test_movz() {
    // MOVZ X0, #0x1234
    let insn = 0xD2824680; // MOVZ X0, #0x1234
    let mut cpu = create_cpu_with_insn(insn);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x1234);
}
#[test]
fn test_movz_shifted() {
    // MOVZ X0, #0xABCD, LSL #16 (hw=01)
    // Encoding: 1 10 100101 01 imm16 Rd = 0xD2B579A0
    let insn = 0xD2B579A0; // MOVZ X0, #0xABCD, LSL #16
    let mut cpu = create_cpu_with_insn(insn);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xABCD_0000);
}
#[test]
fn test_movn() {
    // MOVN X0, #0 (result is ~0 = 0xFFFF_FFFF_FFFF_FFFF)
    let insn = 0x92800000; // MOVN X0, #0
    let mut cpu = create_cpu_with_insn(insn);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_FFFF);
}
#[test]
fn test_movk() {
    // MOVK X0, #0x5678, LSL #16 (keep other bits)
    let insn = 0xF2AACF00; // MOVK X0, #0x5678, LSL #16
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(0, 0x0000_0000_0000_1234);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x0000_0000_5678_1234);
}
// -------------------------------------------------------------------------
// Data Processing Immediate - Bitfield
// -------------------------------------------------------------------------

#[test]
fn test_ubfm_lsr() {
    // UBFM can implement LSR: LSR X0, X1, #4 = UBFM X0, X1, #4, #63
    let insn = 0xD344FC20; // UBFM X0, X1, #4, #63 (LSR #4)
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xF0);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x0F);
}
#[test]
fn test_ubfm_lsr_zero_fills_rotated_high_bits() {
    // UBFM X0, X1, #5, #63 is the LSR #5 alias.
    let insn = 0xD345FC20;
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x1234_5678_9ABC_DEF0);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x0091_A2B3_C4D5_E6F7);
}
#[test]
fn test_ubfm_lsl_discards_rotated_out_high_bit() {
    // UBFM X0, X1, #63, #62 is the LSL #1 alias.
    let insn = 0xD37EFC20;
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x8000_0000_0000_0001);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x2);
}
#[test]
fn test_ubfm_uxtb() {
    // UXTB W0, W1 = UBFM W0, W1, #0, #7
    let insn = 0x53001C20; // UBFM W0, W1, #0, #7
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF_1234);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x34);
}
#[test]
fn test_sbfm_asr() {
    // SBFM can implement ASR: ASR X0, X1, #4 = SBFM X0, X1, #4, #63
    let insn = 0x9344FC20; // SBFM X0, X1, #4, #63 (ASR #4)
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x8000_0000_0000_00F0u64);
    cpu.step().unwrap();
    // Sign-extended shift right
    assert_eq!(cpu.get_x(0), 0xF800_0000_0000_000F);
}
#[test]
fn test_sbfm_sxtb() {
    // SXTB W0, W1 = SBFM W0, W1, #0, #7
    let insn = 0x13001C20; // SBFM W0, W1, #0, #7
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x80); // Negative byte
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFFFF_FF80); // Sign-extended to 32-bit
}
#[test]
fn test_bfm() {
    // BFM X0, X1, #4, #7 - insert bits
    let insn = 0xB344_1C20; // BFM X0, X1, #4, #7
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(0, 0xFFFF_FFFF_FFFF_0000);
    cpu.set_x(1, 0x00AB);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_000A);
}
#[test]
fn test_bfm_bfi_inserts_without_clearing_lower_bits() {
    // BFI W0, W1, #8, #8 inserts the low byte of W1 into bits 15:8.
    let insn = 0x3318_1C20;
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_w(0, 0xA5A5_1234);
    cpu.set_w(1, 0x0000_00CC);
    cpu.step().unwrap();
    assert_eq!(cpu.get_w(0), 0xA5A5_CC34);
}
#[test]
fn test_bfm_self_bfi_replicates_byte() {
    let mut cpu = create_test_cpu();
    write_insn(&mut cpu, 0, 0x3318_1C00); // BFI W0, W0, #8, #8
    write_insn(&mut cpu, 4, 0x3310_1C00); // BFI W0, W0, #16, #8
    write_insn(&mut cpu, 8, 0x3308_1C00); // BFI W0, W0, #24, #8
    cpu.set_w(0, 0x81);
    cpu.step().unwrap();
    cpu.step().unwrap();
    cpu.step().unwrap();
    assert_eq!(cpu.get_w(0), 0x8181_8181);
}
// -------------------------------------------------------------------------
// Data Processing Immediate - Extract
// -------------------------------------------------------------------------

#[test]
fn test_extr() {
    // EXTR X0, X1, X2, #8 - extract bits from concatenation
    // result = (X1 << (64-8)) | (X2 >> 8)
    let insn = 0x93C22020; // EXTR X0, X1, X2, #8
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x0000_0000_0000_00FF);
    cpu.set_x(2, 0xFF00_0000_0000_0000);
    cpu.step().unwrap();
    // (0xFF << 56) | (0xFF00... >> 8) = 0xFF00... | 0x00FF... = 0xFFFF...
    assert_eq!(cpu.get_x(0), 0xFFFF_0000_0000_0000);
}
#[test]
fn test_ror_via_extr() {
    // ROR X0, X1, #4 = EXTR X0, X1, X1, #4
    let insn = 0x93C11020; // EXTR X0, X1, X1, #4
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xF);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xF000_0000_0000_0000);
}
// -------------------------------------------------------------------------
// Data Processing Register - Logical Shifted Register
// -------------------------------------------------------------------------

#[test]
fn test_and_shifted() {
    // AND X0, X1, X2
    let insn = 0x8A020020; // AND X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFF00_FF00);
    cpu.set_x(2, 0x0FF0_0FF0);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x0F00_0F00);
}
#[test]
fn test_and_lsl() {
    // AND X0, X1, X2, LSL #4
    let insn = 0x8A021020; // AND X0, X1, X2, LSL #4
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF);
    cpu.set_x(2, 0x00FF);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x0FF0);
}
#[test]
fn test_orr_reg() {
    // ORR X0, X1, X2
    let insn = 0xAA020020; // ORR X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xF0F0);
    cpu.set_x(2, 0x0F0F);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFFFF);
}
#[test]
fn test_eor_reg() {
    // EOR X0, X1, X2
    let insn = 0xCA020020; // EOR X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF);
    cpu.set_x(2, 0x0F0F);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xF0F0);
}
#[test]
fn test_bic() {
    // BIC X0, X1, X2 (bit clear: X1 AND NOT X2)
    let insn = 0x8A220020; // BIC X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF);
    cpu.set_x(2, 0x00FF);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFF00);
}
#[test]
fn test_ands_reg() {
    // ANDS X0, X1, X2 (sets flags)
    let insn = 0xEA020020; // ANDS X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x1000);
    cpu.set_x(2, 0x0001);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0);
    assert!(cpu.get_z()); // Result is zero
}
#[test]
fn test_mov_reg() {
    // MOV X0, X1 (alias for ORR X0, XZR, X1)
    let insn = 0xAA0103E0; // MOV X0, X1
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xDEAD_BEEF);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xDEAD_BEEF);
}
// -------------------------------------------------------------------------
// Data Processing Register - Add/Subtract Shifted/Extended
// -------------------------------------------------------------------------

#[test]
fn test_add_shifted() {
    // ADD X0, X1, X2
    let insn = 0x8B020020; // ADD X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 100);
    cpu.set_x(2, 200);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 300);
}
#[test]
fn test_add_lsl() {
    // ADD X0, X1, X2, LSL #2
    let insn = 0x8B020820; // ADD X0, X1, X2, LSL #2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 100);
    cpu.set_x(2, 25);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 200);
}
#[test]
fn test_sub_shifted() {
    // SUB X0, X1, X2
    let insn = 0xCB020020; // SUB X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 500);
    cpu.set_x(2, 200);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 300);
}
#[test]
fn test_adds_shifted() {
    // ADDS X0, X1, X2 (sets flags)
    let insn = 0xAB020020; // ADDS X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
    cpu.set_x(2, 1);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0);
    assert!(cpu.get_z()); // Zero
    assert!(cpu.get_c()); // Carry
}
#[test]
fn test_subs_shifted() {
    // SUBS X0, X1, X2 (CMP alias when Rd=XZR)
    let insn = 0xEB020020; // SUBS X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 100);
    cpu.set_x(2, 100);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0);
    assert!(cpu.get_z());
    assert!(cpu.get_c()); // No borrow = C set
}
#[test]
fn test_cmp() {
    // CMP X1, X2 (SUBS XZR, X1, X2)
    let insn = 0xEB02003F; // CMP X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 50);
    cpu.set_x(2, 100);
    cpu.step().unwrap();
    assert!(cpu.get_n()); // Negative
    assert!(!cpu.get_c()); // Borrow = C clear
}
#[test]
fn test_neg() {
    // NEG X0, X1 (SUB X0, XZR, X1)
    let insn = 0xCB0103E0; // NEG X0, X1
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 1);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_FFFF);
}
#[test]
fn test_add_extended() {
    // ADD X0, X1, W2, UXTW (zero-extend W2 to 64-bit)
    let insn = 0x8B224020; // ADD X0, X1, W2, UXTW
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x1000_0000_0000_0000);
    cpu.set_x(2, 0xFFFF_FFFF_0000_0100);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x1000_0000_0000_0100);
}
#[test]
fn test_add_extended_sxtw() {
    // ADD X0, X1, W2, SXTW (sign-extend W2 to 64-bit)
    let insn = 0x8B22C020; // ADD X0, X1, W2, SXTW
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0);
    cpu.set_x(2, 0x8000_0000); // Negative when sign-extended
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_8000_0000);
}
// -------------------------------------------------------------------------
// Data Processing Register - ADC/SBC
// -------------------------------------------------------------------------

#[test]
fn test_adc() {
    // ADC X0, X1, X2 (add with carry)
    let insn = 0x9A020020; // ADC X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 100);
    cpu.set_x(2, 200);
    cpu.set_c(true);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 301); // 100 + 200 + 1
}
#[test]
fn test_adc_no_carry() {
    // ADC X0, X1, X2 (no carry in)
    let insn = 0x9A020020; // ADC X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 100);
    cpu.set_x(2, 200);
    cpu.set_c(false);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 300);
}
#[test]
fn test_adcs() {
    // ADCS X0, X1, X2 (sets flags)
    let insn = 0xBA020020; // ADCS X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
    cpu.set_x(2, 0);
    cpu.set_c(true);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0);
    assert!(cpu.get_z());
    assert!(cpu.get_c()); // Overflow
}
#[test]
fn test_sbc() {
    // SBC X0, X1, X2 (subtract with carry/borrow)
    let insn = 0xDA020020; // SBC X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 500);
    cpu.set_x(2, 200);
    cpu.set_c(true); // No borrow
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 300);
}
#[test]
fn test_sbc_borrow() {
    // SBC X0, X1, X2 (with borrow)
    let insn = 0xDA020020; // SBC X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 500);
    cpu.set_x(2, 200);
    cpu.set_c(false); // Borrow
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 299);
}
#[test]
fn test_sbcs() {
    // SBCS X0, X1, X2 (sets flags)
    let insn = 0xFA020020; // SBCS X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 100);
    cpu.set_x(2, 100);
    cpu.set_c(true);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0);
    assert!(cpu.get_z());
}
#[test]
fn test_lslv() {
    // LSLV X0, X1, X2 (logical shift left variable)
    let insn = 0x9AC22020; // LSLV X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFF);
    cpu.set_x(2, 4);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFF0);
}
#[test]
fn test_lsrv() {
    // LSRV X0, X1, X2 (logical shift right variable)
    let insn = 0x9AC22420; // LSRV X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xFF0);
    cpu.set_x(2, 4);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xFF);
}
#[test]
fn test_asrv() {
    // ASRV X0, X1, X2 (arithmetic shift right variable)
    let insn = 0x9AC22820; // ASRV X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0x8000_0000_0000_0000);
    cpu.set_x(2, 4);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xF800_0000_0000_0000);
}
#[test]
fn test_rorv() {
    // RORV X0, X1, X2 (rotate right variable)
    let insn = 0x9AC22C20; // RORV X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 0xF);
    cpu.set_x(2, 4);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0xF000_0000_0000_0000);
}
// -------------------------------------------------------------------------
// Data Processing Register - 3-source
// -------------------------------------------------------------------------

#[test]
fn test_madd() {
    // MADD X0, X1, X2, X3 (X0 = X1*X2 + X3)
    let insn = 0x9B020C20; // MADD X0, X1, X2, X3
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 10);
    cpu.set_x(2, 20);
    cpu.set_x(3, 5);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 205);
}
#[test]
fn test_mul() {
    // MUL X0, X1, X2 (MADD X0, X1, X2, XZR)
    let insn = 0x9B027C20; // MUL X0, X1, X2
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 100);
    cpu.set_x(2, 200);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 20000);
}
#[test]
fn test_msub() {
    // MSUB X0, X1, X2, X3 (X0 = X3 - X1*X2)
    let insn = 0x9B028C20; // MSUB X0, X1, X2, X3
    let mut cpu = create_cpu_with_insn(insn);
    cpu.set_x(1, 10);
    cpu.set_x(2, 20);
    cpu.set_x(3, 500);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 300);
}
#[test]
fn issue_187_pstate_uao_and_pan_require_advertised_features() {
    let mut cpu = create_test_cpu();

    let msr_uao_1 = msr_imm_pstate(0, 0b011, 1);
    assert!(matches!(
        cpu.exec_system(msr_uao_1),
        Err(ArmError::UndefinedInstruction(insn)) if insn == msr_uao_1
    ));
    assert!(!cpu.uao);

    cpu.sysregs.id_aa64mmfr2_el1 |= 1 << 4;
    assert_eq!(cpu.exec_system(msr_uao_1).unwrap(), CpuExit::Continue);
    assert!(cpu.uao);

    let msr_pan_1 = msr_imm_pstate(0, 0b100, 1);
    assert!(matches!(
        cpu.exec_system(msr_pan_1),
        Err(ArmError::UndefinedInstruction(insn)) if insn == msr_pan_1
    ));
    assert!(!cpu.pan);

    cpu.sysregs.id_aa64mmfr1_el1 |= 1 << 20;
    assert_eq!(cpu.exec_system(msr_pan_1).unwrap(), CpuExit::Continue);
    assert!(cpu.pan);
}
