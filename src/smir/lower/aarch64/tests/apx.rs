//! tests::apx tests

use super::*;
use crate::smir::lower::aarch64::*;

#[test]
fn lowers_cmp_test_apx_egpr_operands_runtime() {
    let cmp_code = lower_single_op(OpKind::Cmp {
        src1: x86(X86Reg::R16),
        src2: SrcOperand::Reg(x86(X86Reg::R17)),
        width: OpWidth::W64,
    });
    let cmp_regs = [(16, 5), (17, 7), (18, 0x1818_1818_1818_1818)];
    let (out, out_nzcv, sp) = run_aarch64_code(&cmp_code, &cmp_regs, 0b1010);
    assert_eq!(out_nzcv, expected_addsub_nzcv(5, 7, true, OpWidth::W64));
    assert_eq!(out[16], 5);
    assert_eq!(out[17], 7);
    assert_eq!(out[18], 0x1818_1818_1818_1818);
    assert_eq!(sp, 0x8000);

    let test_code = lower_single_op(OpKind::Test {
        src1: x86(X86Reg::R19),
        src2: SrcOperand::Reg(x86(X86Reg::R20)),
        width: OpWidth::W64,
    });
    let test_regs = [
        (19, 0xf0f0_f0f0_f0f0_f0f0),
        (20, 0x0f0f_0f0f_0f0f_0f0f),
        (21, 0x2121_2121_2121_2121),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp) = run_aarch64_code(&test_code, &test_regs, old_nzcv);
    let test_result = ref_logic(
        0xf0f0_f0f0_f0f0_f0f0,
        0x0f0f_0f0f_0f0f_0f0f,
        0b00,
        false,
        OpWidth::W64,
    );
    assert_eq!(
        out_nzcv,
        expected_logic_source_nzcv(old_nzcv, test_result, OpWidth::W64, FlagUpdate::All)
    );
    assert_eq!(out[19], 0xf0f0_f0f0_f0f0_f0f0);
    assert_eq!(out[20], 0x0f0f_0f0f_0f0f_0f0f);
    assert_eq!(out[21], 0x2121_2121_2121_2121);
    assert_eq!(sp, 0x8000);

    let subword_test_code = lower_single_op(OpKind::Test {
        src1: x86(X86Reg::R22),
        src2: SrcOperand::Imm(0x80),
        width: OpWidth::W8,
    });
    let subword_regs = [(22, 0xff), (23, 0x2323_2323_2323_2323)];
    let old_nzcv = 0b0011;
    let (out, out_nzcv, sp) = run_aarch64_code(&subword_test_code, &subword_regs, old_nzcv);
    let subword_result = ref_logic(0xff, 0x80, 0b00, false, OpWidth::W8);
    assert_eq!(
        out_nzcv,
        expected_logic_source_nzcv(old_nzcv, subword_result, OpWidth::W8, FlagUpdate::All,)
    );
    assert_eq!(out[22], 0xff);
    assert_eq!(out[23], 0x2323_2323_2323_2323);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_cmp_test_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Cmp {
            src1: x86(X86Reg::R31),
            src2: SrcOperand::Reg(x86(X86Reg::R16)),
            width: OpWidth::W64,
        },
        OpKind::Cmp {
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
        },
        OpKind::Test {
            src1: x86(X86Reg::R31),
            src2: SrcOperand::Imm(0xff),
            width: OpWidth::W32,
        },
        OpKind::Test {
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W8,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
// Regression for issue #61: x86 R30 maps to host X30 — the link register the
// region's `RET` branches through — so it must be rejected for identity-mapped
// operands exactly like R31 (which aliases SP/XZR). Using it as a destination
// overwrites the native return address (a guest-to-host CFI break); as a source
// it leaks a host code pointer. R29 (host X29, guest-state-backed) must still
// lower.
#[test]
fn rejects_apx_r30_identity_mapping_x30_is_link_register() {
    for kind in [
        OpKind::Cmp {
            src1: x86(X86Reg::R30),
            src2: SrcOperand::Reg(x86(X86Reg::R16)),
            width: OpWidth::W64,
        },
        OpKind::Cmp {
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R30)),
            width: OpWidth::W64,
        },
        OpKind::Mov {
            dst: x86(X86Reg::R30),
            src: SrcOperand::Reg(x86(X86Reg::R16)),
            width: OpWidth::W64,
        },
        OpKind::Test {
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R30)),
            width: OpWidth::W8,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(
            matches!(err, LowerError::InvalidRegister(_)),
            "R30 must be rejected (host X30 = LR): {err:?}"
        );
    }

    // Positive control: R29 is a guest-state-backed host register and must
    // still lower successfully (the fix must not over-reject).
    assert!(
        try_lower_single_op(OpKind::Mov {
            dst: x86(X86Reg::R29),
            src: SrcOperand::Reg(x86(X86Reg::R16)),
            width: OpWidth::W64,
        })
        .is_ok(),
        "R29 (host X29, guest-backed) must still lower"
    );
}
#[test]
fn lowers_mov_apx_egpr_dst_as_identity_gpr() {
    let code = lower_single_op(OpKind::Mov {
        dst: x86(X86Reg::R16),
        src: SrcOperand::Imm(0x1234),
        width: OpWidth::W64,
    });

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x1234, 16).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_mov_apx_egpr_register_runtime() {
    let code = lower_single_op(OpKind::Mov {
        dst: x86(X86Reg::R16),
        src: SrcOperand::Reg(x86(X86Reg::R17)),
        width: OpWidth::W64,
    });

    let (out, out_nzcv, sp) = run_aarch64_code(
        &code,
        &[(16, 0x1616_1616_1616_1616), (17, 0xfeed_face_cafe_beef)],
        0b1010,
    );

    assert_eq!(out[16], 0xfeed_face_cafe_beef);
    assert_eq!(out[17], 0xfeed_face_cafe_beef);
    assert_eq!(out_nzcv, 0b1010);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_mov_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Mov {
            dst: x86(X86Reg::R31),
            src: SrcOperand::Imm(0x1234),
            width: OpWidth::W64,
        },
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_addsub_carry_apx_egpr_operands_runtime() {
    let ops = vec![
        OpKind::Add {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Reg(x86(X86Reg::R18)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Adc {
            dst: x86(X86Reg::R19),
            src1: x86(X86Reg::R20),
            src2: SrcOperand::Reg(x86(X86Reg::R21)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Sbb {
            dst: x86(X86Reg::R22),
            src1: x86(X86Reg::R23),
            src2: SrcOperand::Imm(0x34),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        OpKind::Sub {
            dst: x86(X86Reg::R25),
            src1: x86(X86Reg::R26),
            src2: SrcOperand::Reg(x86(X86Reg::R27)),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
    ];
    let code = lower_ops(ops);
    let regs = [
        (17, 0x1111_2222_3333_4444),
        (18, 0x0102_0304_0506_0708),
        (20, 0x100),
        (21, 0x10),
        (23, 0x80f5),
        (24, 0x2424_2424_2424_2424),
        (26, 0x8000_0000),
        (27, 1),
    ];
    let old_nzcv = 0b0010;
    let carry_in = true;
    let borrow_in = true;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(
        out[16],
        ref_addsub(
            0x1111_2222_3333_4444,
            0x0102_0304_0506_0708,
            false,
            OpWidth::W64,
        )
    );
    assert_eq!(
        out[19],
        ref_addsub_carry(0x100, 0x10, carry_in, false, OpWidth::W64)
    );
    assert_eq!(out[22], ref_x86_sbb(0x80f5, 0x34, borrow_in, OpWidth::W16));
    assert_eq!(out[25], ref_addsub(0x8000_0000, 1, true, OpWidth::W32));
    assert_eq!(
        out_nzcv,
        expected_addsub_nzcv(0x8000_0000, 1, true, OpWidth::W32)
    );
    assert_eq!(out[17], 0x1111_2222_3333_4444);
    assert_eq!(out[18], 0x0102_0304_0506_0708);
    assert_eq!(out[20], 0x100);
    assert_eq!(out[21], 0x10);
    assert_eq!(out[23], 0x80f5);
    assert_eq!(out[24], 0x2424_2424_2424_2424);
    assert_eq!(out[26], 0x8000_0000);
    assert_eq!(out[27], 1);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_addsub_carry_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Add {
            dst: x86(X86Reg::R31),
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R17)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Sub {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::R31),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
        OpKind::Add {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Adc {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Sbb {
            dst: x86(X86Reg::R31),
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_mul_apx_egpr_operands_runtime() {
    let full_src1 = 0xffff_0000_0000_0101;
    let full_src2 = 0x0002_0000_0000_0011;
    let full_product = (full_src1 as u128) * (full_src2 as u128);
    let flag_src1 = 0xff;
    let flag_src2 = 2;
    let code = lower_ops(vec![
        OpKind::MulU {
            dst_lo: x86(X86Reg::R16),
            dst_hi: None,
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Reg(x86(X86Reg::R18)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::MulS {
            dst_lo: x86(X86Reg::R19),
            dst_hi: None,
            src1: x86(X86Reg::R20),
            src2: SrcOperand::Imm(-7),
            width: OpWidth::W8,
            flags: FlagUpdate::None,
        },
        OpKind::MulU {
            dst_lo: x86(X86Reg::R21),
            dst_hi: Some(x86(X86Reg::R22)),
            src1: x86(X86Reg::R23),
            src2: SrcOperand::Reg(x86(X86Reg::R24)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::MulAdd {
            dst: x86(X86Reg::R25),
            acc: x86(X86Reg::R26),
            src1: x86(X86Reg::R27),
            src2: x86(X86Reg::R28),
            width: OpWidth::W16,
        },
        OpKind::MulU {
            dst_lo: x86(X86Reg::R29),
            dst_hi: None,
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Reg(x86(X86Reg::R18)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
    ]);
    let regs = [
        (17, flag_src1),
        (18, flag_src2),
        (20, 0x91),
        (23, full_src1),
        (24, full_src2),
        (26, 5),
        (27, 7),
        (28, 9),
        (15, 0x1515_1515_1515_1515),
    ];
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, 0b0101);

    assert_eq!(out[16], ref_mul(flag_src1, flag_src2, false, OpWidth::W64));
    assert_eq!(out[19], ref_mul(0x91, (-7_i64) as u64, true, OpWidth::W8));
    assert_eq!(out[21], full_product as u64);
    assert_eq!(out[22], (full_product >> 64) as u64);
    assert_eq!(out[25], (5 + 7 * 9) & OpWidth::W16.mask());
    assert_eq!(out[29], ref_mul(flag_src1, flag_src2, false, OpWidth::W8));
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(
        out_nzcv,
        expected_mul_nzcv(flag_src1, flag_src2, false, OpWidth::W8)
    );
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_mul_apx_r31_identity_mapping() {
    for kind in [
        OpKind::MulU {
            dst_lo: x86(X86Reg::R31),
            dst_hi: None,
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R17)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::MulU {
            dst_lo: x86(X86Reg::R16),
            dst_hi: None,
            src1: x86(X86Reg::R31),
            src2: SrcOperand::Reg(x86(X86Reg::R17)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::MulU {
            dst_lo: x86(X86Reg::R16),
            dst_hi: None,
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::MulS {
            dst_lo: x86(X86Reg::R16),
            dst_hi: Some(x86(X86Reg::R31)),
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Imm(-3),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::MulU {
            dst_lo: x86(X86Reg::R16),
            dst_hi: None,
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        OpKind::MulAdd {
            dst: x86(X86Reg::R16),
            acc: x86(X86Reg::R31),
            src1: x86(X86Reg::R17),
            src2: x86(X86Reg::R18),
            width: OpWidth::W64,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_div_apx_egpr_operands_runtime() {
    let divu_src1 = 0x1234_5678_9abc_def0;
    let divu_src2 = 10;
    let divs_src1 = 0xffff_ff85;
    let divs_src2 = (-7_i64) as u64;
    let pow2_no_rem_src = 0x4567_89ab_cdef_0120;
    let pow2_rem_src = 0xfedc_ba98_7654_3217;
    let code = lower_ops(vec![
        OpKind::DivU {
            quot: x86(X86Reg::R16),
            rem: Some(x86(X86Reg::R17)),
            src1: x86(X86Reg::R18),
            src2: SrcOperand::Reg(x86(X86Reg::R19)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::DivS {
            quot: x86(X86Reg::R20),
            rem: Some(x86(X86Reg::R21)),
            src1: x86(X86Reg::R22),
            src2: SrcOperand::Imm(-7),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
        OpKind::DivU {
            quot: x86(X86Reg::R23),
            rem: None,
            src1: x86(X86Reg::R24),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::DivU {
            quot: x86(X86Reg::R25),
            rem: Some(x86(X86Reg::R26)),
            src1: x86(X86Reg::R27),
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::DivU {
            quot: x86(X86Reg::R28),
            rem: Some(x86(X86Reg::R29)),
            src1: x86(X86Reg::R18),
            src2: SrcOperand::Reg(x86(X86Reg::R19)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
    ]);
    let regs = [
        (18, divu_src1),
        (19, divu_src2),
        (22, divs_src1),
        (24, pow2_no_rem_src),
        (27, pow2_rem_src),
        (15, 0x1515_1515_1515_1515),
    ];
    let old_nzcv = 0b1010;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
    let (divu_quot, divu_rem) = ref_div(divu_src1, divu_src2, false, OpWidth::W64);
    let (divs_quot, divs_rem) = ref_div(divs_src1, divs_src2, true, OpWidth::W32);
    let (subword_quot, subword_rem) = ref_div(divu_src1, divu_src2, false, OpWidth::W8);

    assert_eq!(out[16], divu_quot);
    assert_eq!(out[17], divu_rem);
    assert_eq!(out[20], divs_quot);
    assert_eq!(out[21], divs_rem);
    assert_eq!(out[23], pow2_no_rem_src / 8);
    assert_eq!(out[25], pow2_rem_src / 16);
    assert_eq!(out[26], pow2_rem_src % 16);
    assert_eq!(out[28], subword_quot);
    assert_eq!(out[29], subword_rem);
    assert_eq!(out[18], divu_src1);
    assert_eq!(out[19], divu_src2);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_div_apx_r31_identity_mapping() {
    for kind in [
        OpKind::DivU {
            quot: x86(X86Reg::R31),
            rem: None,
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R17)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::DivU {
            quot: x86(X86Reg::R16),
            rem: Some(x86(X86Reg::R31)),
            src1: x86(X86Reg::R17),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::DivU {
            quot: x86(X86Reg::R16),
            rem: None,
            src1: x86(X86Reg::R31),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::DivU {
            quot: x86(X86Reg::R16),
            rem: Some(x86(X86Reg::R17)),
            src1: x86(X86Reg::R18),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::DivS {
            quot: x86(X86Reg::R16),
            rem: Some(x86(X86Reg::R17)),
            src1: x86(X86Reg::R18),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        OpKind::DivU {
            quot: x86(X86Reg::R16),
            rem: Some(x86(X86Reg::R17)),
            src1: x86(X86Reg::R31),
            src2: SrcOperand::Imm(7),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_cwd_apx_egpr_operands_runtime() {
    let code = lower_ops(vec![
        OpKind::Cwd {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            width: OpWidth::W8,
        },
        OpKind::Cwd {
            dst: x86(X86Reg::R18),
            src: x86(X86Reg::R19),
            width: OpWidth::W16,
        },
        OpKind::Cwd {
            dst: x86(X86Reg::R20),
            src: x86(X86Reg::R21),
            width: OpWidth::W32,
        },
        OpKind::Cwd {
            dst: x86(X86Reg::R22),
            src: x86(X86Reg::R23),
            width: OpWidth::W64,
        },
    ]);
    let regs = [
        (17, 0x80),
        (19, 0x7fff),
        (21, 0x8000_0000),
        (23, 0x8000_0000_0000_0000),
        (24, 0x2424_2424_2424_2424),
    ];
    let old_nzcv = 0b1010;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[16], 0xff);
    assert_eq!(out[18], 0);
    assert_eq!(out[20], 0xffff_ffff);
    assert_eq!(out[22], u64::MAX);
    assert_eq!(out[24], 0x2424_2424_2424_2424);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_cwd_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Cwd {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W8,
        },
        OpKind::Cwd {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W16,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_clmul_apx_egpr_operands_runtime() {
    let word_a = 0x1234_5678;
    let word_b = 0x8000_0003;
    let half_a = 0x0001_ffff;
    let half_b = 0x0003_0002;
    let acc_a = 0xf0f0_00ff;
    let acc_init = 0xa5a5_5a5a;
    let code = lower_ops(vec![
        OpKind::ClMul {
            dst: x86(X86Reg::R16),
            dst_hi: Some(x86(X86Reg::R17)),
            src1: SrcOperand::Reg(x86(X86Reg::R18)),
            src2: SrcOperand::Reg(x86(X86Reg::R19)),
            elem_bits: 32,
            lanes: 1,
            acc: false,
        },
        OpKind::ClMul {
            dst: x86(X86Reg::R20),
            dst_hi: Some(x86(X86Reg::R21)),
            src1: SrcOperand::Reg(x86(X86Reg::R22)),
            src2: SrcOperand::Reg(x86(X86Reg::R23)),
            elem_bits: 16,
            lanes: 2,
            acc: false,
        },
        OpKind::ClMul {
            dst: x86(X86Reg::R24),
            dst_hi: None,
            src1: SrcOperand::Reg(x86(X86Reg::R25)),
            src2: SrcOperand::Imm64(2),
            elem_bits: 32,
            lanes: 1,
            acc: true,
        },
    ]);
    let regs = [
        (18, 0xaaaa_0000_0000_0000u64 | u64::from(word_a)),
        (19, 0xbbbb_0000_0000_0000u64 | u64::from(word_b)),
        (22, 0xcccc_0000_0000_0000u64 | u64::from(half_a)),
        (23, 0xdddd_0000_0000_0000u64 | u64::from(half_b)),
        (24, 0xeeee_0000_0000_0000u64 | u64::from(acc_init)),
        (25, 0xffff_0000_0000_0000u64 | u64::from(acc_a)),
        (13, 0x1313_1313_1313_1313),
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
    ];
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, 0b0110);
    let word_expected = ref_clmul(word_a, word_b, 32, 1, false, (0, 0));
    let half_expected = ref_clmul(half_a, half_b, 16, 2, false, (0, 0));
    let acc_expected = ref_clmul(acc_a, 2, 32, 1, true, (acc_init, 0));

    assert_eq!(out[16], u64::from(word_expected.0));
    assert_eq!(out[17], u64::from(word_expected.1));
    assert_eq!(out[20], u64::from(half_expected.0));
    assert_eq!(out[21], u64::from(half_expected.1));
    assert_eq!(out[24], u64::from(acc_expected.0));
    assert_eq!(out[18], 0xaaaa_0000_0000_0000u64 | u64::from(word_a));
    assert_eq!(out[19], 0xbbbb_0000_0000_0000u64 | u64::from(word_b));
    assert_eq!(out[22], 0xcccc_0000_0000_0000u64 | u64::from(half_a));
    assert_eq!(out[23], 0xdddd_0000_0000_0000u64 | u64::from(half_b));
    assert_eq!(out[25], 0xffff_0000_0000_0000u64 | u64::from(acc_a));
    assert_eq!(out[13], 0x1313_1313_1313_1313);
    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out_nzcv, 0b0110);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_clmul_apx_r31_identity_mapping() {
    for kind in [
        OpKind::ClMul {
            dst: x86(X86Reg::R31),
            dst_hi: None,
            src1: SrcOperand::Reg(x86(X86Reg::R16)),
            src2: SrcOperand::Reg(x86(X86Reg::R17)),
            elem_bits: 32,
            lanes: 1,
            acc: false,
        },
        OpKind::ClMul {
            dst: x86(X86Reg::R16),
            dst_hi: Some(x86(X86Reg::R31)),
            src1: SrcOperand::Reg(x86(X86Reg::R17)),
            src2: SrcOperand::Reg(x86(X86Reg::R18)),
            elem_bits: 32,
            lanes: 1,
            acc: false,
        },
        OpKind::ClMul {
            dst: x86(X86Reg::R16),
            dst_hi: None,
            src1: SrcOperand::Reg(x86(X86Reg::R31)),
            src2: SrcOperand::Reg(x86(X86Reg::R17)),
            elem_bits: 32,
            lanes: 1,
            acc: false,
        },
        OpKind::ClMul {
            dst: x86(X86Reg::R16),
            dst_hi: None,
            src1: SrcOperand::Reg(x86(X86Reg::R17)),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            elem_bits: 16,
            lanes: 2,
            acc: false,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_lea_apx_egpr_operands_runtime() {
    let code = lower_ops(vec![
        OpKind::Lea {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x86(X86Reg::R17)),
        },
        OpKind::Lea {
            dst: x86(X86Reg::R18),
            addr: Address::BaseOffset {
                base: x86(X86Reg::R19),
                offset: -0x28,
                disp_size: DispSize::Auto,
            },
        },
        OpKind::Lea {
            dst: x86(X86Reg::R20),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R21)),
                index: x86(X86Reg::R22),
                scale: 8,
                disp: 0x30,
                disp_size: DispSize::Auto,
            },
        },
        OpKind::Lea {
            dst: x86(X86Reg::R23),
            addr: Address::BaseIndexScale {
                base: None,
                index: x86(X86Reg::R24),
                scale: 4,
                disp: -0x10,
                disp_size: DispSize::Auto,
            },
        },
    ]);
    let regs = [
        (17, 0x1700),
        (19, 0x1900),
        (21, 0x2100),
        (22, 3),
        (24, 0x24),
        (25, 0x2525_2525_2525_2525),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[16], 0x1700);
    assert_eq!(out[17], 0x1700);
    assert_eq!(out[18], 0x1900 - 0x28);
    assert_eq!(out[19], 0x1900);
    assert_eq!(out[20], 0x2100 + 3 * 8 + 0x30);
    assert_eq!(out[21], 0x2100);
    assert_eq!(out[22], 3);
    assert_eq!(out[23], 0x24 * 4 - 0x10);
    assert_eq!(out[24], 0x24);
    assert_eq!(out[25], 0x2525_2525_2525_2525);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_lea_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Lea {
            dst: x86(X86Reg::R31),
            addr: Address::Direct(x86(X86Reg::R16)),
        },
        OpKind::Lea {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x86(X86Reg::R31)),
        },
        OpKind::Lea {
            dst: x86(X86Reg::R16),
            addr: Address::BaseOffset {
                base: x86(X86Reg::R31),
                offset: 1,
                disp_size: DispSize::Auto,
            },
        },
        OpKind::Lea {
            dst: x86(X86Reg::R16),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R31)),
                index: x86(X86Reg::R17),
                scale: 2,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        },
        OpKind::Lea {
            dst: x86(X86Reg::R16),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R17)),
                index: x86(X86Reg::R31),
                scale: 2,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_scalar_memory_apx_egpr_value_operands_runtime() {
    let mem_addr = 0x9000;
    let initial = 0x1122_3344_5566_7788;
    let store_value = 0xaabb_ccdd_eeff_0011;
    let code = lower_ops(vec![
        OpKind::Load {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Store {
            src: x86(X86Reg::R17),
            addr: Address::Direct(x(2)),
            width: MemWidth::B4,
        },
    ]);
    let words = code_words(&code);
    assert_eq!(words[0], enc_ldst_uimm_regs(3, 0b01, 0, 16, 1));
    assert_eq!(words[1], enc_ldst_uimm_regs(2, 0b00, 0, 17, 2));

    let old_nzcv = 0b1011;
    let regs = [
        (1, mem_addr),
        (2, mem_addr),
        (17, store_value),
        (18, 0x1818_1818_1818_1818),
    ];
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, initial, MemWidth::B8);
    let expected_mem = (initial & !0xffff_ffff) | (store_value & 0xffff_ffff);
    assert_eq!(out[16], initial);
    assert_eq!(out[17], store_value);
    assert_eq!(out[18], 0x1818_1818_1818_1818);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn fuses_scalar_memory_apx_egpr_value_operands() {
    let pre_index = lower_ops(vec![
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Load {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    ]);
    let words = code_words(&pre_index);
    assert_eq!(words[0], enc_ldst_simm_regs(3, 0b01, 0b11, 8, 16, 1));

    let post_index = lower_ops(vec![
        OpKind::Store {
            src: x86(X86Reg::R17),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(-8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ]);
    let words = code_words(&post_index);
    assert_eq!(words[0], enc_ldst_simm_regs(3, 0b00, 0b01, -8, 17, 1));

    let tmp = VReg::virt(0);
    let signed_load = lower_ops(vec![
        OpKind::Load {
            dst: tmp,
            addr: Address::Direct(x(1)),
            width: MemWidth::B1,
            sign: SignExtend::Sign,
        },
        OpKind::ZeroExtend {
            dst: x86(X86Reg::R18),
            src: tmp,
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    ]);
    let words = code_words(&signed_load);
    assert_eq!(words[0], enc_ldst_uimm_regs(0, 0b11, 0, 18, 1));
}
#[test]
fn rejects_scalar_memory_apx_r31_value_mapping() {
    for kind in [
        OpKind::Load {
            dst: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Store {
            src: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }

    let tmp = VReg::virt(0);
    let err = try_lower_ops(vec![
        OpKind::Load {
            dst: tmp,
            addr: Address::Direct(x(1)),
            width: MemWidth::B1,
            sign: SignExtend::Sign,
        },
        OpKind::ZeroExtend {
            dst: x86(X86Reg::R31),
            src: tmp,
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    ])
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
#[test]
fn lowers_scalar_memory_apx_egpr_address_operands_runtime() {
    let mem_addr = 0x9000_u64;
    let initial = 0x1122_3344_5566_7788;
    let store_value = 0xaabb_ccdd_eeff_0011;
    let index = 3_u64;
    let offset = 0x28_i64;
    let disp = -0x10_i32;
    let sib_base = (mem_addr as i64 - (index as i64) * 8 - i64::from(disp)) as u64;
    let code = lower_ops(vec![
        OpKind::Load {
            dst: x86(X86Reg::R17),
            addr: Address::Direct(x86(X86Reg::R16)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Store {
            src: x86(X86Reg::R19),
            addr: Address::BaseOffset {
                base: x86(X86Reg::R18),
                offset,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
        },
        OpKind::Load {
            dst: x86(X86Reg::R22),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R20)),
                index: x86(X86Reg::R21),
                scale: 8,
                disp,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    ]);

    let regs = [
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
        (16, mem_addr),
        (18, mem_addr - offset as u64),
        (19, store_value),
        (20, sib_base),
        (21, index),
        (22, 0x2222_2222_2222_2222),
    ];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, initial, MemWidth::B8);

    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], mem_addr);
    assert_eq!(out[17], initial);
    assert_eq!(out[18], mem_addr - offset as u64);
    assert_eq!(out[19], store_value);
    assert_eq!(out[20], sib_base);
    assert_eq!(out[21], index);
    assert_eq!(out[22], store_value);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, store_value);
}
#[test]
fn lowers_scalar_memory_apx_egpr_reg_offset_address_shape() {
    let code = lower_single_op(OpKind::Load {
        dst: x86(X86Reg::R16),
        addr: Address::BaseIndexScale {
            base: Some(x86(X86Reg::R17)),
            index: x86(X86Reg::R18),
            scale: 8,
            disp: 0,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B8,
        sign: SignExtend::Zero,
    });
    let words = code_words(&code);
    assert_eq!(words[0], enc_ldst_reg_regs(3, 0b01, 18, 17, 16, 0b011, 1));
}
#[test]
fn avoids_scalar_memory_apx_writeback_fusion_when_transfer_aliases_base() {
    let code = lower_ops(vec![
        OpKind::Add {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Load {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x86(X86Reg::R16)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    ]);
    let words = code_words(&code);
    assert_eq!(words[0], enc_addsub_imm_regs(1, 0, 0, 0, 8, 16, 16));
    assert_eq!(words[1], enc_ldst_uimm_regs(3, 0b01, 0, 16, 16));
}
#[test]
fn rejects_scalar_memory_apx_r31_address_mapping() {
    for kind in [
        OpKind::Load {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x86(X86Reg::R31)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Store {
            src: x86(X86Reg::R16),
            addr: Address::BaseOffset {
                base: x86(X86Reg::R31),
                offset: 8,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
        },
        OpKind::Load {
            dst: x86(X86Reg::R16),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R31)),
                index: x86(X86Reg::R17),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Load {
            dst: x86(X86Reg::R16),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R17)),
                index: x86(X86Reg::R31),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_apx_movbe_movrs_lifted_memory_shapes_runtime() {
    let movbe_load_addr = 0x9000_u64;
    let movbe_store_addr = 0x9020_u64;
    let movrs_addr = 0x9040_u64;
    let movbe_load_value = 0x1122_3344_u32;
    let movbe_store_value = 0x0102_0304_0506_0708_u64;
    let movrs_value = 0xabcd_u16;
    let tmp_load = x(9);
    let tmp_store = x(10);
    let code = lower_ops(vec![
        OpKind::Load {
            dst: tmp_load,
            addr: Address::Direct(x86(X86Reg::R17)),
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        },
        OpKind::Bswap {
            dst: x86(X86Reg::R16),
            src: tmp_load,
            width: OpWidth::W32,
        },
        OpKind::Bswap {
            dst: tmp_store,
            src: x86(X86Reg::R18),
            width: OpWidth::W64,
        },
        OpKind::Store {
            src: tmp_store,
            addr: Address::base_off(x86(X86Reg::R19), 8),
            width: MemWidth::B8,
        },
        OpKind::Load {
            dst: x86(X86Reg::R20),
            addr: Address::sib(Some(x86(X86Reg::R21)), x86(X86Reg::R22), 2, -0x10),
            width: MemWidth::B2,
            sign: SignExtend::Zero,
        },
    ]);

    let regs = [
        (9, 0x0909_0909_0909_0909),
        (10, 0x0a0a_0a0a_0a0a_0a0a),
        (17, movbe_load_addr),
        (18, movbe_store_value),
        (19, movbe_store_addr - 8),
        (20, 0x2020_2020_2020_2020),
        (21, movrs_addr),
        (22, 8),
    ];
    let (out, _, mem) = run_aarch64_code_with_regs_simd_and_memory(
        &code,
        &regs,
        &[],
        &[
            (movbe_load_addr, &movbe_load_value.to_le_bytes()),
            (movrs_addr, &movrs_value.to_le_bytes()),
        ],
        movbe_load_addr,
        0x48,
    );

    assert_eq!(out[9], movbe_load_value as u64);
    assert_eq!(out[10], movbe_store_value.swap_bytes());
    assert_eq!(out[16], movbe_load_value.swap_bytes() as u64);
    assert_eq!(out[17], movbe_load_addr);
    assert_eq!(out[18], movbe_store_value);
    assert_eq!(out[19], movbe_store_addr - 8);
    assert_eq!(out[20], movrs_value as u64);
    assert_eq!(out[21], movrs_addr);
    assert_eq!(out[22], 8);

    let store_off = (movbe_store_addr - movbe_load_addr) as usize;
    let movrs_off = (movrs_addr - movbe_load_addr) as usize;
    assert_eq!(&mem[..4], &movbe_load_value.to_le_bytes());
    assert_eq!(
        &mem[store_off..store_off + 8],
        &movbe_store_value.to_be_bytes()
    );
    assert_eq!(&mem[movrs_off..movrs_off + 2], &movrs_value.to_le_bytes());
}
#[test]
fn lowers_apx_push2_pop2_lifted_stack_shapes_runtime() {
    let stack_slot = 0x9000_u64;
    let stack_top = stack_slot + 16;
    let src1 = 0x1122_3344_5566_7788_u64;
    let src2 = 0x99aa_bbcc_ddee_ff00_u64;
    let tmp1 = x(9);
    let tmp2 = x(10);
    let rsp = x86(X86Reg::Rsp);
    let code = lower_ops(vec![
        OpKind::Mov {
            dst: tmp1,
            src: SrcOperand::Reg(x86(X86Reg::R16)),
            width: OpWidth::W64,
        },
        OpKind::Mov {
            dst: tmp2,
            src: SrcOperand::Reg(x86(X86Reg::R17)),
            width: OpWidth::W64,
        },
        OpKind::Sub {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Store {
            src: tmp1,
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
        },
        OpKind::Store {
            src: tmp2,
            addr: Address::base_off(rsp, 8),
            width: MemWidth::B8,
        },
        OpKind::Load {
            dst: tmp1,
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Load {
            dst: tmp2,
            addr: Address::base_off(rsp, 8),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Add {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Mov {
            dst: x86(X86Reg::R20),
            src: SrcOperand::Reg(tmp1),
            width: OpWidth::W64,
        },
        OpKind::Mov {
            dst: x86(X86Reg::R21),
            src: SrcOperand::Reg(tmp2),
            width: OpWidth::W64,
        },
    ]);

    let initial = [0xa5_u8; 16];
    let regs = [
        (4, stack_top),
        (9, 0x0909_0909_0909_0909),
        (10, 0x0a0a_0a0a_0a0a_0a0a),
        (16, src1),
        (17, src2),
        (20, 0x2020_2020_2020_2020),
        (21, 0x2121_2121_2121_2121),
    ];
    let (out, _, mem) = run_aarch64_code_with_regs_simd_and_memory(
        &code,
        &regs,
        &[],
        &[(stack_slot, &initial)],
        stack_slot,
        16,
    );

    let mut expected = Vec::new();
    expected.extend_from_slice(&src1.to_le_bytes());
    expected.extend_from_slice(&src2.to_le_bytes());
    assert_eq!(mem, expected);
    assert_eq!(out[4], stack_top);
    assert_eq!(out[9], src1);
    assert_eq!(out[10], src2);
    assert_eq!(out[16], src1);
    assert_eq!(out[17], src2);
    assert_eq!(out[20], src1);
    assert_eq!(out[21], src2);
}
#[test]
fn lowers_pair_memory_apx_egpr_value_operands_runtime() {
    let mem_addr = 0x9000;
    let initial = 0x1122_3344_5566_7788;
    let src1 = 0xaabb_ccdd_eeff_0011;
    let src2 = 0x2233_4455_6677_8899;
    let code = lower_ops(vec![
        OpKind::LoadPair {
            dst1: x86(X86Reg::R16),
            dst2: x86(X86Reg::R17),
            addr: Address::Direct(x(1)),
            width: MemWidth::B4,
        },
        OpKind::StorePair {
            src1: x86(X86Reg::R18),
            src2: x86(X86Reg::R19),
            addr: Address::Direct(x(2)),
            width: MemWidth::B4,
        },
    ]);
    let words = code_words(&code);
    assert_eq!(words[0], enc_ldp_regs(0b00, 0b10, true, 0, 16, 17, 1));
    assert_eq!(words[1], enc_ldp_regs(0b00, 0b10, false, 0, 18, 19, 2));

    let old_nzcv = 0b0011;
    let regs = [
        (1, mem_addr),
        (2, mem_addr),
        (18, src1),
        (19, src2),
        (20, 0x2020_2020_2020_2020),
    ];
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, initial, MemWidth::B8);
    let expected_mem = ((src2 & 0xffff_ffff) << 32) | (src1 & 0xffff_ffff);
    assert_eq!(out[16], initial & 0xffff_ffff);
    assert_eq!(out[17], initial >> 32);
    assert_eq!(out[18], src1);
    assert_eq!(out[19], src2);
    assert_eq!(out[20], 0x2020_2020_2020_2020);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn lowers_pair_memory_apx_egpr_address_operands_runtime() {
    let mem_addr = 0x9000_u64;
    let initial = 0x1122_3344_5566_7788;
    let src1 = 0xaabb_ccdd_eeff_0011;
    let src2 = 0x2233_4455_6677_8899;
    let index = 5_u64;
    let disp = 0x20_i32;
    let sib_base = mem_addr - index * 4 - disp as u64;
    let code = lower_ops(vec![
        OpKind::LoadPair {
            dst1: x86(X86Reg::R16),
            dst2: x86(X86Reg::R17),
            addr: Address::Direct(x86(X86Reg::R18)),
            width: MemWidth::B4,
        },
        OpKind::StorePair {
            src1: x86(X86Reg::R20),
            src2: x86(X86Reg::R21),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R22)),
                index: x86(X86Reg::R23),
                scale: 4,
                disp,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B4,
        },
    ]);

    let regs = [
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
        (18, mem_addr),
        (20, src1),
        (21, src2),
        (22, sib_base),
        (23, index),
    ];
    let old_nzcv = 0b1001;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, initial, MemWidth::B8);
    let expected_mem = ((src2 & 0xffff_ffff) << 32) | (src1 & 0xffff_ffff);

    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], initial & 0xffff_ffff);
    assert_eq!(out[17], initial >> 32);
    assert_eq!(out[18], mem_addr);
    assert_eq!(out[20], src1);
    assert_eq!(out[21], src2);
    assert_eq!(out[22], sib_base);
    assert_eq!(out[23], index);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn fuses_pair_memory_apx_egpr_value_operands() {
    let pre_index = lower_ops(vec![
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::LoadPair {
            dst1: x86(X86Reg::R16),
            dst2: x86(X86Reg::R17),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    ]);
    let words = code_words(&pre_index);
    assert_eq!(words[0], enc_ldp_regs(0b10, 0b11, true, 2, 16, 17, 1));

    let post_index = lower_ops(vec![
        OpKind::StorePair {
            src1: x86(X86Reg::R18),
            src2: x86(X86Reg::R19),
            addr: Address::Direct(x(1)),
            width: MemWidth::B4,
        },
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(-8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ]);
    let words = code_words(&post_index);
    assert_eq!(words[0], enc_ldp_regs(0b00, 0b01, false, -2, 18, 19, 1));

    let ldpsw = lower_ops(vec![
        OpKind::Load {
            dst: x86(X86Reg::R20),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            sign: SignExtend::Sign,
        },
        OpKind::Load {
            dst: x86(X86Reg::R21),
            addr: Address::BaseOffset {
                base: x(1),
                offset: 8,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
            sign: SignExtend::Sign,
        },
    ]);
    let words = code_words(&ldpsw);
    assert_eq!(words[0], enc_ldp_regs(0b01, 0b10, true, 0, 20, 21, 1));
}
#[test]
fn rejects_pair_memory_apx_r31_value_mapping() {
    for kind in [
        OpKind::LoadPair {
            dst1: x86(X86Reg::R31),
            dst2: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
        OpKind::LoadPair {
            dst1: x86(X86Reg::R16),
            dst2: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
        OpKind::StorePair {
            src1: x86(X86Reg::R31),
            src2: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
        OpKind::StorePair {
            src1: x86(X86Reg::R16),
            src2: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }

    let err = try_lower_ops(vec![
        OpKind::Load {
            dst: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            sign: SignExtend::Sign,
        },
        OpKind::Load {
            dst: x86(X86Reg::R16),
            addr: Address::BaseOffset {
                base: x(1),
                offset: 8,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
            sign: SignExtend::Sign,
        },
    ])
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
#[test]
fn lowers_predicated_memory_apx_egpr_condition_runtime() {
    let mem_addr = 0x9000;
    let load_code = lower_single_op(OpKind::PredLoad {
        dst: x(0),
        cond: x86(X86Reg::R18),
        addr: Address::Direct(x(1)),
        width: MemWidth::B8,
        signed: SignExtend::Zero,
    });
    let load_words = code_words(&load_code);
    assert_eq!(load_words[0], enc_test_branch(18, 0, false, 8));

    let old_nzcv = 0b1010;
    let regs_true = [(0, 0x1111), (1, mem_addr), (18, 1)];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &load_code,
        &regs_true,
        old_nzcv,
        mem_addr,
        0xaabb_ccdd_eeff_0011,
        MemWidth::B8,
    );
    assert_eq!(out[0], 0xaabb_ccdd_eeff_0011);
    assert_eq!(out[18], 1);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, 0xaabb_ccdd_eeff_0011);

    let regs_false = [(0, 0x2222), (1, mem_addr), (18, 0)];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &load_code,
        &regs_false,
        old_nzcv,
        mem_addr,
        0x1234_5678_9abc_def0,
        MemWidth::B8,
    );
    assert_eq!(out[0], 0x2222);
    assert_eq!(out[18], 0);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, 0x1234_5678_9abc_def0);

    let store_code = lower_single_op(OpKind::PredStore {
        src: SrcOperand::Reg(x(2)),
        cond: x86(X86Reg::R19),
        addr: Address::Direct(x(1)),
        width: MemWidth::B4,
    });
    let store_words = code_words(&store_code);
    assert_eq!(store_words[0], enc_test_branch(19, 0, false, 8));

    let regs_true = [(1, mem_addr), (2, 0x5566_7788), (19, 1)];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &store_code,
        &regs_true,
        old_nzcv,
        mem_addr,
        0xaabb_ccdd,
        MemWidth::B4,
    );
    assert_eq!(mem, 0x5566_7788);
    assert_eq!(out[2], 0x5566_7788);
    assert_eq!(out[19], 1);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);

    let regs_false = [(1, mem_addr), (2, 0x5566_7788), (19, 0)];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &store_code,
        &regs_false,
        old_nzcv,
        mem_addr,
        0xaabb_ccdd,
        MemWidth::B4,
    );
    assert_eq!(mem, 0xaabb_ccdd);
    assert_eq!(out[2], 0x5566_7788);
    assert_eq!(out[19], 0);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_predicated_memory_apx_egpr_address_operands_runtime() {
    let mem_addr = 0x9000_u64;
    let initial = 0x1122_3344_5566_7788;
    let store_value = 0xaabb_ccdd_eeff_0011;
    let index = 3_u64;
    let disp = -0x10_i32;
    let sib_base = mem_addr - index * 8 + 0x10;
    let code = lower_ops(vec![
        OpKind::PredLoad {
            dst: x86(X86Reg::R16),
            cond: x(2),
            addr: Address::Direct(x86(X86Reg::R17)),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        },
        OpKind::PredStore {
            src: SrcOperand::Reg(x86(X86Reg::R19)),
            cond: x(3),
            addr: Address::base_off(x86(X86Reg::R18), 0x20),
            width: MemWidth::B8,
        },
        OpKind::PredLoad {
            dst: x86(X86Reg::R22),
            cond: x(4),
            addr: Address::sib(Some(x86(X86Reg::R20)), x86(X86Reg::R21), 8, disp),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        },
    ]);

    let regs = [
        (2, 1),
        (3, 1),
        (4, 1),
        (17, mem_addr),
        (18, mem_addr - 0x20),
        (19, store_value),
        (20, sib_base),
        (21, index),
        (22, 0x2222_2222_2222_2222),
    ];
    let old_nzcv = 0b0101;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, initial, MemWidth::B8);

    assert_eq!(out[2], 1);
    assert_eq!(out[3], 1);
    assert_eq!(out[4], 1);
    assert_eq!(out[16], initial);
    assert_eq!(out[17], mem_addr);
    assert_eq!(out[18], mem_addr - 0x20);
    assert_eq!(out[19], store_value);
    assert_eq!(out[20], sib_base);
    assert_eq!(out[21], index);
    assert_eq!(out[22], store_value);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, store_value);
}
#[test]
fn rejects_predicated_memory_apx_r31_condition() {
    let load_err = try_lower_single_op(OpKind::PredLoad {
        dst: x(0),
        cond: x86(X86Reg::R31),
        addr: Address::Direct(x(1)),
        width: MemWidth::B8,
        signed: SignExtend::Zero,
    })
    .unwrap_err();
    assert!(matches!(load_err, LowerError::InvalidRegister(_)));

    let store_err = try_lower_single_op(OpKind::PredStore {
        src: SrcOperand::Reg(x(2)),
        cond: x86(X86Reg::R31),
        addr: Address::Direct(x(1)),
        width: MemWidth::B4,
    })
    .unwrap_err();
    assert!(matches!(store_err, LowerError::InvalidRegister(_)));
}
#[test]
fn rejects_predicated_memory_apx_r31_address_mapping() {
    for kind in [
        OpKind::PredLoad {
            dst: x86(X86Reg::R16),
            cond: x(2),
            addr: Address::Direct(x86(X86Reg::R31)),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        },
        OpKind::PredStore {
            src: SrcOperand::Reg(x86(X86Reg::R16)),
            cond: x(2),
            addr: Address::base_off(x86(X86Reg::R31), 0x20),
            width: MemWidth::B8,
        },
        OpKind::PredLoad {
            dst: x86(X86Reg::R16),
            cond: x(2),
            addr: Address::sib(Some(x86(X86Reg::R17)), x86(X86Reg::R31), 8, 0),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_rep_stos_apx_egpr_operands_runtime() {
    let base = 0x9000;
    let value = 0xbeef;
    let count = 3;
    let initial = 0x1122_3344_5566_7788;
    let code = lower_single_op(OpKind::RepStos {
        dst: x86(X86Reg::R16),
        src: x86(X86Reg::R17),
        count: x86(X86Reg::R18),
        width: MemWidth::B2,
    });

    let regs = [
        (0, 0x0101_0101_0101_0101),
        (1, 0x0202_0202_0202_0202),
        (16, base),
        (17, value),
        (18, count),
        (19, 0x1919_1919_1919_1919),
    ];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, base, initial, MemWidth::B8);

    assert_eq!(
        mem,
        ref_rep_stos_window(initial, value, MemWidth::B2, count)
    );
    assert_eq!(out[16], base + count * u64::from(MemWidth::B2.bytes()));
    assert_eq!(out[17], value);
    assert_eq!(out[18], 0);
    assert_eq!(out[19], 0x1919_1919_1919_1919);
    assert_eq!(out[0], 0x0101_0101_0101_0101);
    assert_eq!(out[1], 0x0202_0202_0202_0202);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_rep_stos_apx_r31_identity_mapping() {
    for (label, dst, src, count) in [
        ("dst", x86(X86Reg::R31), x86(X86Reg::R17), x86(X86Reg::R18)),
        ("src", x86(X86Reg::R16), x86(X86Reg::R31), x86(X86Reg::R18)),
        (
            "count",
            x86(X86Reg::R16),
            x86(X86Reg::R17),
            x86(X86Reg::R31),
        ),
    ] {
        let err = try_lower_single_op(OpKind::RepStos {
            dst,
            src,
            count,
            width: MemWidth::B8,
        })
        .unwrap_err();
        assert!(
            matches!(err, LowerError::InvalidRegister(_)),
            "{label}: {err:?}"
        );
    }
}
#[test]
fn rejects_io_in_apx_r31_destination_mapping() {
    let err = try_lower_single_op(OpKind::IoIn {
        dst: x86(X86Reg::R31),
        port: x86(X86Reg::Rdx),
        width: MemWidth::B4,
    })
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
#[test]
fn lowers_exclusive_memory_apx_egpr_value_operands() {
    let load = lower_single_op(OpKind::LoadExclusive {
        dst: x86(X86Reg::R16),
        addr: Address::Direct(x(1)),
        width: MemWidth::B8,
    });
    let words = code_words(&load);
    assert_eq!(words[0], enc_ldxr_regs(3, 16, 1));

    let store = lower_single_op(OpKind::StoreExclusive {
        status: x86(X86Reg::R17),
        src: x86(X86Reg::R18),
        addr: Address::Direct(x(1)),
        width: MemWidth::B4,
    });
    let words = code_words(&store);
    assert_eq!(words[0], enc_stxr_regs(2, 17, 18, 1));
}
#[test]
fn rejects_exclusive_memory_apx_r31_value_mapping() {
    let err = try_lower_single_op(OpKind::LoadExclusive {
        dst: x86(X86Reg::R31),
        addr: Address::Direct(x(1)),
        width: MemWidth::B8,
    })
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));

    for kind in [
        OpKind::StoreExclusive {
            status: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
        OpKind::StoreExclusive {
            status: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn fuses_lifted_ldclr_apx_egpr_value_operands() {
    let inverted = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Not {
            dst: inverted,
            src: x86(X86Reg::R18),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0,
        OpKind::AtomicRmw {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            src: inverted,
            op: AtomicOp::And,
            width: MemWidth::B8,
            order: MemoryOrder::Release,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_atomic_rmw_regs(3, 0, 1, 0, 0b001, 18, 1, 16).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cas_apx_egpr_lifted_shape_direct() {
    let success = VReg::virt(0);
    let code = lower_single_op(OpKind::Cas {
        dst: x86(X86Reg::R16),
        success,
        addr: Address::Direct(x(1)),
        expected: x86(X86Reg::R16),
        new_val: x86(X86Reg::R17),
        width: MemWidth::B8,
        order: MemoryOrder::AcqRel,
    });
    let words = code_words(&code);
    assert_eq!(words[0], enc_cas_regs(3, 1, 1, 16, 1, 17));
}
#[test]
fn lowers_cas_apx_egpr_observable_success_runtime() {
    let mem_addr = 0x9000_u64;
    let old_value = 0x1111_2222_3333_4444;
    let new_value = 0x5555_6666_7777_8888;
    let code = lower_single_op(OpKind::Cas {
        dst: x86(X86Reg::R16),
        success: x86(X86Reg::R18),
        addr: Address::Direct(x(1)),
        expected: x86(X86Reg::R16),
        new_val: x86(X86Reg::R17),
        width: MemWidth::B8,
        order: MemoryOrder::AcqRel,
    });

    let regs = [
        (1, mem_addr),
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
        (16, old_value),
        (17, new_value),
        (18, 0x1818_1818_1818_1818),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, old_value, MemWidth::B8);

    assert_eq!(out[1], mem_addr);
    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], old_value);
    assert_eq!(out[17], new_value);
    assert_eq!(out[18], 1);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, new_value);
}
#[test]
fn rejects_cas_apx_r31_value_mapping() {
    let success = VReg::virt(0);
    for kind in [
        OpKind::Cas {
            dst: x86(X86Reg::R31),
            success,
            addr: Address::Direct(x(1)),
            expected: x86(X86Reg::R16),
            new_val: x86(X86Reg::R17),
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        },
        OpKind::Cas {
            dst: x86(X86Reg::R16),
            success,
            addr: Address::Direct(x(1)),
            expected: x86(X86Reg::R31),
            new_val: x86(X86Reg::R17),
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        },
        OpKind::Cas {
            dst: x86(X86Reg::R16),
            success,
            addr: Address::Direct(x(1)),
            expected: x86(X86Reg::R16),
            new_val: x86(X86Reg::R31),
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        },
        OpKind::Cas {
            dst: x86(X86Reg::R16),
            success: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            expected: x86(X86Reg::R16),
            new_val: x86(X86Reg::R17),
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_bextr_bzhi_apx_egpr_operands_runtime() {
    let ops = vec![
        OpKind::Bextr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            control: VReg::Imm((12 << 8) | 4),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bextr {
            dst: x86(X86Reg::R18),
            src: x86(X86Reg::R19),
            control: x86(X86Reg::R20),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
        OpKind::Bzhi {
            dst: x86(X86Reg::R21),
            src: x86(X86Reg::R22),
            index: VReg::Imm(13),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bzhi {
            dst: x86(X86Reg::R23),
            src: x86(X86Reg::R24),
            index: x86(X86Reg::R25),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
    ];
    let code = lower_ops(ops);
    let regs = [
        (17, 0xfedc_ba98_7654_3210),
        (19, 0x7654_3210),
        (20, (10 << 8) | 3),
        (22, 0x1234_5678_9abc_0012),
        (24, 0xffff_80f5),
        (25, 8),
        (26, 0x2626_2626_2626_2626),
    ];
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, 0b1011);

    assert_eq!(
        out[16],
        ref_bextr(0xfedc_ba98_7654_3210, (12 << 8) | 4, OpWidth::W64)
    );
    assert_eq!(out[18], ref_bextr(0x7654_3210, (10 << 8) | 3, OpWidth::W32));
    assert_eq!(out[21], ref_bzhi(0x1234_5678_9abc_0012, 13, OpWidth::W64).0);
    assert_eq!(out[23], ref_bzhi(0xffff_80f5, 8, OpWidth::W16).0);
    assert_eq!(out[17], 0xfedc_ba98_7654_3210);
    assert_eq!(out[19], 0x7654_3210);
    assert_eq!(out[20], (10 << 8) | 3);
    assert_eq!(out[22], 0x1234_5678_9abc_0012);
    assert_eq!(out[24], 0xffff_80f5);
    assert_eq!(out[25], 8);
    assert_eq!(out[26], 0x2626_2626_2626_2626);
    assert_eq!(out_nzcv, 0b1011);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_apx_bextr_bzhi_lifted_memory_shape_runtime() {
    let mem_addr = 0x9000_u64;
    let mem_value = 0xfedc_ba98_7654_3210_u64;
    let index = 3_u64;
    let disp = 0x30_i32;
    let base = mem_addr - index * 8 - disp as u64;
    let loaded = x(9);
    let control = (10 << 8) | 3;
    let code = lower_ops(vec![
        OpKind::Load {
            dst: loaded,
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R17), 8, disp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Bextr {
            dst: x86(X86Reg::R18),
            src: loaded,
            control: VReg::Imm((12 << 8) | 4),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bextr {
            dst: x86(X86Reg::R19),
            src: loaded,
            control: x86(X86Reg::R20),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
        OpKind::Bzhi {
            dst: x86(X86Reg::R21),
            src: loaded,
            index: VReg::Imm(13),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bzhi {
            dst: x86(X86Reg::R22),
            src: loaded,
            index: x86(X86Reg::R23),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
    ]);

    let regs = [
        (16, base),
        (17, index),
        (18, 0x1818),
        (19, 0x1919),
        (20, control),
        (21, 0x2121),
        (22, 0x2222),
        (23, 8),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[9], mem_value);
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out[18], ref_bextr(mem_value, (12 << 8) | 4, OpWidth::W64));
    assert_eq!(out[19], ref_bextr(mem_value, control, OpWidth::W32));
    assert_eq!(out[20], control);
    assert_eq!(out[21], ref_bzhi(mem_value, 13, OpWidth::W64).0);
    assert_eq!(out[22], ref_bzhi(mem_value, 8, OpWidth::W16).0);
    assert_eq!(out[23], 8);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn lowers_apx_blsr_lifted_memory_shape_runtime() {
    let mem_addr = 0x9000_u64;
    let mem_value = 0xfedc_ba98_7654_3210_u64;
    let index = 4_u64;
    let disp = -0x20_i32;
    let base = mem_addr - index * 4 + 0x20;
    let loaded = x(9);
    let minus_one = x(10);
    let code = lower_ops(vec![
        OpKind::Load {
            dst: loaded,
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R17), 4, disp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Sub {
            dst: minus_one,
            src1: loaded,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::And {
            dst: x86(X86Reg::R18),
            src1: loaded,
            src2: SrcOperand::Reg(minus_one),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ]);

    let regs = [(16, base), (17, index), (18, 0x1818)];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[9], mem_value);
    assert_eq!(out[10], mem_value.wrapping_sub(1));
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out[18], mem_value & mem_value.wrapping_sub(1));
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn lowers_x86_adx_both_chains_widths_aliases_and_exact_flag_updates() {
    let cases = [
        (X86AdxKind::Adcx, OpWidth::W64, u64::MAX, 0, 0b1011_u8),
        (
            X86AdxKind::Adcx,
            OpWidth::W32,
            u64::from(u32::MAX),
            1,
            0b1001_u8,
        ),
        (X86AdxKind::Adox, OpWidth::W64, u64::MAX, 1, 0b1000_u8),
        (X86AdxKind::Adox, OpWidth::W32, 5, 7, 0b0111_u8),
    ];
    for (kind, width, lhs, rhs, old_nzcv) in cases {
        let code = lower_single_op(OpKind::X86Adx {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rcx),
            src2: x86(X86Reg::Rdx),
            width,
            kind,
            flags: adx_flags(kind),
        });
        let carry_in = match kind {
            X86AdxKind::Adcx => (old_nzcv & 0b0010) != 0,
            X86AdxKind::Adox => (old_nzcv & 0b0001) != 0,
        };
        let mask = width.mask();
        let sum = u128::from(lhs & mask) + u128::from(rhs & mask) + u128::from(carry_in);
        let expected = (sum as u64) & mask;
        let carry_out = (sum >> width.bits()) != 0;
        let selected_bit = if kind == X86AdxKind::Adcx {
            0b0010
        } else {
            0b0001
        };
        let expected_nzcv = (old_nzcv & !selected_bit) | (u8::from(carry_out) * selected_bit);
        let sentinels = [
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
        ];
        let mut regs = sentinels.to_vec();
        regs.extend([(1, lhs), (2, rhs)]);
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
        assert_eq!(out[0], expected, "{kind:?} {width:?} result");
        assert_eq!(out[1], lhs, "{kind:?} lhs preserved");
        assert_eq!(out[2], rhs, "{kind:?} rhs preserved");
        assert_eq!(out_nzcv, expected_nzcv, "{kind:?} selected chain output");
        assert_eq!(sp, 0x8000, "{kind:?} stack restored");
        for (reg, value) in sentinels {
            assert_eq!(out[reg as usize], value, "{kind:?} restored x{reg}");
        }
    }

    for (kind, width, dst, dst_index, lhs, rhs, old_nzcv, expected_nzcv) in [
        (
            X86AdxKind::Adcx,
            OpWidth::W64,
            X86Reg::Rcx,
            1_usize,
            u64::MAX,
            1_u64,
            0b1001_u8,
            0b1011_u8,
        ),
        (
            X86AdxKind::Adox,
            OpWidth::W32,
            X86Reg::Rdx,
            2_usize,
            u64::from(u32::MAX),
            1_u64,
            0b1010_u8,
            0b1011_u8,
        ),
    ] {
        let code = lower_single_op(OpKind::X86Adx {
            dst: x86(dst),
            src1: x86(X86Reg::Rcx),
            src2: x86(X86Reg::Rdx),
            width,
            kind,
            flags: adx_flags(kind),
        });
        let (out, out_nzcv, sp) = run_aarch64_code(
            &code,
            &[
                (1, lhs),
                (2, rhs),
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
                (15, 0x1515_1515_1515_1515),
            ],
            old_nzcv,
        );
        assert_eq!(out[dst_index], 0, "{kind:?} aliased destination result");
        assert_eq!(out_nzcv, expected_nzcv, "{kind:?} aliased flag update");
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(out[15], 0x1515_1515_1515_1515);
        assert_eq!(sp, 0x8000);
    }

    let code = lower_single_op(OpKind::X86Adx {
        dst: x86(X86Reg::Rdx),
        src1: x86(X86Reg::Rax),
        src2: x86(X86Reg::Rdx),
        width: OpWidth::W64,
        kind: X86AdxKind::Adox,
        flags: FlagUpdate::None,
    });
    let old_nzcv = 0b1111;
    let (out, out_nzcv, sp) = run_aarch64_code(
        &code,
        &[
            (0, 5),
            (2, 7),
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
        ],
        old_nzcv,
    );
    assert_eq!(out[2], 13, "aliased NF ADOX consumes OF as carry-in");
    assert_eq!(out[0], 5);
    assert_eq!(out_nzcv, old_nzcv, "NF ADOX preserves every NZCV bit");
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_malformed_x86_bls_and_adx_shapes() {
    for (name, op) in [
        (
            "BLS subword width",
            OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W16,
                kind: X86BlsKind::Blsr,
                flags: bls_flags(),
            },
        ),
        (
            "BLS flag contract",
            OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsi,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ),
        (
            "BLS virtual source",
            OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: VReg::virt(0),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsmsk,
                flags: FlagUpdate::None,
            },
        ),
        (
            "BLS reserved destination",
            OpKind::X86Bls {
                dst: x86(X86Reg::R30),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsr,
                flags: FlagUpdate::None,
            },
        ),
        (
            "ADX subword width",
            OpKind::X86Adx {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W16,
                kind: X86AdxKind::Adcx,
                flags: adx_flags(X86AdxKind::Adcx),
            },
        ),
        (
            "ADOX flag contract",
            OpKind::X86Adx {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W64,
                kind: X86AdxKind::Adox,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ),
        (
            "ADX immediate source",
            OpKind::X86Adx {
                dst: x86(X86Reg::Rax),
                src1: VReg::Imm(0),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W64,
                kind: X86AdxKind::Adcx,
                flags: FlagUpdate::None,
            },
        ),
        (
            "ADX reserved destination",
            OpKind::X86Adx {
                dst: x86(X86Reg::R30),
                src1: x86(X86Reg::Rcx),
                src2: x86(X86Reg::Rdx),
                width: OpWidth::W64,
                kind: X86AdxKind::Adox,
                flags: FlagUpdate::None,
            },
        ),
    ] {
        let err = try_lower_single_op(op).expect_err(name);
        assert!(
            matches!(
                err,
                LowerError::InvalidOperand { .. }
                    | LowerError::InvalidRegister(_)
                    | LowerError::UnsupportedOp { .. }
            ),
            "{name}: unexpected error {err:?}"
        );
    }
}
#[test]
fn lowers_apx_andn_lifted_memory_shape_runtime() {
    let mem_addr = 0x9000_u64;
    let mem_value = 0x0f0f_f0f0_ffff_0000_u64;
    let src_value = 0x00ff_00ff_3333_5555_u64;
    let index = 6_u64;
    let disp = 0x18_i32;
    let base = mem_addr - index * 8 - disp as u64;
    let loaded = x(9);
    let inverted = x(10);
    let code = lower_ops(vec![
        OpKind::Load {
            dst: loaded,
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R17), 8, disp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Not {
            dst: inverted,
            src: x86(X86Reg::R18),
            width: OpWidth::W64,
        },
        OpKind::And {
            dst: x86(X86Reg::R19),
            src1: inverted,
            src2: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ]);

    let regs = [(16, base), (17, index), (18, src_value), (19, 0x1919)];
    let old_nzcv = 0b1001;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[9], mem_value);
    assert_eq!(out[10], !src_value);
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out[18], src_value);
    assert_eq!(out[19], !src_value & mem_value);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn rejects_bextr_bzhi_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Bextr {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            control: VReg::Imm((12 << 8) | 4),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bextr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            control: x86(X86Reg::R31),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bzhi {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            index: VReg::Imm(13),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bzhi {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            index: x86(X86Reg::R31),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_bit_test_apx_egpr_operands_runtime() {
    let bt_src = 0x10;
    let bts_src = 0x0000_0001;
    let bts_index = 31;
    let btr_src = 0x1ff;
    let btc_src = 0x8001;
    let btc_index = 15;
    let code = lower_ops(vec![
        OpKind::Bt {
            src: x86(X86Reg::R16),
            index: SrcOperand::Imm(68),
            width: OpWidth::W64,
        },
        OpKind::Bts {
            dst: x86(X86Reg::R17),
            src: x86(X86Reg::R18),
            index: SrcOperand::Reg(x86(X86Reg::R19)),
            width: OpWidth::W32,
        },
        OpKind::Btr {
            dst: x86(X86Reg::R20),
            src: x86(X86Reg::R21),
            index: SrcOperand::Imm(0),
            width: OpWidth::W8,
        },
        OpKind::Btc {
            dst: x86(X86Reg::R22),
            src: x86(X86Reg::R23),
            index: SrcOperand::Reg(x86(X86Reg::R24)),
            width: OpWidth::W16,
        },
    ]);
    let regs = [
        (16, bt_src),
        (18, bts_src),
        (19, bts_index),
        (21, btr_src),
        (23, btc_src),
        (24, btc_index),
        (13, 0x1313_1313_1313_1313),
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
    ];
    let old_nzcv = 0b0101;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(
        out_nzcv,
        expected_bit_test_nzcv(old_nzcv, btc_src, btc_index, OpWidth::W16)
    );
    assert_eq!(
        out[17],
        ref_bit_update(bts_src, bts_index, BitTestAction::Set, OpWidth::W32)
    );
    assert_eq!(
        out[20],
        ref_bit_update(btr_src, 0, BitTestAction::Reset, OpWidth::W8)
    );
    assert_eq!(
        out[22],
        ref_bit_update(btc_src, btc_index, BitTestAction::Toggle, OpWidth::W16)
    );
    assert_eq!(out[16], bt_src);
    assert_eq!(out[18], bts_src);
    assert_eq!(out[19], bts_index);
    assert_eq!(out[21], btr_src);
    assert_eq!(out[23], btc_src);
    assert_eq!(out[24], btc_index);
    assert_eq!(out[13], 0x1313_1313_1313_1313);
    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_bit_test_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Bt {
            src: x86(X86Reg::R31),
            index: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
        OpKind::Bts {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            index: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
        OpKind::Btr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            index: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
        OpKind::Btc {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            index: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_pdep_pext_apx_egpr_operands_runtime() {
    let ops = vec![
        OpKind::Pdep {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            mask: VReg::Imm(0x1f0),
            width: OpWidth::W64,
        },
        OpKind::Pext {
            dst: x86(X86Reg::R18),
            src: x86(X86Reg::R19),
            mask: VReg::Imm(0x8040_0101_0000_1016_u64 as i64),
            width: OpWidth::W64,
        },
        OpKind::Pdep {
            dst: x86(X86Reg::R20),
            src: x86(X86Reg::R21),
            mask: x86(X86Reg::R22),
            width: OpWidth::W32,
        },
        OpKind::Pext {
            dst: x86(X86Reg::R23),
            src: x86(X86Reg::R24),
            mask: x86(X86Reg::R25),
            width: OpWidth::W16,
        },
    ];
    let code = lower_ops(ops);
    let regs = [
        (17, 0b1011_0110),
        (19, 0xf0f1_2233_4455_6677),
        (21, 0xffff_0001),
        (22, 0x8080_00f1),
        (24, 0xffff_1234),
        (25, 0xa55a),
        (26, 0x2626_2626_2626_2626),
    ];
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, 0b0110);

    assert_eq!(out[16], Aarch64Lowerer::eval_pdep(0b1011_0110, 0x1f0, 64));
    assert_eq!(
        out[18],
        Aarch64Lowerer::eval_pext(0xf0f1_2233_4455_6677, 0x8040_0101_0000_1016, 64,)
    );
    assert_eq!(
        out[20],
        Aarch64Lowerer::eval_pdep(0xffff_0001, 0x8080_00f1, 32)
    );
    assert_eq!(out[23], Aarch64Lowerer::eval_pext(0x1234, 0xa55a, 16));
    assert_eq!(out[17], 0b1011_0110);
    assert_eq!(out[19], 0xf0f1_2233_4455_6677);
    assert_eq!(out[21], 0xffff_0001);
    assert_eq!(out[22], 0x8080_00f1);
    assert_eq!(out[24], 0xffff_1234);
    assert_eq!(out[25], 0xa55a);
    assert_eq!(out[26], 0x2626_2626_2626_2626);
    assert_eq!(out_nzcv, 0b0110);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_pdep_pext_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Pdep {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            mask: VReg::Imm(0x1f0),
            width: OpWidth::W64,
        },
        OpKind::Pext {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            mask: VReg::Imm(0x1f0),
            width: OpWidth::W64,
        },
        OpKind::Pdep {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            mask: x86(X86Reg::R31),
            width: OpWidth::W64,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_bitfield_extend_truncate_apx_egpr_operands_runtime() {
    let extracted = VReg::virt(0);
    let bfi_dst_in = 0xaaaa_bbbb_ccdd_eeff;
    let bfi_src = 0x1234_5678_9abc_def0;
    let bfxil_src = 0x0fed_cba9_8765_4321;
    let bfxil_dst_in = 0x1111_2222_3333_4444;
    let zext_src = 0xffff_ffff_ffff_12ab;
    let sext_src = 0x91;
    let trunc_src = 0x1234_5678_9abc_def0;
    let code = lower_ops(vec![
        OpKind::Bfi {
            dst: x86(X86Reg::R16),
            dst_in: x86(X86Reg::R17),
            src: x86(X86Reg::R18),
            lsb: 8,
            width_bits: 12,
            op_width: OpWidth::W64,
        },
        OpKind::Bfx {
            dst: extracted,
            src: x86(X86Reg::R20),
            lsb: 16,
            width_bits: 8,
            sign_extend: false,
            op_width: OpWidth::W64,
        },
        OpKind::Bfi {
            dst: x86(X86Reg::R19),
            dst_in: x86(X86Reg::R21),
            src: extracted,
            lsb: 0,
            width_bits: 8,
            op_width: OpWidth::W64,
        },
        OpKind::ZeroExtend {
            dst: x86(X86Reg::R22),
            src: x86(X86Reg::R23),
            from_width: OpWidth::W8,
            to_width: OpWidth::W64,
        },
        OpKind::SignExtend {
            dst: x86(X86Reg::R24),
            src: x86(X86Reg::R25),
            from_width: OpWidth::W8,
            to_width: OpWidth::W16,
        },
        OpKind::Truncate {
            dst: x86(X86Reg::R26),
            src: x86(X86Reg::R27),
            from_width: OpWidth::W64,
            to_width: OpWidth::W32,
        },
    ]);
    let regs = [
        (17, bfi_dst_in),
        (18, bfi_src),
        (20, bfxil_src),
        (21, bfxil_dst_in),
        (23, zext_src),
        (25, sext_src),
        (27, trunc_src),
        (15, 0x1515_1515_1515_1515),
    ];
    let old_nzcv = 0b1001;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[16], ref_bfi(bfi_dst_in, bfi_src, 8, 12, OpWidth::W64));
    assert_eq!(
        out[19],
        ref_bfxil(bfxil_dst_in, bfxil_src, 16, 8, OpWidth::W64)
    );
    assert_eq!(out[22], zext_src & width_mask(OpWidth::W8));
    assert_eq!(
        out[24],
        sign_extend_width(sext_src, OpWidth::W8) as u64 & width_mask(OpWidth::W16)
    );
    assert_eq!(out[26], trunc_src & width_mask(OpWidth::W32));
    assert_eq!(out[17], bfi_dst_in);
    assert_eq!(out[18], bfi_src);
    assert_eq!(out[20], bfxil_src);
    assert_eq!(out[21], bfxil_dst_in);
    assert_eq!(out[23], zext_src);
    assert_eq!(out[25], sext_src);
    assert_eq!(out[27], trunc_src);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_bitfield_extend_truncate_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Bfi {
            dst: x86(X86Reg::R31),
            dst_in: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            lsb: 8,
            width_bits: 8,
            op_width: OpWidth::W64,
        },
        OpKind::Bfi {
            dst: x86(X86Reg::R16),
            dst_in: x86(X86Reg::R31),
            src: x86(X86Reg::R17),
            lsb: 8,
            width_bits: 8,
            op_width: OpWidth::W64,
        },
        OpKind::Bfi {
            dst: x86(X86Reg::R16),
            dst_in: x86(X86Reg::R17),
            src: x86(X86Reg::R31),
            lsb: 8,
            width_bits: 8,
            op_width: OpWidth::W64,
        },
        OpKind::ZeroExtend {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            from_width: OpWidth::W8,
            to_width: OpWidth::W64,
        },
        OpKind::SignExtend {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            from_width: OpWidth::W8,
            to_width: OpWidth::W16,
        },
        OpKind::Truncate {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            from_width: OpWidth::W64,
            to_width: OpWidth::W32,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }

    let extracted = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Bfx {
            dst: extracted,
            src: x86(X86Reg::R16),
            lsb: 8,
            width_bits: 8,
            sign_extend: false,
            op_width: OpWidth::W64,
        },
    );
    builder.push_op(
        0,
        OpKind::Bfi {
            dst: x86(X86Reg::R17),
            dst_in: x86(X86Reg::R31),
            src: extracted,
            lsb: 0,
            width_bits: 8,
            op_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
#[test]
fn lowers_select_apx_egpr_operands_runtime() {
    let cond_eq = VReg::virt(0);
    let cond_ne = VReg::virt(1);
    let inc_tmp = VReg::virt(2);
    let code = lower_ops(vec![
        OpKind::Select {
            dst: x86(X86Reg::R21),
            cond: x86(X86Reg::R16),
            src_true: x86(X86Reg::R18),
            src_false: x86(X86Reg::R19),
            width: OpWidth::W64,
        },
        OpKind::Select {
            dst: x86(X86Reg::R22),
            cond: x86(X86Reg::R17),
            src_true: x86(X86Reg::R18),
            src_false: x86(X86Reg::R19),
            width: OpWidth::W16,
        },
        OpKind::Select {
            dst: x86(X86Reg::R23),
            cond: VReg::Imm(0),
            src_true: x86(X86Reg::R18),
            src_false: VReg::Imm(0xabcd),
            width: OpWidth::W8,
        },
        OpKind::TestCondition {
            dst: cond_eq,
            cond: Condition::Eq,
        },
        OpKind::Select {
            dst: x86(X86Reg::R24),
            cond: cond_eq,
            src_true: x86(X86Reg::R25),
            src_false: x86(X86Reg::R26),
            width: OpWidth::W64,
        },
        OpKind::TestCondition {
            dst: cond_ne,
            cond: Condition::Ne,
        },
        OpKind::Add {
            dst: inc_tmp,
            src1: x86(X86Reg::R28),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Select {
            dst: x86(X86Reg::R27),
            cond: cond_ne,
            src_true: x86(X86Reg::R18),
            src_false: inc_tmp,
            width: OpWidth::W64,
        },
    ]);
    let words = code_words(&code);
    assert!(words.contains(&enc_csel_regs(1, 0, 0, 25, 26, 0, 24)));
    assert!(words.contains(&enc_csel_regs(1, 0, 1, 18, 28, 1, 27)));

    let regs = [
        (16, 1),
        (17, 0),
        (18, 0x1818_1818_1818_1818),
        (19, 0x1919_1919_1919_4321),
        (20, 0x2020_2020_2020_2020),
        (25, 0x2525_2525_2525_2525),
        (26, 0x2626_2626_2626_2626),
        (28, 0x2828_2828_2828_2828),
    ];
    let old_nzcv = 0b0100;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[21], 0x1818_1818_1818_1818);
    assert_eq!(out[22], 0x4321);
    assert_eq!(out[23], 0xcd);
    assert_eq!(out[24], 0x2525_2525_2525_2525);
    assert_eq!(out[27], 0x2828_2828_2828_2829);
    assert_eq!(out[16], 1);
    assert_eq!(out[17], 0);
    assert_eq!(out[18], 0x1818_1818_1818_1818);
    assert_eq!(out[19], 0x1919_1919_1919_4321);
    assert_eq!(out[20], 0x2020_2020_2020_2020);
    assert_eq!(out[25], 0x2525_2525_2525_2525);
    assert_eq!(out[26], 0x2626_2626_2626_2626);
    assert_eq!(out[28], 0x2828_2828_2828_2828);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_select_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Select {
            dst: x86(X86Reg::R16),
            cond: x86(X86Reg::R31),
            src_true: x86(X86Reg::R17),
            src_false: x86(X86Reg::R18),
            width: OpWidth::W64,
        },
        OpKind::Select {
            dst: x86(X86Reg::R31),
            cond: VReg::Imm(1),
            src_true: x86(X86Reg::R17),
            src_false: x86(X86Reg::R18),
            width: OpWidth::W8,
        },
        OpKind::Select {
            dst: x86(X86Reg::R16),
            cond: x86(X86Reg::R17),
            src_true: x86(X86Reg::R31),
            src_false: x86(X86Reg::R18),
            width: OpWidth::W64,
        },
        OpKind::Select {
            dst: x86(X86Reg::R16),
            cond: x86(X86Reg::R17),
            src_true: x86(X86Reg::R18),
            src_false: x86(X86Reg::R31),
            width: OpWidth::W64,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }

    let cond = VReg::virt(0);
    let err = try_lower_ops(vec![
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
        OpKind::Select {
            dst: x86(X86Reg::R31),
            cond,
            src_true: x86(X86Reg::R16),
            src_false: x86(X86Reg::R17),
            width: OpWidth::W64,
        },
    ])
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));

    let cond = VReg::virt(0);
    let err = try_lower_ops(vec![
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
        OpKind::Select {
            dst: x86(X86Reg::R16),
            cond,
            src_true: x86(X86Reg::R31),
            src_false: x86(X86Reg::R17),
            width: OpWidth::W64,
        },
    ])
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
#[test]
fn lowers_setcc_cmove_apx_egpr_operands_runtime() {
    let code = lower_ops(vec![
        OpKind::TestCondition {
            dst: x86(X86Reg::R16),
            cond: Condition::Eq,
        },
        OpKind::SetCC {
            dst: x86(X86Reg::R17),
            cond: Condition::Ne,
            width: OpWidth::W8,
        },
        OpKind::CMove {
            dst: x86(X86Reg::R18),
            src: x86(X86Reg::R19),
            cond: Condition::Eq,
            width: OpWidth::W64,
        },
        OpKind::CMove {
            dst: x86(X86Reg::R20),
            src: VReg::Imm(0x3456),
            cond: Condition::Ne,
            width: OpWidth::W32,
        },
        OpKind::CMove {
            dst: x86(X86Reg::R21),
            src: VReg::Imm(0xabcd),
            cond: Condition::Eq,
            width: OpWidth::W16,
        },
    ]);
    let old_nzcv = 0b0110;
    let regs = [
        (18, 0x1818_1818_1818_1818),
        (19, 0x1919_1919_1919_1919),
        (20, 0xffff_ffff_8765_4321),
        (21, 0x2121_2121_2121_2121),
        (22, 0x2222_2222_2222_2222),
    ];
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[16], 1);
    assert_eq!(out[17], 0);
    assert_eq!(out[18], 0x1919_1919_1919_1919);
    assert_eq!(out[19], 0x1919_1919_1919_1919);
    assert_eq!(out[20], 0x8765_4321);
    assert_eq!(out[21], 0xabcd);
    assert_eq!(out[22], 0x2222_2222_2222_2222);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_apx_setzucc_lifted_memory_shape_runtime() {
    let mem_addr = 0x9000_u64;
    let index = 3_u64;
    let disp = -5_i32;
    let base = mem_addr - index + 5;
    let tmp = x(9);
    let code = lower_ops(vec![
        OpKind::SetCC {
            dst: tmp,
            cond: Condition::Eq,
            width: OpWidth::W8,
        },
        OpKind::Store {
            src: tmp,
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R17), 1, disp),
            width: MemWidth::B1,
        },
    ]);

    let regs = [(9, 0x0909_0909_0909_0909), (16, base), (17, index)];
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, 0b0100, mem_addr, 0xaa, MemWidth::B1);
    assert_eq!(mem, 1);
    assert_eq!(out[9], 1);
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out_nzcv, 0b0100);
    assert_eq!(sp, 0x8000);

    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, 0, mem_addr, 0xaa, MemWidth::B1);
    assert_eq!(mem, 0);
    assert_eq!(out[9], 0);
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out_nzcv, 0);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_apx_cfcmov_lifted_memory_shapes_runtime() {
    let mem_addr = 0x9000_u64;
    let mem_value = 0xaabb_ccdd_eeff_0011;
    let index = 4_u64;
    let disp = -0x20_i32;
    let base = mem_addr - index * 8 + 0x20;
    let cond = x(9);
    let loaded = x(10);
    let load_code = lower_ops(vec![
        OpKind::SetCC {
            dst: cond,
            cond: Condition::Eq,
            width: OpWidth::W8,
        },
        OpKind::PredLoad {
            dst: loaded,
            cond,
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R17), 8, disp),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        },
        OpKind::Select {
            dst: x86(X86Reg::R18),
            cond,
            src_true: loaded,
            src_false: x86(X86Reg::R19),
            width: OpWidth::W64,
        },
    ]);

    let regs_true = [
        (16, base),
        (17, index),
        (18, 0x1818_1818_1818_1818),
        (19, 0x1919_1919_1919_1919),
    ];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &load_code,
        &regs_true,
        0b0100,
        mem_addr,
        mem_value,
        MemWidth::B8,
    );
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out[18], mem_value);
    assert_eq!(out[19], 0x1919_1919_1919_1919);
    assert_eq!(out[9], 1);
    assert_eq!(out[10], mem_value);
    assert_eq!(out_nzcv, 0b0100);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);

    let regs_false = [
        (16, 0xffff_0000),
        (17, index),
        (18, 0x1818_1818_1818_1818),
        (19, 0x1919_1919_1919_1919),
    ];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &load_code,
        &regs_false,
        0,
        mem_addr,
        mem_value,
        MemWidth::B8,
    );
    assert_eq!(out[16], 0xffff_0000);
    assert_eq!(out[17], index);
    assert_eq!(out[18], 0x1919_1919_1919_1919);
    assert_eq!(out[19], 0x1919_1919_1919_1919);
    assert_eq!(out[9], 0);
    assert_eq!(out_nzcv, 0);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);

    let cond = x(9);
    let store_code = lower_ops(vec![
        OpKind::SetCC {
            dst: cond,
            cond: Condition::Eq,
            width: OpWidth::W8,
        },
        OpKind::PredStore {
            src: SrcOperand::Reg(x86(X86Reg::R20)),
            cond,
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R17), 8, disp),
            width: MemWidth::B8,
        },
    ]);

    let regs_true = [(16, base), (17, index), (20, 0x1122_3344_5566_7788)];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &store_code,
        &regs_true,
        0b0100,
        mem_addr,
        mem_value,
        MemWidth::B8,
    );
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out[20], 0x1122_3344_5566_7788);
    assert_eq!(out[9], 1);
    assert_eq!(out_nzcv, 0b0100);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, 0x1122_3344_5566_7788);

    let regs_false = [(16, 0xffff_0000), (17, index), (20, 0x1122_3344_5566_7788)];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &store_code,
        &regs_false,
        0,
        mem_addr,
        mem_value,
        MemWidth::B8,
    );
    assert_eq!(out[16], 0xffff_0000);
    assert_eq!(out[17], index);
    assert_eq!(out[20], 0x1122_3344_5566_7788);
    assert_eq!(out[9], 0);
    assert_eq!(out_nzcv, 0);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn rejects_setcc_cmove_apx_r31_identity_mapping() {
    for kind in [
        OpKind::TestCondition {
            dst: x86(X86Reg::R31),
            cond: Condition::Eq,
        },
        OpKind::SetCC {
            dst: x86(X86Reg::R31),
            cond: Condition::Ne,
            width: OpWidth::W8,
        },
        OpKind::CMove {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            cond: Condition::Eq,
            width: OpWidth::W64,
        },
        OpKind::CMove {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            cond: Condition::Eq,
            width: OpWidth::W64,
        },
        OpKind::CMove {
            dst: x86(X86Reg::R31),
            src: VReg::Imm(1),
            cond: Condition::Eq,
            width: OpWidth::W32,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_control_flow_apx_egpr_operands_runtime() {
    let cond_sentinel = 0x1234_5678_9abc_def0;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let true_target = builder.create_block(4);
    let false_target = builder.create_block(8);
    builder.set_terminator(Terminator::CondBranch {
        cond: x86(X86Reg::R16),
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.push_op(
        4,
        OpKind::Mov {
            dst: x86(X86Reg::R17),
            src: SrcOperand::Imm(0x1111),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(false_target);
    builder.push_op(
        8,
        OpKind::Mov {
            dst: x86(X86Reg::R17),
            src: SrcOperand::Imm(0x2222),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let cond_func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&cond_func).unwrap();
    let cond_code = lowerer.finalize().unwrap();

    let (out, _, sp) = run_aarch64_code(&cond_code, &[(16, 1), (18, cond_sentinel)], 0);
    assert_eq!(out[16], 1);
    assert_eq!(out[17], 0x1111);
    assert_eq!(out[18], cond_sentinel);
    assert_eq!(sp, 0x8000);

    let (out, _, sp) = run_aarch64_code(&cond_code, &[(16, 0), (18, cond_sentinel)], 0);
    assert_eq!(out[16], 0);
    assert_eq!(out[17], 0x2222);
    assert_eq!(out[18], cond_sentinel);
    assert_eq!(sp, 0x8000);

    let switch_sentinel = 0x0fed_cba9_8765_4321;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let case0 = builder.create_block(4);
    let case1 = builder.create_block(8);
    let default = builder.create_block(12);
    builder.set_terminator(Terminator::Switch {
        index: x86(X86Reg::R18),
        targets: vec![case0, case1],
        default,
    });
    builder.switch_to_block(case0);
    builder.push_op(
        4,
        OpKind::Mov {
            dst: x86(X86Reg::R19),
            src: SrcOperand::Imm(0x1000),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(case1);
    builder.push_op(
        8,
        OpKind::Mov {
            dst: x86(X86Reg::R19),
            src: SrcOperand::Imm(0x1001),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(default);
    builder.push_op(
        12,
        OpKind::Mov {
            dst: x86(X86Reg::R19),
            src: SrcOperand::Imm(0xdddd),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let switch_func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&switch_func).unwrap();
    let switch_code = lowerer.finalize().unwrap();

    let (out, _, sp) = run_aarch64_code(&switch_code, &[(18, 1), (16, switch_sentinel)], 0);
    assert_eq!(out[16], switch_sentinel);
    assert_eq!(out[18], 1);
    assert_eq!(out[19], 0x1001);
    assert_eq!(sp, 0x8000);

    let (out, _, sp) = run_aarch64_code(&switch_code, &[(18, 7), (16, switch_sentinel)], 0);
    assert_eq!(out[16], switch_sentinel);
    assert_eq!(out[18], 7);
    assert_eq!(out[19], 0xdddd);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_control_flow_apx_r31_identity_mapping() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let true_target = builder.create_block(4);
    let false_target = builder.create_block(8);
    builder.set_terminator(Terminator::CondBranch {
        cond: x86(X86Reg::R31),
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(false_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let case0 = builder.create_block(4);
    let default = builder.create_block(8);
    builder.set_terminator(Terminator::Switch {
        index: x86(X86Reg::R31),
        targets: vec![case0],
        default,
    });
    builder.switch_to_block(case0);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(default);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
#[test]
fn lowers_prefetch_apx_egpr_address_operands() {
    let code = lower_ops(vec![
        OpKind::Prefetch {
            addr: Address::BaseOffset {
                base: x86(X86Reg::R16),
                offset: 24,
                disp_size: DispSize::Auto,
            },
            write: false,
        },
        OpKind::Prefetch {
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R16)),
                index: x86(X86Reg::R17),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            write: true,
        },
    ]);

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_uimm_regs(3, 0b10, 3, 0, 16).to_le_bytes());
    expected
        .extend_from_slice(&enc_ldst_reg_regs(3, 0b10, 17, 16, 0b10000, 0b011, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn rejects_prefetch_apx_r31_address_mapping() {
    for addr in [
        Address::Direct(x86(X86Reg::R31)),
        Address::BaseIndexScale {
            base: Some(x86(X86Reg::R16)),
            index: x86(X86Reg::R31),
            scale: 8,
            disp: 0,
            disp_size: DispSize::Auto,
        },
    ] {
        let err = try_lower_single_op(OpKind::Prefetch { addr, write: false }).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn fuses_lifted_extract_apx_egpr_operands_runtime() {
    let lo = VReg::virt(0);
    let hi = VReg::virt(1);
    let hi_value = 0x0123_4567_89ab_cdef;
    let lo_value = 0xfedc_ba98_7654_3210;
    let code = lower_ops(vec![
        OpKind::Shr {
            dst: lo,
            src: x86(X86Reg::R18),
            amount: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Shl {
            dst: hi,
            src: x86(X86Reg::R17),
            amount: SrcOperand::Imm(56),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Or {
            dst: x86(X86Reg::R16),
            src1: lo,
            src2: SrcOperand::Reg(hi),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ]);
    let words = code_words(&code);
    assert_eq!(words.len(), 2);
    assert_eq!(words[0] & 0x1f, 16);
    assert_eq!((words[0] >> 5) & 0x1f, 17);
    assert_eq!((words[0] >> 10) & 0x3f, 8);
    assert_eq!((words[0] >> 16) & 0x1f, 18);
    assert_eq!(words[0] >> 31, 1);

    let sentinel = 0x1515_1515_1515_1515;
    let (out, _, sp) =
        run_aarch64_code(&code, &[(17, hi_value), (18, lo_value), (21, sentinel)], 0);
    assert_eq!(out[16], (lo_value >> 8) | (hi_value << 56));
    assert_eq!(out[17], hi_value);
    assert_eq!(out[18], lo_value);
    assert_eq!(out[21], sentinel);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_shift_rotate_apx_egpr_operands_runtime() {
    let ops = vec![
        OpKind::Shl {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            amount: SrcOperand::Reg(x86(X86Reg::R18)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Shr {
            dst: x86(X86Reg::R19),
            src: x86(X86Reg::R20),
            amount: SrcOperand::Imm(4),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
        OpKind::Sar {
            dst: x86(X86Reg::R21),
            src: x86(X86Reg::R22),
            amount: SrcOperand::Reg(x86(X86Reg::R23)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        OpKind::Ror {
            dst: x86(X86Reg::R24),
            src: x86(X86Reg::R25),
            amount: SrcOperand::Reg(x86(X86Reg::R26)),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        OpKind::Rol {
            dst: x86(X86Reg::R27),
            src: x86(X86Reg::R28),
            amount: SrcOperand::Imm(9),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ];
    let code = lower_ops(ops);
    let regs = [
        (17, 0x0123_4567_89ab_cdef),
        (18, 4),
        (20, 0x8000_0010),
        (22, 0x80),
        (23, 3),
        (25, 0x8001),
        (26, 4),
        (28, 0x8000_0000_0000_0001),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(
        out[16],
        ref_shift_reg(0x0123_4567_89ab_cdef, 4, ShiftOp::Lsl, OpWidth::W64)
    );
    assert_eq!(
        out[19] & width_mask(OpWidth::W32),
        ref_shift_reg(0x8000_0010, 4, ShiftOp::Lsr, OpWidth::W32)
    );
    assert_eq!(
        out[21] & width_mask(OpWidth::W8),
        ref_shift_reg(0x80, 3, ShiftOp::Asr, OpWidth::W8)
    );
    assert_eq!(
        out_nzcv,
        expected_shift_nzcv(
            old_nzcv,
            0x80,
            3,
            ShiftOp::Asr,
            OpWidth::W8,
            FlagUpdate::All,
        )
    );
    assert_eq!(
        out[24] & width_mask(OpWidth::W16),
        ref_ror_reg(0x8001, 4, OpWidth::W16)
    );
    assert_eq!(out[27], ref_rol_reg(0x8000_0000_0000_0001, 9, OpWidth::W64));
    assert_eq!(out[17], 0x0123_4567_89ab_cdef);
    assert_eq!(out[18], 4);
    assert_eq!(out[20], 0x8000_0010);
    assert_eq!(out[22], 0x80);
    assert_eq!(out[23], 3);
    assert_eq!(out[25], 0x8001);
    assert_eq!(out[26], 4);
    assert_eq!(out[28], 0x8000_0000_0000_0001);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_shift_rotate_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Shl {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Shr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Sar {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            amount: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Rol {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            amount: SrcOperand::Reg(x86(X86Reg::R17)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::BidirShift {
            dst: x86(X86Reg::R16),
            src: SrcOperand::Reg(x86(X86Reg::R31)),
            amount: SrcOperand::Reg(x86(X86Reg::R17)),
            kind: 2,
            width: OpWidth::W64,
        },
        OpKind::Shld {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            amount: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Rcr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            amount: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_unary_bit_apx_egpr_operands_runtime() {
    let ops = vec![
        OpKind::Not {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            width: OpWidth::W16,
        },
        OpKind::Clz {
            dst: x86(X86Reg::R18),
            src: x86(X86Reg::R19),
            width: OpWidth::W32,
        },
        OpKind::Ctz {
            dst: x86(X86Reg::R20),
            src: x86(X86Reg::R21),
            width: OpWidth::W64,
        },
        OpKind::Popcnt {
            dst: x86(X86Reg::R22),
            src: x86(X86Reg::R23),
            width: OpWidth::W64,
        },
        OpKind::Bswap {
            dst: x86(X86Reg::R24),
            src: x86(X86Reg::R25),
            width: OpWidth::W32,
        },
        OpKind::Rbit {
            dst: x86(X86Reg::R26),
            src: x86(X86Reg::R27),
            width: OpWidth::W64,
        },
    ];
    let code = lower_ops(ops);
    let regs = [
        (17, 0x1234),
        (19, 0x0000_00f0),
        (21, 0x1000),
        (23, 0xf0f0_0000_0000_0001),
        (25, 0x1234_5678),
        (27, 0x0123_4567_89ab_cdef),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[16], !0x1234 & width_mask(OpWidth::W16));
    assert_eq!(out[18], (0x0000_00f0_u32).leading_zeros() as u64);
    assert_eq!(out[20], (0x1000_u64).trailing_zeros() as u64);
    assert_eq!(out[22], 0xf0f0_0000_0000_0001_u64.count_ones() as u64);
    assert_eq!(out[24], (0x1234_5678_u32).swap_bytes() as u64);
    assert_eq!(out[26], 0x0123_4567_89ab_cdef_u64.reverse_bits());
    assert_eq!(out[17], 0x1234);
    assert_eq!(out[19], 0x0000_00f0);
    assert_eq!(out[21], 0x1000);
    assert_eq!(out[23], 0xf0f0_0000_0000_0001);
    assert_eq!(out[25], 0x1234_5678);
    assert_eq!(out[27], 0x0123_4567_89ab_cdef);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_apx_count_lifted_memory_shape_runtime() {
    let mem_addr = 0x9000_u64;
    let mem_value = 0x00ff_0000_0000_0010_u64;
    let index = 5_u64;
    let disp = -0x28_i32;
    let base = mem_addr - index * 8 + 0x28;
    let loaded = x(9);
    let code = lower_ops(vec![
        OpKind::Load {
            dst: loaded,
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R17), 8, disp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        OpKind::Clz {
            dst: x86(X86Reg::R18),
            src: loaded,
            width: OpWidth::W64,
        },
        OpKind::Ctz {
            dst: x86(X86Reg::R19),
            src: loaded,
            width: OpWidth::W64,
        },
        OpKind::Popcnt {
            dst: x86(X86Reg::R20),
            src: loaded,
            width: OpWidth::W64,
        },
    ]);

    let regs = [
        (16, base),
        (17, index),
        (18, 0x1818),
        (19, 0x1919),
        (20, 0x2020),
    ];
    let old_nzcv = 0b1010;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[9], mem_value);
    assert_eq!(out[16], base);
    assert_eq!(out[17], index);
    assert_eq!(out[18], mem_value.leading_zeros() as u64);
    assert_eq!(out[19], mem_value.trailing_zeros() as u64);
    assert_eq!(out[20], mem_value.count_ones() as u64);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn lowers_bit_scan_apx_egpr_operands_runtime() {
    let bsf_code = lower_single_op(OpKind::Bsf {
        dst: x86(X86Reg::R16),
        src: x86(X86Reg::R17),
        width: OpWidth::W64,
        flags: FlagUpdate::All,
    });
    let old_nzcv = 0b1111;
    let src_value = 0x8000_0000_0000_0010;
    let (out, out_nzcv, sp) =
        run_aarch64_code(&bsf_code, &[(17, src_value), (18, 0x1818)], old_nzcv);
    assert_eq!(out[16], ref_bsf(src_value, OpWidth::W64));
    assert_eq!(
        out_nzcv,
        expected_logic_source_nzcv(old_nzcv, src_value, OpWidth::W64, FlagUpdate::All)
    );
    assert_eq!(out[17], src_value);
    assert_eq!(out[18], 0x1818);
    assert_eq!(sp, 0x8000);

    let bsr_code = lower_single_op(OpKind::Bsr {
        dst: x86(X86Reg::R19),
        src: x86(X86Reg::R20),
        width: OpWidth::W8,
        flags: FlagUpdate::All,
    });
    let old_nzcv = 0b0011;
    let src_value = 0x80;
    let (out, out_nzcv, sp) = run_aarch64_code(&bsr_code, &[(20, src_value)], old_nzcv);
    assert_eq!(out[19], ref_bsr(src_value, OpWidth::W8));
    assert_eq!(
        out_nzcv,
        expected_logic_source_nzcv(old_nzcv, src_value, OpWidth::W8, FlagUpdate::All)
    );
    assert_eq!(out[20], src_value);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_unary_bit_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Not {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W64,
        },
        OpKind::Clz {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W32,
        },
        OpKind::Ctz {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W64,
        },
        OpKind::Popcnt {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W64,
        },
        OpKind::Bswap {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W32,
        },
        OpKind::Rbit {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W64,
        },
        OpKind::Bsf {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        OpKind::Bsr {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_unary_arithmetic_apx_egpr_operands_runtime() {
    let neg_x_src = 0x0000_0000_0000_0011;
    let neg_b_src = 0x91;
    let neg_h_flags_src = 0x8000;
    let inc_b_flags_src = 0x7f;
    let dec_x_flags_src = 0x8000_0000_0000_0000;
    let code = lower_ops(vec![
        OpKind::Neg {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Neg {
            dst: x86(X86Reg::R18),
            src: x86(X86Reg::R19),
            width: OpWidth::W8,
            flags: FlagUpdate::None,
        },
        OpKind::Neg {
            dst: x86(X86Reg::R20),
            src: x86(X86Reg::R21),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        },
        OpKind::Inc {
            dst: x86(X86Reg::R22),
            src: x86(X86Reg::R23),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        OpKind::Dec {
            dst: x86(X86Reg::R24),
            src: x86(X86Reg::R25),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    ]);
    let regs = [
        (17, neg_x_src),
        (19, neg_b_src),
        (21, neg_h_flags_src),
        (23, inc_b_flags_src),
        (25, dec_x_flags_src),
        (13, 0x1313_1313_1313_1313),
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
    ];
    let old_nzcv = 0b0010;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[16], ref_addsub(0, neg_x_src, true, OpWidth::W64));
    assert_eq!(out[18], ref_addsub(0, neg_b_src, true, OpWidth::W8));
    assert_eq!(out[20], ref_addsub(0, neg_h_flags_src, true, OpWidth::W16));
    assert_eq!(out[22], ref_inc_dec(inc_b_flags_src, false, OpWidth::W8));
    assert_eq!(out[24], ref_inc_dec(dec_x_flags_src, true, OpWidth::W64));
    let after_neg_h_nzcv = expected_addsub_nzcv(0, neg_h_flags_src, true, OpWidth::W16);
    let after_inc_b_nzcv =
        expected_inc_dec_nzcv(after_neg_h_nzcv, inc_b_flags_src, false, OpWidth::W8);
    assert_eq!(
        out_nzcv,
        expected_inc_dec_nzcv(after_inc_b_nzcv, dec_x_flags_src, true, OpWidth::W64)
    );
    assert_eq!(out[17], neg_x_src);
    assert_eq!(out[19], neg_b_src);
    assert_eq!(out[21], neg_h_flags_src);
    assert_eq!(out[23], inc_b_flags_src);
    assert_eq!(out[25], dec_x_flags_src);
    assert_eq!(out[13], 0x1313_1313_1313_1313);
    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_unary_arithmetic_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Neg {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Neg {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        OpKind::Inc {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        },
        OpKind::Inc {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W8,
            flags: FlagUpdate::None,
        },
        OpKind::Dec {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R31),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
