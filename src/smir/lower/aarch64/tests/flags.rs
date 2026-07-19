//! tests::flags tests

use super::*;
use crate::smir::lower::aarch64::*;

#[test]
fn lowers_x86_sbb_flag_outputs_as_canonical_borrow_cf() {
    const UPPER: u64 = 0xAAAA_BBBB_CCCC_0000;
    const SCRATCH16: u64 = 0x1616_1616_1616_1616;
    const SCRATCH17: u64 = 0x1717_1717_1717_1717;

    for flagm in [false, true] {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for (src1, src2, borrow_in) in [(1_u64 << (width.bits() - 1), 1, false), (0, 0, true)] {
                let mask = width.mask();
                let dst_initial = (UPPER & !mask) | src1;
                let code = lower_ops_with_flagm_features(
                    vec![OpKind::Sbb {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                        width,
                        flags: FlagUpdate::All,
                    }],
                    flagm,
                    false,
                );
                let result = ref_x86_sbb(src1, src2, borrow_in, width);
                let expected = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                    (dst_initial & !mask) | result
                } else {
                    result
                };
                let initial_nzcv = 0b1101 | (u8::from(borrow_in) << 1);
                let (out, nzcv, sp) = run_aarch64_code(
                    &code,
                    &[
                        (0, dst_initial),
                        (1, src2),
                        (16, SCRATCH16),
                        (17, SCRATCH17),
                    ],
                    initial_nzcv,
                );

                assert_eq!(out[0], expected, "SBB {width:?} flag-setting result");
                assert_eq!(
                    nzcv,
                    expected_x86_sbb_nzcv(src1, src2, borrow_in, width),
                    "SBB {width:?} must expose x86 borrow in NZCV.C"
                );
                assert_eq!(out[1], src2, "SBB {width:?} source");
                assert_eq!(out[16], SCRATCH16, "SBB {width:?} x16 scratch");
                assert_eq!(out[17], SCRATCH17, "SBB {width:?} x17 scratch");
                assert_eq!(sp, 0x8000, "SBB {width:?} stack");
            }
        }
    }
}
#[test]
fn lowers_cmp_x_zero_base_zero_imm_as_cmp_zero_regs() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0),
            src2: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 1, 0, 0, 31, 31, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmp_w_zero_base_masked_zero_imm_as_cmp_zero_regs() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0),
            src2: SrcOperand::Imm64(0x1_0000_0000),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 1, 0, 0, 31, 31, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmp_two_imms_as_constant_flags() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(5),
            src2: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 1, 0, 0, 31, 31, 31).to_le_bytes());
    expected.extend_from_slice(&enc_condcmp(1, 1, false, 31, 1, 31, 0b0010).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0x7f),
            src2: SrcOperand::Imm(0xff),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 1, 0, 0, 31, 31, 31).to_le_bytes());
    expected.extend_from_slice(&enc_condcmp(0, 1, false, 31, 1, 31, 0b1001).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmp_zero_base_register_sources() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0),
            src2: SrcOperand::Reg(x(1)),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 1, 0, 0, 31, 31, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0),
            src2: SrcOperand::Shifted {
                reg: x(1),
                shift: ShiftOp::Lsl,
                amount: 3,
            },
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 1, 0, 3, 31, 31, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0),
            src2: SrcOperand::Extended {
                reg: x(1),
                extend: ExtendOp::Uxtw,
                shift: 2,
            },
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    for word in zero_base_extended_flags_words(1, 1, 0b010, 2, 1) {
        expected.extend_from_slice(&word.to_le_bytes());
    }
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmp_w8_zero_base_zero_source_reg_as_constant_flags() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0),
            src2: SrcOperand::Reg(VReg::Imm(0)),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 1, 0, 0, 31, 31, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_flag_setting_div_without_touching_nzcv() {
    assert_div_w64_lowering(
        "divu_reg_flags",
        false,
        0,
        Some(3),
        1,
        SrcOperand::Reg(x(2)),
        Some(2),
        0x1234_5678_9abc_def0,
        0x101,
        FlagUpdate::All,
    );
    assert_div_w64_lowering(
        "divs_outputs_alias_both_sources_flags",
        true,
        2,
        Some(1),
        1,
        SrcOperand::Reg(x(2)),
        Some(2),
        0xffff_ffff_f8a4_32eb,
        0x141,
        FlagUpdate::All,
    );
    assert_div_w64_lowering(
        "divu_imm_one_flags",
        false,
        0,
        Some(3),
        1,
        SrcOperand::Imm(1),
        None,
        0x1234_5678_9abc_def0,
        1,
        FlagUpdate::All,
    );
}
#[test]
fn lowers_test_two_imms_as_constant_flags() {
    let cases = [
        (
            OpKind::Test {
                src1: VReg::Imm(0x10),
                src2: SrcOperand::Imm(0x20),
                width: OpWidth::W8,
            },
            vec![enc_logical_reg_n(0, 0b11, 0, 31, 31, 31)],
        ),
        (
            OpKind::Test {
                src1: VReg::Imm(0x03),
                src2: SrcOperand::Imm(0x05),
                width: OpWidth::W64,
            },
            vec![enc_msr_sysreg(31, 3, 4, 2, 0)],
        ),
        (
            OpKind::Test {
                src1: VReg::Imm(0x8001),
                src2: SrcOperand::Imm(0x8000),
                width: OpWidth::W16,
            },
            // 0x8001 & 0x8000 = 0x8000: negative W16 result (N=1, Z=C=V=0).
            // Routed through the ccmp fallback (subs wzr,wzr,wzr; ccmp ...)
            // instead of `cmp wsp, #1`, whose Rn = 31 is WSP.
            vec![
                enc_addsub_shift_regs(0, 1, 1, 0, 0, 31, 31, 31),
                0x7a5f_13e8u32,
            ],
        ),
    ];

    for (kind, expected_words) in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(0, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        for word in expected_words {
            expected.extend_from_slice(&word.to_le_bytes());
        }
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
}
#[test]
fn lowers_test_all_ones_left_imm_extended_zero_base_with_scratch() {
    let cases = [
        (
            OpKind::Test {
                src1: VReg::Imm(-1),
                src2: SrcOperand::Extended {
                    reg: x(2),
                    extend: ExtendOp::Uxtw,
                    shift: 0,
                },
                width: OpWidth::W64,
            },
            zero_base_extended_flags_words(1, 0, 0b010, 0, 2),
        ),
        (
            OpKind::Test {
                src1: VReg::Imm(-1),
                src2: SrcOperand::Extended {
                    reg: x(2),
                    extend: ExtendOp::Sxtw,
                    shift: 2,
                },
                width: OpWidth::W64,
            },
            zero_base_extended_flags_words(1, 0, 0b110, 2, 2),
        ),
        (
            OpKind::Test {
                src1: VReg::Imm(0x1_ffff_ffff),
                src2: SrcOperand::Extended {
                    reg: x(2),
                    extend: ExtendOp::Uxtb,
                    shift: 1,
                },
                width: OpWidth::W32,
            },
            zero_base_extended_flags_words(0, 0, 0b000, 1, 2),
        ),
    ];

    for (kind, expected_words) in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(0, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        for word in expected_words {
            expected.extend_from_slice(&word.to_le_bytes());
        }
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
}
#[test]
fn lowers_sparse_test_immediate_via_scratch() {
    let src = 0x8000_0000_0000_1055;
    let imm = 0x8000_0000_0000_0055_u64;
    let expected_result = src & imm;
    let expected_nzcv =
        expected_logic_source_nzcv(0b0011, expected_result, OpWidth::W64, FlagUpdate::All);
    assert_sparse_logic_imm_lowering(
        "test_x_sparse_imm",
        OpKind::Test {
            src1: x(1),
            src2: SrcOperand::Imm64(imm as i64),
            width: OpWidth::W64,
        },
        1,
        src,
        None,
        0,
        expected_nzcv,
    );
}
#[test]
fn lowers_register_select_with_aliased_condition() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: x(0),
            src_true: x(1),
            src_false: x(2),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_cbz(0, 3).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
    expected.extend_from_slice(&enc_b(2).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(1, 0, 2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_register_select_w16_with_aliased_condition() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: x(0),
            src_true: x(1),
            src_false: x(2),
            width: OpWidth::W16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_cbz(0, 4).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_b(3).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(0, 0, 2).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_setcc_w8_as_cset() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::SetCC {
            dst: x(0),
            cond: Condition::Ne,
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(1, 0, 1, 31, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_x86_setcc_with_partial_register_merge() {
    let code = lower_ops(vec![
        OpKind::SetCC {
            dst: x86(X86Reg::Rbx),
            cond: Condition::Eq,
            width: OpWidth::W8,
        },
        OpKind::SetCC {
            dst: x86(X86Reg::Rdx),
            cond: Condition::Ne,
            width: OpWidth::W8,
        },
    ]);
    let regs = [
        (2, 0x2222_3333_4444_55AA),
        (3, 0xBBBB_CCCC_DDDD_EEFF),
        (16, 0x1616_1616_1616_1616),
    ];
    let (out, nzcv, sp) = run_aarch64_code(&code, &regs, 0b0100);

    assert_eq!(out[3], 0xBBBB_CCCC_DDDD_EE01);
    assert_eq!(out[2], 0x2222_3333_4444_5500);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(nzcv, 0b0100);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_cmove_x_as_csel() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(1),
            cond: Condition::Eq,
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(1, 0, 0, 1, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w_as_csel_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(1),
            cond: Condition::Eq,
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(0, 0, 0, 1, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w_imm_with_false_path_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: VReg::Imm(0x1234),
            cond: Condition::Eq,
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_b_cond(1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x1234, 0).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_x_zero_imm_as_csel_zero() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: VReg::Imm(0),
            cond: Condition::Eq,
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(1, 0, 0, 31, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_x_one_imm_as_csinc_zero() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: VReg::Imm(1),
            cond: Condition::Eq,
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(1, 0, 1, 0, 31, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w8_all_ones_imm_as_csinv_zero_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: VReg::Imm(-1),
            cond: Condition::Ne,
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(0, 1, 0, 0, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_x_always_reg_as_mov() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(1),
            cond: Condition::Always,
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w8_always_reg_as_mov_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(1),
            cond: Condition::Always,
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_x_same_reg_as_noop() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(0),
            cond: Condition::Eq,
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w_same_reg_as_self_mov_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(0),
            cond: Condition::Eq,
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w8_same_reg_as_cond_branch_ubfx() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(0),
            cond: Condition::Eq,
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    // ARM dst W8 CMove: branch over a UBFX so a false condition writes nothing
    // (the destination is fully preserved), instead of an unconditional UXTB
    // that truncated it on the false path. (#15)
    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_b_cond(1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w16_arm_dst_as_cond_branch_ubfx() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(1),
            cond: Condition::Eq,
            width: OpWidth::W16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    // ARM dst W16 CMove: branch over a UBFX (zero-extend low 16) so the false
    // path leaves the full destination unchanged. (#15)
    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_b_cond(1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmove_w8_arm_dst_as_cond_branch_ubfx() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(1),
            cond: Condition::Eq,
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    // ARM dst W8 CMove: branch over a UBFX (zero-extend low 8) so the false path
    // leaves the full destination unchanged. (#15)
    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_b_cond(1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cmc_cf_without_flagm_via_sysreg_fallback() {
    let code = lower_ops_with_flagm_features(vec![OpKind::CmcCF], false, false);
    let words = code_words(&code);
    let (imm_n, immr, imms) = Aarch64Lowerer::logical_bitmask_imm(NZCV_C, OpWidth::W32).unwrap();

    assert!(!code_has_flagm(&code, 0b000));
    assert_eq!(
        words,
        vec![
            enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31),
            enc_mrs_sysreg(16, 3, 4, 2, 0),
            enc_logical_imm(0, 0b10, imm_n, immr, imms, 16, 16),
            enc_msr_sysreg(16, 3, 4, 2, 0),
            enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31),
            0xd65f_03c0,
        ]
    );

    for nzcv in 0_u8..16 {
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(16, 0x1616_1616_1616_1616)], nzcv);
        assert_eq!(out_nzcv, nzcv ^ 0b0010, "NZCV {nzcv:#06b}");
        assert_eq!(out[16], 0x1616_1616_1616_1616, "x16 preserved");
        assert_eq!(sp, 0x8000, "stack restored");
    }
}
#[test]
fn lowers_axflag_without_flagm2_via_sysreg_fallback() {
    let code = lower_ops_with_flagm_features(axflag_ops(), true, false);

    assert!(!code_has_flagm(&code, 0b010));
    for nzcv in 0_u8..16 {
        let sentinels = [
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
        ];
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &sentinels, nzcv);
        assert_eq!(out_nzcv, expected_axflag_nzcv(nzcv), "NZCV {nzcv:#06b}");
        assert_eq!(sp, 0x8000, "stack restored");
        for (reg, value) in sentinels {
            assert_eq!(out[reg as usize], value, "x{reg} preserved");
        }
    }
}
#[test]
fn lowers_xaflag_without_flagm2_via_sysreg_fallback() {
    let code = lower_ops_with_flagm_features(xaflag_ops(), true, false);

    assert!(!code_has_flagm(&code, 0b001));
    for nzcv in 0_u8..16 {
        let sentinels = [
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
            (14, 0x1414_1414_1414_1414),
        ];
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &sentinels, nzcv);
        assert_eq!(out_nzcv, expected_xaflag_nzcv(nzcv), "NZCV {nzcv:#06b}");
        assert_eq!(sp, 0x8000, "stack restored");
        for (reg, value) in sentinels {
            assert_eq!(out[reg as usize], value, "x{reg} preserved");
        }
    }
}
#[test]
fn lowers_test_condition_cond_branch_as_b_cond() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let true_target = builder.create_block(4);
    let false_target = builder.create_block(8);
    let cond = VReg::virt(0);
    builder.push_op(
        0,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
    );
    builder.set_terminator(Terminator::CondBranch {
        cond,
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(false_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_b_cond(0, 2).to_le_bytes());
    expected.extend_from_slice(&enc_b(2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn native_exit_edge_folded_true_condition_exits_only_taken_path() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x2000);
    let source = builder.current_block();
    let true_target = builder.create_block(0x2010);
    let false_target = builder.create_block(0x2020);
    let cond = builder.alloc_vreg();
    builder.push_op(
        0x2000,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
    );
    builder.set_terminator(Terminator::CondBranch {
        cond,
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.push_op(
        0x2010,
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Imm(0x1111),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(false_target);
    builder.push_op(
        0x2020,
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Imm(0x2222),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let resume_pc = 0xaaaa_bbbb_cccc_dddd;
    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_native_exit_edges(HashMap::from([((source, true_target), resume_pc)]));
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let state_base = 0x6000;
    let state_pc = state_base + u64::from(A64_GUEST_PC_OFFSET);
    let prior_pc = 0x1357_9bdf_2468_ace0;
    let x9 = 0x0909_0909_0909_0909;
    let x0 = 0x7777_8888_9999_aaaa;

    let (taken, taken_nzcv, taken_sp, taken_pc) = run_aarch64_code_with_memory(
        &code,
        &[(0, x0), (9, x9), (28, state_base)],
        0b0100,
        state_pc,
        prior_pc,
        MemWidth::B8,
    );
    assert_eq!(taken_pc, resume_pc);
    assert_eq!(taken[0], x0, "exiting true target must not execute");
    assert_eq!(taken[9], x9);
    assert_eq!(taken_nzcv, 0b0100);
    assert_eq!(taken_sp, 0x8000);

    let (not_taken, not_taken_nzcv, not_taken_sp, not_taken_pc) = run_aarch64_code_with_memory(
        &code,
        &[(0, x0), (9, x9), (28, state_base)],
        0b1011,
        state_pc,
        prior_pc,
        MemWidth::B8,
    );
    assert_eq!(not_taken_pc, prior_pc);
    assert_eq!(not_taken[0], 0x2222, "false target must execute normally");
    assert_eq!(not_taken[9], x9);
    assert_eq!(not_taken_nzcv, 0b1011);
    assert_eq!(not_taken_sp, 0x8000);
}
#[test]
fn lowers_register_cond_branch_as_cbnz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let true_target = builder.create_block(4);
    let false_target = builder.create_block(8);
    builder.set_terminator(Terminator::CondBranch {
        cond: x(1),
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(false_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_cbnz(1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_b(2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn native_exit_edge_register_false_condition_exits_only_false_path() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x3000);
    let source = builder.current_block();
    let true_target = builder.create_block(0x3010);
    let false_target = builder.create_block(0x3020);
    builder.set_terminator(Terminator::CondBranch {
        cond: x(1),
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.push_op(
        0x3010,
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Imm(0x3333),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(false_target);
    builder.push_op(
        0x3020,
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Imm(0x4444),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let resume_pc = 0x1111_aaaa_2222_bbbb;
    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_native_exit_edges(HashMap::from([((source, false_target), resume_pc)]));
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let state_base = 0x6000;
    let state_pc = state_base + u64::from(A64_GUEST_PC_OFFSET);
    let prior_pc = 0x5555_6666_7777_8888;
    let x9 = 0x0909_0909_0909_0909;
    let x0 = 0x9999_aaaa_bbbb_cccc;
    let old_nzcv = 0b1101;

    let (taken, taken_nzcv, taken_sp, taken_pc) = run_aarch64_code_with_memory(
        &code,
        &[(0, x0), (1, 1), (9, x9), (28, state_base)],
        old_nzcv,
        state_pc,
        prior_pc,
        MemWidth::B8,
    );
    assert_eq!(taken_pc, prior_pc);
    assert_eq!(taken[0], 0x3333, "true target must execute normally");
    assert_eq!(taken[9], x9);
    assert_eq!(taken_nzcv, old_nzcv);
    assert_eq!(taken_sp, 0x8000);

    let (not_taken, not_taken_nzcv, not_taken_sp, not_taken_pc) = run_aarch64_code_with_memory(
        &code,
        &[(0, x0), (1, 0), (9, x9), (28, state_base)],
        old_nzcv,
        state_pc,
        prior_pc,
        MemWidth::B8,
    );
    assert_eq!(not_taken_pc, resume_pc);
    assert_eq!(not_taken[0], x0, "exiting false target must not execute");
    assert_eq!(not_taken[9], x9);
    assert_eq!(not_taken_nzcv, old_nzcv);
    assert_eq!(not_taken_sp, 0x8000);
}
#[test]
fn lowers_immediate_cond_branch_as_single_b() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let true_target = builder.create_block(4);
    let false_target = builder.create_block(8);
    builder.set_terminator(Terminator::CondBranch {
        cond: VReg::Imm(0),
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.switch_to_block(false_target);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_b(2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_materialize_flags_as_noop() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(0, OpKind::MaterializeFlags);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_nzcv_read_with_masked_mask_imm() {
    let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));
    let masked = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::And {
            dst: masked,
            src1: nzcv,
            src2: SrcOperand::Imm64(0x1_f000_0000),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Reg(masked),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mrs_sysreg(0, 3, 4, 2, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_nzcv_write_with_masked_mask_imm() {
    let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));
    let masked = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::And {
            dst: masked,
            src1: x(1),
            src2: SrcOperand::Imm64(0x1_f000_0000),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Mov {
            dst: nzcv,
            src: SrcOperand::Reg(masked),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_msr_sysreg(1, 3, 4, 2, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_read_sysreg_nzcv_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::ReadSysReg {
            dst: x(0),
            reg: SYSREG_NZCV,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mrs_sysreg(0, 3, 4, 2, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_write_sysreg_nzcv_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::WriteSysReg {
            reg: SYSREG_NZCV,
            src: x(1),
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_msr_sysreg(1, 3, 4, 2, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_write_sysreg_nzcv_imm_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::WriteSysReg {
            reg: SYSREG_NZCV,
            src: VReg::Imm(NZCV_N | NZCV_C),
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 16).to_le_bytes());
    expected.extend_from_slice(&enc_mov_wide(0, 0b11, 1, 0xa000, 16).to_le_bytes());
    expected.extend_from_slice(&enc_msr_sysreg(16, 3, 4, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_named_sysreg_nzcv_imm_runtime() {
    let code = lower_single_op(OpKind::Mov {
        dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
        src: SrcOperand::Imm(NZCV_Z | NZCV_V),
        width: OpWidth::W32,
    });

    let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(16, 0x1616_1616_1616_1616)], 0b1010);

    assert_eq!(out_nzcv, 0b0101);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_rcl_rcr_immediate_counts_with_exact_flags() {
    assert_rotate_carry_lowering(
        "rcl_w8_imm1",
        OpKind::Rcl {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8,
            flags: rotate_flags(),
        },
        0x42,
        1,
        0b1010,
        OpWidth::W8,
        rotate_flags(),
        false,
        0,
        None,
    );

    assert_rotate_carry_lowering(
        "rcr_w8_full_period_preserves_flags",
        OpKind::Rcr {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(9),
            width: OpWidth::W8,
            flags: rotate_flags(),
        },
        0xa5,
        9,
        0b0111,
        OpWidth::W8,
        rotate_flags(),
        true,
        0,
        None,
    );

    assert_rotate_carry_lowering(
        "rcl_x_imm32_deterministic_undefined_of",
        OpKind::Rcl {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(32),
            width: OpWidth::W64,
            flags: rotate_flags(),
        },
        0x1234_5678_9abc_def0,
        32,
        0b0100,
        OpWidth::W64,
        rotate_flags(),
        false,
        0,
        None,
    );
}
// Regression for issue #36: RCR-by-1 OF is the XOR of the two MOST-significant
// bits of the result. A lowering that XORed the MSB with the (out-of-range) bit
// ABOVE it would collapse to just the MSB and diverge here — e.g. src=0x80,
// CF=0 rotates to 0x40, whose two MSBs (0,1) differ, so OF MUST be 1 (the
// alleged bug would yield 0). The helper compares the executed NZCV against the
// x86 reference, so a wrong OF fails. (#36)
#[test]
fn rcr_count1_overflow_flag_matches_reference() {
    for &(src, nzcv) in &[
        (0x80u64, 0b0000u8),
        (0x40, 0b0010),
        (0x01, 0b0000),
        (0xff, 0b0010),
    ] {
        assert_rotate_carry_lowering(
            "rcr_w8_imm1_of",
            OpKind::Rcr {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W8,
                flags: rotate_flags(),
            },
            src,
            1,
            nzcv,
            OpWidth::W8,
            rotate_flags(),
            true,
            0,
            None,
        );
    }

    assert_rotate_carry_lowering(
        "rcr_x_imm1_of",
        OpKind::Rcr {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: rotate_flags(),
        },
        0x8000_0000_0000_0000,
        1,
        0b0000,
        OpWidth::W64,
        rotate_flags(),
        true,
        0,
        None,
    );
}
#[test]
fn lowers_x86_count_architectural_flag_contracts() {
    let defined_count = FlagSet::CF.union(FlagSet::ZF);
    let cases = [
        (
            "popcnt-zero-all-flags",
            X86CountKind::Popcnt,
            OpWidth::W32,
            FlagUpdate::All,
            0_u64,
            0b1111,
        ),
        (
            "popcnt-nonzero-all-flags",
            X86CountKind::Popcnt,
            OpWidth::W64,
            FlagUpdate::All,
            0xf0,
            0b1111,
        ),
        (
            "tzcnt-zero-cf-zf",
            X86CountKind::Tzcnt,
            OpWidth::W32,
            FlagUpdate::Specific(defined_count),
            0,
            0b1001,
        ),
        (
            "tzcnt-w16-alias-result-zero",
            X86CountKind::Tzcnt,
            OpWidth::W16,
            FlagUpdate::Specific(defined_count),
            0xaaaa_bbbb_cccc_0001,
            0b1001,
        ),
        (
            "lzcnt-high-bit-cf-zf",
            X86CountKind::Lzcnt,
            OpWidth::W64,
            FlagUpdate::Specific(defined_count),
            1 << 63,
            0b1011,
        ),
        (
            "lzcnt-zero-zf-only",
            X86CountKind::Lzcnt,
            OpWidth::W64,
            FlagUpdate::Specific(FlagSet::ZF),
            0,
            0b1011,
        ),
    ];
    let sentinels = [
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
        (15, 0x1515_1515_1515_1515),
        (14, 0x1414_1414_1414_1414),
        (13, 0x1313_1313_1313_1313),
    ];

    for (label, kind, width, flags, value, old_nzcv) in cases {
        let code = lower_single_op(OpKind::X86Count {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rax),
            width,
            kind,
            flags,
        });
        let masked = value & width.mask();
        let result = match kind {
            X86CountKind::Popcnt => u64::from(masked.count_ones()),
            X86CountKind::Tzcnt => {
                if masked == 0 {
                    u64::from(width.bits())
                } else {
                    u64::from(masked.trailing_zeros())
                }
            }
            X86CountKind::Lzcnt => u64::from(masked.leading_zeros() - (64 - width.bits())),
        };
        let expected = if width == OpWidth::W16 {
            (value & !0xffff) | result
        } else {
            result
        };
        let produced = match kind {
            X86CountKind::Popcnt => ((masked == 0) as u8) << 2,
            X86CountKind::Tzcnt | X86CountKind::Lzcnt => {
                (((result == 0) as u8) << 2) | (((masked == 0) as u8) << 1)
            }
        };
        let requested = flags.as_set();
        let mut mask = 0_u8;
        if requested.contains(FlagSet::SF) {
            mask |= 0b1000;
        }
        if requested.contains(FlagSet::ZF) {
            mask |= 0b0100;
        }
        if requested.contains(FlagSet::CF) {
            mask |= 0b0010;
        }
        if requested.contains(FlagSet::OF) {
            mask |= 0b0001;
        }
        let expected_nzcv = (old_nzcv & !mask) | (produced & mask);
        let mut regs = sentinels.to_vec();
        regs.push((0, value));
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

        assert_eq!(out[0], expected, "{label}: result");
        assert_eq!(out_nzcv, expected_nzcv, "{label}: NZCV");
        for (index, sentinel) in sentinels {
            assert_eq!(out[index as usize], sentinel, "{label}: x{index} scratch");
        }
        assert_eq!(sp, 0x8000, "{label}: stack");
    }
}
#[test]
fn rejects_setcc_parity_condition_lowering() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::SetCC {
            dst: x(0),
            cond: Condition::Parity,
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn rejects_setcc_w128_lowering() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::SetCC {
            dst: x(0),
            cond: Condition::Ne,
            width: OpWidth::W128,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn rejects_cmove_parity_condition_lowering() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::CMove {
            dst: x(0),
            src: x(1),
            cond: Condition::Parity,
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_flag_setting_full_width_logical_runtime() {
    assert_subword_logic_flags_lowering(
        "or_x_reg_sets_negative",
        0b01,
        false,
        x(0),
        1,
        SrcOperand::Reg(x(2)),
        0x8000_0000_0000_0000,
        0x0000_0000_0000_0001,
        OpWidth::W64,
        0b0111,
    );
    assert_subword_logic_flags_lowering(
        "xor_w_imm_sets_zero",
        0b10,
        false,
        x(0),
        1,
        SrcOperand::Imm(-1),
        0xffff_ffff,
        0xffff_ffff,
        OpWidth::W32,
        0b1011,
    );
    assert_subword_logic_flags_lowering(
        "or_x_virtual_dst_sets_zero",
        0b01,
        false,
        VReg::virt(0),
        1,
        SrcOperand::Imm(0),
        0,
        0,
        OpWidth::W64,
        0b1011,
    );
    assert_subword_logic_flags_lowering(
        "xor_x_dst_aliases_src1",
        0b10,
        false,
        x(1),
        1,
        SrcOperand::Reg(x(2)),
        0x1234_5678_9abc_def0,
        0x1234_5678_9abc_def0,
        OpWidth::W64,
        0b1111,
    );
    assert_subword_logic_flags_lowering(
        "or_x_sparse_imm_sets_negative",
        0b01,
        false,
        x(0),
        1,
        SrcOperand::Imm64(0x0001_0000_0000_0001),
        0x8000_0000_0000_0000,
        0x0001_0000_0000_0001,
        OpWidth::W64,
        0b0101,
    );
}
