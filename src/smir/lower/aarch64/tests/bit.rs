//! tests::bit tests

use super::*;
use crate::smir::lower::aarch64::*;

    // Regression for issue #11: the BFIZ/UBFIZ fusion (Bfx{lsb:0} + Shl) must not
    // fire when the Bfx destination is an architectural register — that would drop
    // the guest-visible Bfx write. With x2 architectural, both x2 and x0 must update.
    #[test]
    fn issue_11_bfiz_fusion_preserves_arch_bfx_write() {
        let code = lower_ops(vec![
            OpKind::Bfx {
                dst: x(2),
                src: x(1),
                lsb: 0,
                width_bits: 8,
                sign_extend: false,
                op_width: OpWidth::W64,
            },
            OpKind::Shl {
                dst: x(0),
                src: x(2),
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ]);
        let (regs, _, _) = run_aarch64_code(&code, &[(1, 0x123), (2, 0xDEAD)], 0);
        assert_eq!(
            regs[2], 0x23,
            "Bfx must write x2 (UBFIZ fusion must not drop it)"
        );
        assert_eq!(regs[0], 0x230, "final Shl result");
    }
    // Regression for issue #31: a W8/W16 CWD (sign-mask broadcast) into an x86
    // destination is a PARTIAL write — only the low 8/16 bits receive the sign mask
    // and the upper bits are preserved. The previous lowering wrote the whole
    // register, zeroing the preserved bits.
    #[test]
    fn issue_31_cwd_subword_x86_dst_merges_low_bits_preserves_upper() {
        let dst = x86(X86Reg::Rax);
        let src = x86(X86Reg::Rcx);
        let hd = Aarch64Lowerer::gpr_arm_or_x86(dst).unwrap();
        let hs = Aarch64Lowerer::gpr_arm_or_x86(src).unwrap();
        assert_ne!(hd, hs);

        let code8 = lower_single_op(OpKind::Cwd {
            dst,
            src,
            width: OpWidth::W8,
        });
        // Negative low byte (bit 7 set) -> sign mask 0xff; upper bits preserved.
        let (regs, _, _) = run_aarch64_code(&code8, &[(hd, 0xDEAD_BEEF_0000_1234), (hs, 0x80)], 0);
        assert_eq!(
            regs[hd as usize], 0xDEAD_BEEF_0000_12FF,
            "W8 CWD merges 0xff into the low byte, preserving the upper bits",
        );
        // Non-negative low byte -> sign mask 0x00; upper bits preserved.
        let (regs, _, _) = run_aarch64_code(&code8, &[(hd, 0xDEAD_BEEF_0000_1234), (hs, 0x7F)], 0);
        assert_eq!(
            regs[hd as usize], 0xDEAD_BEEF_0000_1200,
            "W8 CWD merges 0x00 into the low byte, preserving the upper bits",
        );

        // W16: sign mask fills the low 16 bits only.
        let code16 = lower_single_op(OpKind::Cwd {
            dst,
            src,
            width: OpWidth::W16,
        });
        let (regs, _, _) =
            run_aarch64_code(&code16, &[(hd, 0xDEAD_BEEF_0000_1234), (hs, 0x8000)], 0);
        assert_eq!(
            regs[hd as usize], 0xDEAD_BEEF_0000_FFFF,
            "W16 CWD merges 0xffff into the low 16 bits, preserving the upper bits",
        );
    }
    #[test]
    fn lowers_x86_subword_mov_with_partial_register_merge() {
        let code = lower_ops(vec![
            OpKind::Mov {
                dst: x86(X86Reg::Rax),
                src: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
            },
            OpKind::Mov {
                dst: x86(X86Reg::Rdx),
                src: SrcOperand::Imm(0xab),
                width: OpWidth::W8,
            },
            OpKind::Mov {
                dst: x86(X86Reg::R8),
                src: SrcOperand::Imm64(-2),
                width: OpWidth::W16,
            },
        ]);
        let regs = [
            (0, 0xAAAA_BBBB_CCCC_DDDD),
            (1, 0x1111_2222_3333_5678),
            (2, 0xDEAD_BEEF_1234_5600),
            (8, 0x8888_7777_6666_5555),
            (16, 0x1616_1616_1616_1616),
        ];
        let (out, nzcv, sp) = run_aarch64_code(&code, &regs, 0b1010);

        assert_eq!(out[0], 0xAAAA_BBBB_CCCC_5678);
        assert_eq!(out[1], 0x1111_2222_3333_5678);
        assert_eq!(out[2], 0xDEAD_BEEF_1234_56AB);
        assert_eq!(out[8], 0x8888_7777_6666_FFFE);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(nzcv, 0b1010);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_x86_subword_not_and_xchg_with_partial_register_merges() {
        let code = lower_ops(vec![
            OpKind::Not {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rax),
                width: OpWidth::W16,
            },
            OpKind::Not {
                dst: x86(X86Reg::Rdx),
                src: x86(X86Reg::Rdx),
                width: OpWidth::W8,
            },
            OpKind::Xchg {
                reg1: x86(X86Reg::Rsi),
                reg2: x86(X86Reg::Rdi),
                width: OpWidth::W16,
            },
        ]);
        let regs = [
            (0, 0xAAAA_BBBB_CCCC_00F0),
            (2, 0xDEAD_BEEF_1234_56A5),
            (6, 0x6666_7777_8888_9999),
            (7, 0x1111_2222_3333_4444),
            (16, 0x1616_1616_1616_1616),
        ];
        let (out, nzcv, sp) = run_aarch64_code(&code, &regs, 0b0110);

        assert_eq!(out[0], 0xAAAA_BBBB_CCCC_FF0F);
        assert_eq!(out[2], 0xDEAD_BEEF_1234_565A);
        assert_eq!(out[6], 0x6666_7777_8888_4444);
        assert_eq!(out[7], 0x1111_2222_3333_9999);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(nzcv, 0b0110);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_x86_subword_integer_alu_with_partial_register_merges() {
        let code = lower_ops(vec![
            OpKind::Add {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Sub {
                dst: x86(X86Reg::Rdx),
                src1: x86(X86Reg::Rdx),
                src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::Adc {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::R8),
                src2: SrcOperand::Reg(x86(X86Reg::Rdi)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Neg {
                dst: x86(X86Reg::R9),
                src: x86(X86Reg::R9),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::Inc {
                dst: x86(X86Reg::R10),
                src: x86(X86Reg::R10),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Dec {
                dst: x86(X86Reg::R11),
                src: x86(X86Reg::R11),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::And {
                dst: x86(X86Reg::R12),
                src1: x86(X86Reg::R12),
                src2: SrcOperand::Reg(x86(X86Reg::R15)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            OpKind::Or {
                dst: x86(X86Reg::R13),
                src1: x86(X86Reg::R13),
                src2: SrcOperand::Reg(x86(X86Reg::R15)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            OpKind::Xor {
                dst: x86(X86Reg::R14),
                src1: x86(X86Reg::R14),
                src2: SrcOperand::Reg(x86(X86Reg::R15)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ]);
        let regs = [
            (0, 0xAAAA_BBBB_CCCC_00FF),
            (1, 0x1111_2222_3333_0001),
            (2, 0xDEAD_BEEF_1234_56F0),
            (3, 0xBBBB_CCCC_DDDD_EE20),
            (7, 0x7777_6666_5555_0000),
            (8, 0x8888_7777_6666_FFFF),
            (9, 0x9999_8888_7777_6601),
            (10, 0xAAAA_9999_8888_FFFF),
            (11, 0xBBBB_AAAA_9999_8800),
            (12, 0xCCCC_BBBB_AAAA_F0F0),
            (13, 0xDDDD_CCCC_BBBB_AA0F),
            (14, 0xEEEE_DDDD_CCCC_AAAA),
            (15, 0xFFFF_EEEE_DDDD_0FF0),
            (16, 0x1616_1616_1616_1616),
        ];
        let (out, nzcv, sp) = run_aarch64_code(&code, &regs, 0b0110);

        assert_eq!(out[0], 0xAAAA_BBBB_CCCC_0100);
        assert_eq!(out[2], 0xDEAD_BEEF_1234_56D0);
        assert_eq!(out[8], 0x8888_7777_6666_0000);
        assert_eq!(out[9], 0x9999_8888_7777_66FF);
        assert_eq!(out[10], 0xAAAA_9999_8888_0000);
        assert_eq!(out[11], 0xBBBB_AAAA_9999_88FF);
        assert_eq!(out[12], 0xCCCC_BBBB_AAAA_00F0);
        assert_eq!(out[13], 0xDDDD_CCCC_BBBB_AAFF);
        assert_eq!(out[14], 0xEEEE_DDDD_CCCC_A55A);
        assert_eq!(out[15], 0xFFFF_EEEE_DDDD_0FF0);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(nzcv, 0b0110, "flag-free ALU forms preserve NZCV");
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_x86_subword_integer_alu_complete_width_matrix() {
        const UPPER: u64 = 0xAAAA_BBBB_CCCC_5A00;
        const SRC_UPPER: u64 = 0x1111_2222_3333_A500;
        const SCRATCH: u64 = 0x1616_1616_1616_1616;

        for width in [OpWidth::W8, OpWidth::W16] {
            let mask = width.mask();
            let cases = vec![
                (
                    "add",
                    OpKind::Add {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                        width,
                        flags: FlagUpdate::None,
                    },
                    0x7f,
                    1,
                    0,
                    0x80,
                ),
                (
                    "sub",
                    OpKind::Sub {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                        width,
                        flags: FlagUpdate::None,
                    },
                    0,
                    1,
                    0,
                    mask,
                ),
                (
                    "adc",
                    OpKind::Adc {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                        width,
                        flags: FlagUpdate::None,
                    },
                    mask,
                    0,
                    0b0010,
                    0,
                ),
                (
                    "neg",
                    OpKind::Neg {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        width,
                        flags: FlagUpdate::None,
                    },
                    1,
                    0,
                    0,
                    mask,
                ),
                (
                    "inc",
                    OpKind::Inc {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        width,
                        flags: FlagUpdate::None,
                    },
                    mask,
                    0,
                    0b0110,
                    0,
                ),
                (
                    "dec",
                    OpKind::Dec {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        width,
                        flags: FlagUpdate::None,
                    },
                    0,
                    0,
                    0b1011,
                    mask,
                ),
                (
                    "and",
                    OpKind::And {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                        width,
                        flags: FlagUpdate::None,
                    },
                    0xF0F0,
                    0x0FF0,
                    0b0110,
                    0x00F0,
                ),
                (
                    "or",
                    OpKind::Or {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                        width,
                        flags: FlagUpdate::None,
                    },
                    0x000F,
                    0x00F0,
                    0b1001,
                    0x00FF,
                ),
                (
                    "xor",
                    OpKind::Xor {
                        dst: x86(X86Reg::Rax),
                        src1: x86(X86Reg::Rax),
                        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                        width,
                        flags: FlagUpdate::None,
                    },
                    0xAAAA,
                    0x0FF0,
                    0b0101,
                    0xA55A,
                ),
            ];

            for (name, op, dst_low, src_low, initial_nzcv, expected_low) in cases {
                let code = lower_ops(vec![op]);
                let dst = (UPPER & !mask) | (dst_low & mask);
                let src = (SRC_UPPER & !mask) | (src_low & mask);
                let (out, nzcv, sp) =
                    run_aarch64_code(&code, &[(0, dst), (1, src), (16, SCRATCH)], initial_nzcv);
                assert_eq!(
                    out[0],
                    (dst & !mask) | (expected_low & mask),
                    "{name} {width:?} result"
                );
                assert_eq!(out[1], src, "{name} {width:?} source");
                assert_eq!(out[16], SCRATCH, "{name} {width:?} scratch");
                assert_eq!(nzcv, initial_nzcv, "{name} {width:?} flags");
                assert_eq!(sp, 0x8000, "{name} {width:?} stack");
            }
        }
    }
    #[test]
    fn lowers_x86_subword_shift_rotate_partial_write_matrix() {
        const UPPER: u64 = 0xAAAA_BBBB_CCCC_0000;
        const SCRATCH16: u64 = 0x1616_1616_1616_1616;
        const SCRATCH17: u64 = 0x1717_1717_1717_1717;

        for width in [OpWidth::W8, OpWidth::W16] {
            let source_low = if width == OpWidth::W8 { 0x95 } else { 0x8123 };
            let source = (UPPER & !width.mask()) | source_low;
            for (name, shift) in [
                ("shl", ShiftOp::Lsl),
                ("shr", ShiftOp::Lsr),
                ("sar", ShiftOp::Asr),
                ("ror", ShiftOp::Ror),
            ] {
                for register_count in [false, true] {
                    let amount = if register_count {
                        SrcOperand::Reg(x86(X86Reg::Rcx))
                    } else {
                        SrcOperand::Imm(3)
                    };
                    let op = match shift {
                        ShiftOp::Lsl => OpKind::Shl {
                            dst: x86(X86Reg::Rax),
                            src: x86(X86Reg::Rax),
                            amount,
                            width,
                            flags: FlagUpdate::None,
                        },
                        ShiftOp::Lsr => OpKind::Shr {
                            dst: x86(X86Reg::Rax),
                            src: x86(X86Reg::Rax),
                            amount,
                            width,
                            flags: FlagUpdate::None,
                        },
                        ShiftOp::Asr => OpKind::Sar {
                            dst: x86(X86Reg::Rax),
                            src: x86(X86Reg::Rax),
                            amount,
                            width,
                            flags: FlagUpdate::None,
                        },
                        ShiftOp::Ror => OpKind::Ror {
                            dst: x86(X86Reg::Rax),
                            src: x86(X86Reg::Rax),
                            amount,
                            width,
                            flags: FlagUpdate::None,
                        },
                        ShiftOp::Rrx => unreachable!(),
                    };
                    let code = lower_ops(vec![op]);
                    let expected_low = ref_shift_reg(source, 3, shift, width);
                    let (out, nzcv, sp) = run_aarch64_code(
                        &code,
                        &[(0, source), (1, 3), (16, SCRATCH16), (17, SCRATCH17)],
                        0b1011,
                    );

                    assert_eq!(
                        out[0],
                        (source & !width.mask()) | expected_low,
                        "{name} {width:?} register_count={register_count} result"
                    );
                    assert_eq!(out[1], 3, "{name} {width:?} count");
                    assert_eq!(out[16], SCRATCH16, "{name} {width:?} x16 scratch");
                    assert_eq!(out[17], SCRATCH17, "{name} {width:?} x17 scratch");
                    assert_eq!(nzcv, 0b1011, "{name} {width:?} flags");
                    assert_eq!(sp, 0x8000, "{name} {width:?} stack");
                }
            }

            for register_count in [false, true] {
                let amount = if register_count {
                    SrcOperand::Reg(x86(X86Reg::Rcx))
                } else {
                    SrcOperand::Imm(3)
                };
                let code = lower_ops(vec![OpKind::Rol {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount,
                    width,
                    flags: FlagUpdate::None,
                }]);
                let expected_low = ref_rol_reg(source, 3, width);
                let (out, nzcv, sp) = run_aarch64_code(
                    &code,
                    &[(0, source), (1, 3), (16, SCRATCH16), (17, SCRATCH17)],
                    0b0110,
                );

                assert_eq!(
                    out[0],
                    (source & !width.mask()) | expected_low,
                    "rol {width:?} register_count={register_count} result"
                );
                assert_eq!(out[1], 3, "rol {width:?} count");
                assert_eq!(out[16], SCRATCH16, "rol {width:?} x16 scratch");
                assert_eq!(out[17], SCRATCH17, "rol {width:?} x17 scratch");
                assert_eq!(nzcv, 0b0110, "rol {width:?} flags");
                assert_eq!(sp, 0x8000, "rol {width:?} stack");
            }
        }
    }
    #[test]
    fn lowers_x86_subword_shift_rotate_flags_before_partial_merge() {
        const UPPER: u64 = 0xAAAA_BBBB_CCCC_0000;
        const SCRATCH16: u64 = 0x1616_1616_1616_1616;
        const SCRATCH17: u64 = 0x1717_1717_1717_1717;

        for width in [OpWidth::W8, OpWidth::W16] {
            let source_low = 1_u64 << (width.bits() - 1);
            let source = (UPPER & !width.mask()) | source_low;
            for (name, shift) in [
                ("shl", ShiftOp::Lsl),
                ("shr", ShiftOp::Lsr),
                ("sar", ShiftOp::Asr),
            ] {
                let op = match shift {
                    ShiftOp::Lsl => OpKind::Shl {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: SrcOperand::Imm(1),
                        width,
                        flags: FlagUpdate::All,
                    },
                    ShiftOp::Lsr => OpKind::Shr {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: SrcOperand::Imm(1),
                        width,
                        flags: FlagUpdate::All,
                    },
                    ShiftOp::Asr => OpKind::Sar {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount: SrcOperand::Imm(1),
                        width,
                        flags: FlagUpdate::All,
                    },
                    ShiftOp::Ror | ShiftOp::Rrx => unreachable!(),
                };
                let code = lower_ops(vec![op]);
                let expected_low = ref_shift_reg(source, 1, shift, width);
                let expected_nzcv =
                    expected_shift_nzcv(0b1011, source, 1, shift, width, FlagUpdate::All);
                let (out, nzcv, sp) = run_aarch64_code(
                    &code,
                    &[(0, source), (16, SCRATCH16), (17, SCRATCH17)],
                    0b1011,
                );

                assert_eq!(
                    out[0],
                    (source & !width.mask()) | expected_low,
                    "{name} {width:?} result"
                );
                assert_eq!(nzcv, expected_nzcv, "{name} {width:?} flags");
                assert_eq!(out[16], SCRATCH16, "{name} {width:?} x16 scratch");
                assert_eq!(out[17], SCRATCH17, "{name} {width:?} x17 scratch");
                assert_eq!(sp, 0x8000, "{name} {width:?} stack");
            }

            let rotate_flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
            for (right, register_count) in
                [(false, false), (false, true), (true, false), (true, true)]
            {
                let amount = if register_count {
                    SrcOperand::Reg(x86(X86Reg::Rcx))
                } else {
                    SrcOperand::Imm(1)
                };
                let op = if right {
                    OpKind::Ror {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount,
                        width,
                        flags: rotate_flags,
                    }
                } else {
                    OpKind::Rol {
                        dst: x86(X86Reg::Rax),
                        src: x86(X86Reg::Rax),
                        amount,
                        width,
                        flags: rotate_flags,
                    }
                };
                let code = lower_ops(vec![op]);
                let expected_low = if right {
                    ref_ror_reg(source, 1, width)
                } else {
                    ref_rol_reg(source, 1, width)
                };
                let expected_nzcv =
                    expected_rotate_nzcv(0b1100, expected_low, 1, width, rotate_flags, right);
                let (out, nzcv, sp) = run_aarch64_code(
                    &code,
                    &[(0, source), (1, 1), (16, SCRATCH16), (17, SCRATCH17)],
                    0b1100,
                );

                assert_eq!(
                    out[0],
                    (source & !width.mask()) | expected_low,
                    "{} {width:?} register_count={register_count} result",
                    if right { "ror" } else { "rol" }
                );
                assert_eq!(
                    nzcv,
                    expected_nzcv,
                    "{} {width:?} register_count={register_count} flags",
                    if right { "ror" } else { "rol" }
                );
                assert_eq!(out[1], 1, "rotate {width:?} count");
                assert_eq!(out[16], SCRATCH16, "rotate {width:?} x16 scratch");
                assert_eq!(out[17], SCRATCH17, "rotate {width:?} x17 scratch");
                assert_eq!(sp, 0x8000, "rotate {width:?} stack");
            }
        }
    }
    #[test]
    fn lowers_x86_subword_integer_alu_flags_before_partial_register_merge() {
        let add = lower_ops(vec![OpKind::Add {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        }]);
        let (out, nzcv, sp) = run_aarch64_code(
            &add,
            &[(0, 0xAAAA_BBBB_CCCC_DD7F), (16, 0x1616_1616_1616_1616)],
            0,
        );
        assert_eq!(out[0], 0xAAAA_BBBB_CCCC_DD80);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(nzcv, 0b1001, "W8 0x7f + 1 sets N/V");
        assert_eq!(sp, 0x8000);

        let adc = lower_ops(vec![OpKind::Adc {
            dst: x86(X86Reg::R8),
            src1: x86(X86Reg::R8),
            src2: SrcOperand::Imm(0),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        }]);
        let (out, nzcv, sp) = run_aarch64_code(
            &adc,
            &[(8, 0x8888_7777_6666_FFFF), (16, 0x1616_1616_1616_1616)],
            0b0010,
        );
        assert_eq!(out[8], 0x8888_7777_6666_0000);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(nzcv, 0b0110, "W16 0xffff + carry sets Z/C");
        assert_eq!(sp, 0x8000);

        let inc = lower_ops(vec![OpKind::Inc {
            dst: x86(X86Reg::R9),
            src: x86(X86Reg::R9),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        }]);
        let (out, nzcv, sp) = run_aarch64_code(
            &inc,
            &[(9, 0x9999_8888_7777_667F), (16, 0x1616_1616_1616_1616)],
            0b0010,
        );
        assert_eq!(out[9], 0x9999_8888_7777_6680);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(nzcv, 0b1011, "INC preserves C and sets N/V");
        assert_eq!(sp, 0x8000);

        let xor = lower_ops(vec![OpKind::Xor {
            dst: x86(X86Reg::R10),
            src1: x86(X86Reg::R10),
            src2: SrcOperand::Reg(x86(X86Reg::R10)),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        }]);
        let (out, nzcv, sp) = run_aarch64_code(
            &xor,
            &[(10, 0xAAAA_9999_8888_7777), (16, 0x1616_1616_1616_1616)],
            0b1011,
        );
        assert_eq!(out[10], 0xAAAA_9999_8888_0000);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(nzcv, 0b0100, "logical zero sets only Z");
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_subword_zero_base_addsub_register_sources() {
        let cases = [
            (
                OpKind::Sub {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Reg(x(1)),
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                vec![
                    enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 31, 1),
                    enc_bitfield_regs(0, 0b10, 0, 7, 0, 0),
                ],
            ),
            (
                OpKind::Add {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Shifted {
                        reg: x(1),
                        shift: ShiftOp::Lsl,
                        amount: 3,
                    },
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                },
                vec![
                    enc_addsub_shift_regs(1, 0, 0, 0, 3, 0, 31, 1),
                    enc_bitfield_regs(0, 0b10, 0, 15, 0, 0),
                ],
            ),
            (
                OpKind::Sub {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Shifted {
                        reg: x(1),
                        shift: ShiftOp::Asr,
                        amount: 40,
                    },
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                vec![
                    enc_addsub_shift_regs(1, 1, 0, 2, 40, 0, 31, 1),
                    enc_bitfield_regs(0, 0b10, 0, 7, 0, 0),
                ],
            ),
            (
                OpKind::Sub {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(1),
                        extend: ExtendOp::Uxtb,
                        shift: 1,
                    },
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                // zero-base extended Sub: UBFIZ x0, x1, #1, #8 (extend+shift,
                // no SP), then NEG via XZR-based sub, then truncate to W8.
                vec![
                    enc_bitfield_regs(1, 0b10, 63, 7, 1, 0),
                    enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 0),
                    enc_bitfield_regs(0, 0b10, 0, 7, 0, 0),
                ],
            ),
        ];

        for (kind, words) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_subword_addsub_shifted_and_extended_sources() {
        let cases = [
            (
                OpKind::Add {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Shifted {
                        reg: x(2),
                        shift: ShiftOp::Lsl,
                        amount: 2,
                    },
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                enc_addsub_shift_regs(1, 0, 0, 0, 2, 0, 1, 2),
                enc_bitfield_regs(0, 0b10, 0, 7, 0, 0),
            ),
            (
                OpKind::Add {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Shifted {
                        reg: x(2),
                        shift: ShiftOp::Lsr,
                        amount: 40,
                    },
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                enc_addsub_shift_regs(1, 0, 0, 1, 40, 0, 1, 2),
                enc_bitfield_regs(0, 0b10, 0, 7, 0, 0),
            ),
            (
                OpKind::Sub {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxth,
                        shift: 1,
                    },
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                },
                enc_addsub_ext_regs(1, 1, 0, 0b101, 1, 0, 1, 2),
                enc_bitfield_regs(0, 0b10, 0, 15, 0, 0),
            ),
        ];

        for (kind, first, trunc) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&first.to_le_bytes());
            expected.extend_from_slice(&trunc.to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_subword_addsub_ror_sources() {
        let cases = [
            (
                OpKind::Sub {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Shifted {
                        reg: x(1),
                        shift: ShiftOp::Ror,
                        amount: 13,
                    },
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                enc_extract(1, 1, 1, 13),
                enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 0),
                enc_bitfield_regs(0, 0b10, 0, 7, 0, 0),
            ),
            (
                OpKind::Add {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Shifted {
                        reg: x(2),
                        shift: ShiftOp::Ror,
                        amount: 52,
                    },
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                },
                enc_extract(1, 2, 2, 52),
                enc_addsub_shift_regs(1, 0, 0, 0, 0, 0, 1, 0),
                enc_bitfield_regs(0, 0b10, 0, 15, 0, 0),
            ),
        ];

        for (kind, rotate, addsub, trunc) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&rotate.to_le_bytes());
            expected.extend_from_slice(&addsub.to_le_bytes());
            expected.extend_from_slice(&trunc.to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_subword_addsub_effective_zero_ror_sources_as_register_sources() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 64,
                },
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 0, 0, 1, 1, 2).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn executes_divs_w_imm_power_of_two_in_place_positive_high_bit() {
        assert_div_runtime_lowering(
            "divs_w_imm_power_of_two_in_place_positive_high_bit",
            true,
            1,
            None,
            1,
            SrcOperand::Imm(16),
            None,
            0x7fff_ffff,
            16,
            OpWidth::W32,
        );
    }
    #[test]
    fn lowers_subword_div_runtime() {
        assert_div_runtime_lowering(
            "divu_w8_reg_with_remainder",
            false,
            0,
            Some(3),
            1,
            SrcOperand::Reg(x(2)),
            Some(2),
            0xaa55_aa55_aa55_00f3,
            10,
            OpWidth::W8,
        );
        assert_div_runtime_lowering(
            "divs_w16_general_imm_with_remainder",
            true,
            0,
            Some(3),
            1,
            SrcOperand::Imm(7),
            None,
            0xffff_ff85,
            7,
            OpWidth::W16,
        );
    }
    #[test]
    fn lowers_rol_w16_imm_as_subword_ror() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Rol {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Imm(5),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 16, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 11, 26, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_subword_shift_reg_when_dst_is_count() {
        assert_shift_reg_count_alias_lowering(
            "shr_w16_dst_aliases_count",
            ShiftOp::Lsr,
            1,
            0xf0f0,
            2,
            4,
            OpWidth::W16,
            2,
        );
        assert_shift_reg_count_alias_lowering(
            "shr_w8_dst_aliases_count_oob_zero",
            ShiftOp::Lsr,
            1,
            0xff,
            2,
            8,
            OpWidth::W8,
            2,
        );
        assert_shift_reg_count_alias_lowering(
            "sar_w8_dst_aliases_count_sign_fill",
            ShiftOp::Asr,
            1,
            0xf0,
            2,
            3,
            OpWidth::W8,
            2,
        );
        assert_shift_reg_count_alias_lowering(
            "sar_w16_dst_aliases_count_oob_sign",
            ShiftOp::Asr,
            1,
            0x8001,
            2,
            16,
            OpWidth::W16,
            2,
        );
        assert_shift_reg_count_alias_lowering(
            "ror_w8_dst_aliases_count",
            ShiftOp::Ror,
            1,
            0x81,
            2,
            9,
            OpWidth::W8,
            2,
        );
        assert_shift_reg_count_alias_lowering(
            "ror_w8_dst_aliases_src_and_count",
            ShiftOp::Ror,
            1,
            0x81,
            1,
            0x81,
            OpWidth::W8,
            1,
        );
    }
    #[test]
    fn lowers_clz_w8_as_aligned_sentinel_clz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Clz {
                dst: x(0),
                src: x(1),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 8, 7, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 9, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_clz_w16_as_aligned_sentinel_clz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Clz {
                dst: x(0),
                src: x(1),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 16, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 17, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_clz_w16_imm_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Clz {
                dst: x(0),
                src: VReg::Imm(0x1_0000_0080),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 8, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ctz_x_as_rbit_clz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Ctz {
                dst: x(0),
                src: x(1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1_regs(1, 0b000000, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(1, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ctz_w_as_rbit_clz_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Ctz {
                dst: x(0),
                src: x(1),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000000, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsf_x_as_rbit_clz_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsf {
                dst: x(0),
                src: x(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1_regs(1, 0b000000, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(1, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 0, 5, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsf_w_as_rbit_clz_ubfx_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsf {
                dst: x(0),
                src: x(1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000000, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 4, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsf_w16_imm_masked_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsf {
                dst: x(0),
                src: VReg::Imm(0x1_0000_0080),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 7, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsr_x_as_orr_clz_eor_mask() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsr {
                dst: x(0),
                src: x(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 0, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(1, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(1, 0b10, 1, 0, 5, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsr_w_as_orr_clz_eor_mask_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsr {
                dst: x(0),
                src: x(1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 0, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 0, 4, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsr_w16_imm_masked_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsr {
                dst: x(0),
                src: VReg::Imm(0x1_0000_8000),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 15, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_x_imm_control_as_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: x(1),
                control: VReg::Imm((12 << 8) | 4),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 4, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_x_two_imms_as_movz_movk() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: VReg::Imm(0x1234_5678_9abc_def0),
                control: VReg::Imm((32 << 8) | 16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x9abc, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b11, 1, 0x5678, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_w_two_imms_all_ones_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: VReg::Imm(-1),
                control: VReg::Imm(32 << 8),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_x_two_imms_all_ones_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: VReg::Imm(-1),
                control: VReg::Imm(64 << 8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_w8_imm_control_as_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: x(1),
                control: VReg::Imm((3 << 8) | 2),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 2, 4, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_w8_two_imms_as_movz_masked_extract() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: VReg::Imm(0x1ff),
                control: VReg::Imm((3 << 8) | 2),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 7, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_w16_imm_control_clips_at_subword_width() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: x(1),
                control: VReg::Imm((8 << 8) | 12),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 12, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_w8_imm_control_empty_extract_as_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: x(1),
                control: VReg::Imm((1 << 8) | 8),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_w_imm_control_empty_extract_as_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: x(1),
                control: VReg::Imm((8 << 8) | 32),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_x_imm_control_with_flags_as_ubfx_ands() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: x(1),
                control: VReg::Imm((16 << 8) | 8),
                width: OpWidth::W64,
                flags: bextr_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 8, 23, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_x_two_imms_with_flags_as_movz_movk_ands() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: VReg::Imm(0x1234_5678_9abc_def0),
                control: VReg::Imm((32 << 8) | 16),
                width: OpWidth::W64,
                flags: bextr_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x9abc, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b11, 1, 0x5678, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_x_two_imms_all_ones_with_flags_as_movn_ands() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bextr {
                dst: x(0),
                src: VReg::Imm(-1),
                control: VReg::Imm(64 << 8),
                width: OpWidth::W64,
                flags: bextr_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bextr_zero_imm_source_reg_control_as_zero() {
        let cases = [
            (
                OpKind::Bextr {
                    dst: x(0),
                    src: VReg::Imm(0),
                    control: x(2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(1, 0b10, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Bextr {
                    dst: x(0),
                    src: VReg::Imm(0x1_0000_0000),
                    control: x(0),
                    width: OpWidth::W32,
                    flags: bextr_flags(),
                },
                vec![
                    enc_mov_wide(0, 0b10, 0, 0, 0),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Bextr {
                    dst: x(1),
                    src: VReg::Imm(0x100),
                    control: x(2),
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0, 1), 0xd65f_03c0u32],
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
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_bextr_register_control_runtime() {
        assert_bextr_runtime_control_lowering(
            "bextr_x_register_control_basic",
            0,
            1,
            0xfedc_ba98_7654_3210,
            2,
            (12 << 8) | 4,
            OpWidth::W64,
            FlagUpdate::None,
            0b1011,
        );
        assert_bextr_runtime_control_lowering(
            "bextr_w_register_control_len_ge_bits",
            0,
            1,
            0x7654_3210,
            2,
            (64 << 8) | 8,
            OpWidth::W32,
            FlagUpdate::None,
            0b1011,
        );
        assert_bextr_runtime_control_lowering(
            "bextr_x_register_control_zero_length",
            0,
            1,
            0xfedc_ba98_7654_3210,
            2,
            5,
            OpWidth::W64,
            FlagUpdate::None,
            0b1011,
        );
        assert_bextr_runtime_control_lowering(
            "bextr_x_register_control_start_oob",
            0,
            1,
            0xfedc_ba98_7654_3210,
            2,
            (8 << 8) | 64,
            OpWidth::W64,
            FlagUpdate::None,
            0b1011,
        );
        assert_bextr_runtime_control_lowering(
            "bextr_w8_register_control_masks_source",
            0,
            1,
            0x1f5,
            2,
            (3 << 8) | 4,
            OpWidth::W8,
            FlagUpdate::None,
            0b1011,
        );
        assert_bextr_runtime_control_lowering(
            "bextr_w16_register_control_dst_aliases_src",
            0,
            0,
            0xabcd,
            2,
            (10 << 8) | 3,
            OpWidth::W16,
            FlagUpdate::None,
            0b1011,
        );
        assert_bextr_runtime_control_lowering(
            "bextr_x_register_control_dst_aliases_control",
            2,
            1,
            0xfedc_ba98_7654_3210,
            2,
            (8 << 8) | 4,
            OpWidth::W64,
            FlagUpdate::None,
            0b1011,
        );
    }
    #[test]
    fn lowers_bextr_register_control_with_flags_runtime() {
        assert_bextr_runtime_control_lowering(
            "bextr_x_register_control_flags_nonzero",
            0,
            1,
            0xff00,
            2,
            (8 << 8) | 8,
            OpWidth::W64,
            bextr_flags(),
            0b1111,
        );
        assert_bextr_runtime_control_lowering(
            "bextr_x_register_control_flags_zero",
            0,
            1,
            0xff00,
            2,
            8,
            OpWidth::W64,
            bextr_flags(),
            0b1011,
        );
    }
    #[test]
    fn lowers_pdep_pext_zero_and_full_masks_as_identity_or_zero() {
        let cases = [
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0),
                    width: OpWidth::W64,
                },
                vec![enc_mov_wide(1, 0b10, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: x(0),
                    mask: VReg::Imm(-1),
                    width: OpWidth::W64,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: x(0),
                    mask: VReg::Imm(0xffff_ffff),
                    width: OpWidth::W32,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0xff),
                    width: OpWidth::W8,
                },
                vec![enc_bitfield_regs(0, 0b10, 0, 7, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: VReg::Imm(-1),
                    mask: VReg::Imm(0xffff),
                    width: OpWidth::W16,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0xffff, 0), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in expected_words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_pdep_pext_low_masks_as_and_mask() {
        let cases = [
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0x1fff),
                    width: OpWidth::W64,
                },
                vec![enc_bitfield_regs(1, 0b10, 0, 12, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0x1f),
                    width: OpWidth::W32,
                },
                vec![enc_bitfield_regs(0, 0b10, 0, 4, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0xf),
                    width: OpWidth::W8,
                },
                vec![enc_bitfield_regs(0, 0b10, 0, 3, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: VReg::Imm(0x12345),
                    mask: VReg::Imm(0xff),
                    width: OpWidth::W16,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0x45, 0), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in expected_words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_pdep_pext_single_bit_masks_as_bitfield_ops() {
        let cases = [
            (
                OpKind::Pext {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(1 << 12),
                    width: OpWidth::W64,
                },
                vec![enc_bitfield_regs(1, 0b10, 12, 12, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(1 << 12),
                    width: OpWidth::W64,
                },
                vec![enc_bitfield_regs(1, 0b10, 52, 0, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0x20),
                    width: OpWidth::W8,
                },
                vec![enc_bitfield_regs(0, 0b10, 5, 5, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: VReg::Imm(3),
                    mask: VReg::Imm(0x80),
                    width: OpWidth::W16,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0x80, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: VReg::Imm(0),
                    mask: VReg::Imm(0x4000),
                    width: OpWidth::W16,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0, 0), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in expected_words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_pdep_pext_shifted_contiguous_masks_as_bitfield_ops() {
        let cases = [
            (
                OpKind::Pext {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0x1f00),
                    width: OpWidth::W64,
                },
                vec![enc_bitfield_regs(1, 0b10, 8, 12, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0x3f_0000),
                    width: OpWidth::W64,
                },
                vec![enc_bitfield_regs(1, 0b10, 48, 5, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0x03f0),
                    width: OpWidth::W16,
                },
                vec![enc_bitfield_regs(0, 0b10, 4, 9, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: x(1),
                    mask: VReg::Imm(0x70),
                    width: OpWidth::W8,
                },
                vec![enc_bitfield_regs(0, 0b10, 28, 2, 1, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: VReg::Imm(0x3f),
                    mask: VReg::Imm(0x3f00),
                    width: OpWidth::W16,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0x3f00, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pext {
                    dst: x(0),
                    src: VReg::Imm(0xabc0),
                    mask: VReg::Imm(0x0ff0),
                    width: OpWidth::W64,
                },
                vec![enc_mov_wide(1, 0b10, 0, 0xbc, 0), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in expected_words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_pdep_pext_arbitrary_masks_with_imm_sources_as_constants() {
        let cases = [
            (
                OpKind::Pext {
                    dst: x(0),
                    src: VReg::Imm(0x421),
                    mask: VReg::Imm(0x421),
                    width: OpWidth::W64,
                },
                vec![enc_mov_wide(1, 0b10, 0, 7, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: VReg::Imm(0b101),
                    mask: VReg::Imm(0x421),
                    width: OpWidth::W64,
                },
                vec![enc_mov_wide(1, 0b10, 0, 0x401, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Pdep {
                    dst: x(0),
                    src: VReg::Imm(0b1010),
                    mask: VReg::Imm(0x8421),
                    width: OpWidth::W16,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0x8020, 0), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in expected_words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_bzhi_w_with_low_byte_index_guards() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: x(2),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_test_branch(2, 5, true, 28).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(2, 6, true, 24).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(2, 7, true, 20).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp2_regs(0, 0b1000, 0, 2, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b00, 1, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_b(2).to_le_bytes());
        expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_with_low_byte_index_guards() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: x(2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_test_branch(2, 6, true, 24).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(2, 7, true, 20).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp2_regs(1, 0b1000, 0, 2, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b00, 1, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_b(2).to_le_bytes());
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_with_flags_and_low_byte_index_guards() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: x(2),
                width: OpWidth::W64,
                flags: bzhi_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_test_branch(2, 6, true, 28).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(2, 7, true, 24).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp2_regs(1, 0b1000, 0, 2, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b00, 1, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_b(4).to_le_bytes());
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_flagm(0b000).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_zero_imm_source_reg_index_as_zero() {
        let cases = [
            (
                OpKind::Bzhi {
                    dst: x(0),
                    src: VReg::Imm(0),
                    index: x(2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                enc_mov_wide(1, 0b10, 0, 0, 0),
            ),
            (
                OpKind::Bzhi {
                    dst: x(0),
                    src: VReg::Imm(0x1_0000_0000),
                    index: x(0),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                enc_mov_wide(0, 0b10, 0, 0, 0),
            ),
            (
                OpKind::Bzhi {
                    dst: x(1),
                    src: VReg::Imm(0x100),
                    index: x(2),
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                enc_mov_wide(0, 0b10, 0, 0, 1),
            ),
            (
                OpKind::Bzhi {
                    dst: x(2),
                    src: VReg::Imm(0x1_0000),
                    index: x(3),
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                },
                enc_mov_wide(0, 0b10, 0, 0, 2),
            ),
        ];

        for (kind, expected_word) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&expected_word.to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_bzhi_x_imm_index_as_and_mask_in_place() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(0),
                index: VReg::Imm(13),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 0, 12, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_two_imms_as_movz_movk() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: VReg::Imm(0x1234_5678_9abc_def0),
                index: VReg::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0xdef0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b11, 1, 0x9abc, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_w_two_imms_all_ones_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: VReg::Imm(-1),
                index: VReg::Imm(32),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_two_imms_all_ones_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: VReg::Imm(-1),
                index: VReg::Imm(64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_imm_index_with_flags_as_and_ands() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: VReg::Imm(13),
                width: OpWidth::W64,
                flags: bzhi_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 0, 12, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_w8_imm_index_with_flags_as_and_uxtb_ands() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: VReg::Imm(5),
                width: OpWidth::W8,
                flags: bzhi_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 0, 4, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_w16_imm_index_at_width_with_flags_sets_carry() {
        assert_bzhi_imm_index_lowering(
            "bzhi_w16_imm_index_at_width_with_flags_sets_carry",
            1,
            0x8000,
            16,
            OpWidth::W16,
            0,
            bzhi_flags(),
            0b0001,
        );
    }
    #[test]
    fn lowers_bzhi_x_two_imms_with_flags_sets_carry() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: VReg::Imm(1),
                index: VReg::Imm(64),
                width: OpWidth::W64,
                flags: bzhi_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_flagm(0b000).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: VReg::Imm(-1),
                index: VReg::Imm(64),
                width: OpWidth::W64,
                flags: bzhi_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_flagm(0b000).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: VReg::Imm(64),
                width: OpWidth::W64,
                flags: bzhi_flags(),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_flagm(0b000).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_x_imm_index_zero_as_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(0),
                index: VReg::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_w8_imm_index_as_and_mask() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: VReg::Imm(5),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 0, 4, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_w8_two_imms_as_movz_masked() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: VReg::Imm(0x1ff),
                index: VReg::Imm(5),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x1f, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_w16_imm_index_at_width_as_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: VReg::Imm(16),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_w8_imm_index_zero_as_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: VReg::Imm(0),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_subword_imm_index_with_flags_runtime() {
        assert_bzhi_imm_index_lowering(
            "bzhi_w8_imm_index_at_width_sets_subword_negative_and_carry",
            1,
            0xffff_ffff_ffff_ff80,
            8,
            OpWidth::W8,
            0,
            bzhi_flags(),
            0b0101,
        );
        assert_bzhi_imm_index_lowering(
            "bzhi_w8_imm_index_zero_sets_zero",
            1,
            0xff,
            0,
            OpWidth::W8,
            0,
            bzhi_flags(),
            0b1010,
        );
        assert_bzhi_imm_index_lowering(
            "bzhi_w16_imm_index_masks_and_clears_carry",
            1,
            0xffff_ffff_ffff_80f5,
            8,
            OpWidth::W16,
            0,
            bzhi_flags(),
            0b1111,
        );
    }
    #[test]
    fn lowers_bzhi_w_imm_index_at_width_as_mov_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bzhi {
                dst: x(0),
                src: x(1),
                index: VReg::Imm(0x120),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bzhi_runtime_index_aliases_with_scratch() {
        assert_bzhi_runtime_index_lowering(
            "bzhi_w_dst_aliases_src_zero_extends",
            1,
            0xffff_ffff_8000_00f5,
            2,
            9,
            OpWidth::W32,
            1,
            FlagUpdate::None,
            0b1011,
        );
        assert_bzhi_runtime_index_lowering(
            "bzhi_x_dst_aliases_index_passes_through",
            1,
            0x8000_0000_0000_0001,
            2,
            64,
            OpWidth::W64,
            2,
            FlagUpdate::None,
            0b0101,
        );
        assert_bzhi_runtime_index_lowering(
            "bzhi_x_dst_aliases_index_sets_zero_flag",
            1,
            0x20,
            2,
            5,
            OpWidth::W64,
            2,
            bzhi_flags(),
            0b0101,
        );
        assert_bzhi_runtime_index_lowering(
            "bzhi_x_dst_aliases_src_and_index",
            1,
            0x1234_5678_9abc_0012,
            1,
            0x1234_5678_9abc_0012,
            OpWidth::W64,
            1,
            FlagUpdate::None,
            0b0110,
        );
        assert_bzhi_runtime_index_lowering(
            "bzhi_w8_runtime_index_sets_subword_negative_and_carry",
            1,
            0xffff_ffff_ffff_ff80,
            2,
            8,
            OpWidth::W8,
            0,
            bzhi_flags(),
            0b0101,
        );
        assert_bzhi_runtime_index_lowering(
            "bzhi_w16_runtime_index_masks_and_clears_carry",
            1,
            0xffff_ffff_ffff_80f5,
            2,
            8,
            OpWidth::W16,
            0,
            bzhi_flags(),
            0b1111,
        );
        assert_bzhi_runtime_index_lowering(
            "bzhi_w16_dst_aliases_index_passes_masked_source",
            1,
            0xffff_ffff_ffff_8001,
            2,
            16,
            OpWidth::W16,
            2,
            bzhi_flags(),
            0b0000,
        );
    }
    #[test]
    fn lowers_bit_test_runtime() {
        assert_bt_lowering(
            "bt_w8_imm_wrap_sets_carry",
            1,
            0x102,
            SrcOperand::Imm(9),
            9,
            OpWidth::W8,
            0b1001,
        );
        assert_bt_lowering(
            "bt_w16_reg_wrap_clears_carry",
            1,
            0x7fff,
            SrcOperand::Reg(x(2)),
            31,
            OpWidth::W16,
            0b0011,
        );
        assert_bt_lowering(
            "bt_w32_negative_imm_uses_high_bit",
            1,
            0x8000_0000,
            SrcOperand::Imm(-1),
            u64::MAX,
            OpWidth::W32,
            0b0100,
        );
        assert_bt_lowering(
            "bt_x_reg_uses_bit63",
            1,
            0x8000_0000_0000_0000,
            SrcOperand::Reg(x(3)),
            127,
            OpWidth::W64,
            0b0000,
        );
    }
    #[test]
    fn lowers_bit_test_update_runtime() {
        assert_bit_update_lowering(
            "btr_w8_dst_aliases_src_masks_result",
            BitTestAction::Reset,
            1,
            1,
            0x1ff,
            SrcOperand::Imm(0),
            0,
            OpWidth::W8,
            0b0101,
        );
        assert_bit_update_lowering(
            "bts_w16_reg_index_sets_bit_and_clears_carry",
            BitTestAction::Set,
            0,
            1,
            0x8001,
            SrcOperand::Reg(x(2)),
            20,
            OpWidth::W16,
            0b1111,
        );
        assert_bit_update_lowering(
            "btr_w16_dst_aliases_index",
            BitTestAction::Reset,
            2,
            1,
            0xffff,
            SrcOperand::Reg(x(2)),
            20,
            OpWidth::W16,
            0b0000,
        );
        assert_bit_update_lowering(
            "btc_w32_imm_toggles_and_sets_carry",
            BitTestAction::Toggle,
            0,
            1,
            0xffff_ffff,
            SrcOperand::Imm(5),
            5,
            OpWidth::W32,
            0b1000,
        );
        assert_bit_update_lowering(
            "btc_x_dst_aliases_src_high_bit",
            BitTestAction::Toggle,
            1,
            1,
            0x8000_0000_0000_0003,
            SrcOperand::Reg(x(2)),
            63,
            OpWidth::W64,
            0b0100,
        );
    }
    #[test]
    fn lowers_x86_subword_bit_updates_with_partial_register_merge() {
        let alias = lower_single_op(OpKind::Bts {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rax),
            index: SrcOperand::Imm(15),
            width: OpWidth::W16,
        });
        let (out, nzcv, sp) = run_aarch64_code(
            &alias,
            &[(0, 0xABCD_EF01_2345_0001), (16, 0x1616_1616_1616_1616)],
            0b1100,
        );
        assert_eq!(out[0], 0xABCD_EF01_2345_8001);
        assert_eq!(nzcv, 0b1100, "tested bit was clear; only C is replaced");
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(sp, 0x8000);

        let distinct = lower_single_op(OpKind::Btr {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rcx),
            index: SrcOperand::Reg(x86(X86Reg::Rdx)),
            width: OpWidth::W16,
        });
        let (out, nzcv, sp) = run_aarch64_code(
            &distinct,
            &[
                (0, 0xABCD_EF01_2345_5555),
                (1, 0xFEDC_BA98_7654_FFFF),
                (2, 17),
                (16, 0x1616_1616_1616_1616),
            ],
            0b1001,
        );
        assert_eq!(out[0], 0xABCD_EF01_2345_FFFD);
        assert_eq!(out[1], 0xFEDC_BA98_7654_FFFF);
        assert_eq!(out[2], 17);
        assert_eq!(nzcv, 0b1011, "bit 1 was set; only C is replaced");
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_pdep_x_contiguous_imm_mask_as_ubfiz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Pdep {
                dst: x(0),
                src: x(1),
                mask: VReg::Imm(0x1f0),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 60, 4, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_pext_x_contiguous_imm_mask_as_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Pext {
                dst: x(0),
                src: x(1),
                mask: VReg::Imm(0xff00),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 8, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_pdep_x_non_contiguous_imm_mask_with_test_branches() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Pdep {
                dst: x(0),
                src: x(1),
                mask: VReg::Imm(0b10110),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(1, 2, false, 8).to_le_bytes());
        expected.extend_from_slice(&enc_orr_single_bit(1, 0, 0, 4).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(1, 1, false, 8).to_le_bytes());
        expected.extend_from_slice(&enc_orr_single_bit(1, 0, 0, 2).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(1, 0, false, 8).to_le_bytes());
        expected.extend_from_slice(&enc_orr_single_bit(1, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_pext_x_non_contiguous_imm_mask_with_shifted_result() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Pext {
                dst: x(0),
                src: x(1),
                mask: VReg::Imm(0b10110),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(1, 4, false, 8).to_le_bytes());
        expected.extend_from_slice(&enc_orr_single_bit(1, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 63, 62, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(1, 2, false, 8).to_le_bytes());
        expected.extend_from_slice(&enc_orr_single_bit(1, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 63, 62, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_test_branch(1, 1, false, 8).to_le_bytes());
        expected.extend_from_slice(&enc_orr_single_bit(1, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_pdep_pext_zero_mask_and_immediate_source() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Pdep {
                dst: x(0),
                src: x(1),
                mask: VReg::Imm(0),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            4,
            OpKind::Pext {
                dst: x(1),
                src: VReg::Imm(0b10110),
                mask: VReg::Imm(0b10110),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 7, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_pdep_pext_runtime_masks_with_exact_results() {
        assert_pdep_pext_runtime_mask_lowering(
            "pdep_x_sparse_runtime_mask",
            true,
            Some(1),
            0b1011_0110,
            2,
            0x8040_0101_0000_1021,
            OpWidth::W64,
            0,
        );
        assert_pdep_pext_runtime_mask_lowering(
            "pdep_x_full_runtime_mask_copies_high_bit",
            true,
            Some(1),
            0x8000_0000_0000_0001,
            2,
            u64::MAX,
            OpWidth::W64,
            0,
        );
        assert_pdep_pext_runtime_mask_lowering(
            "pext_x_sparse_runtime_mask_dst_aliases_src",
            false,
            Some(1),
            0xf0f1_2233_4455_6677,
            2,
            0x0101_0101_8000_001f,
            OpWidth::W64,
            1,
        );
        assert_pdep_pext_runtime_mask_lowering(
            "pext_x_full_runtime_mask_reconstructs_high_bit",
            false,
            Some(1),
            0x8000_0000_0000_0001,
            2,
            u64::MAX,
            OpWidth::W64,
            0,
        );
        assert_pdep_pext_runtime_mask_lowering(
            "pdep_w_runtime_mask_dst_aliases_mask",
            true,
            Some(1),
            0xffff_0001,
            2,
            0x8080_00f1,
            OpWidth::W32,
            2,
        );
        assert_pdep_pext_runtime_mask_lowering(
            "pext_x_zero_runtime_mask_dst_aliases_mask",
            false,
            Some(1),
            0xffff_ffff_ffff_ffff,
            2,
            0,
            OpWidth::W64,
            2,
        );
        assert_pdep_pext_runtime_mask_lowering(
            "pext_x_immediate_source_runtime_mask",
            false,
            None,
            0xdead_beef_1234_5678,
            2,
            0x00ff_000f_f000_00ff,
            OpWidth::W64,
            0,
        );
        assert_pdep_pext_runtime_mask_lowering(
            "pext_h_runtime_mask_masks_subword_inputs",
            false,
            Some(1),
            0xffff_1234,
            2,
            0xa55a,
            OpWidth::W16,
            0,
        );
    }
    #[test]
    fn lowers_pdep_pext_non_contiguous_imm_mask_source_aliases() {
        assert_pdep_pext_const_mask_lowering(
            "pdep_x_sparse_imm_mask_dst_aliases_src",
            true,
            0,
            0b1011_0110,
            0b10110,
            OpWidth::W64,
            0,
        );
        assert_pdep_pext_const_mask_lowering(
            "pext_x_sparse_imm_mask_dst_aliases_src",
            false,
            0,
            0x8040_0101_0000_1016,
            0x8040_0101_0000_1016,
            OpWidth::W64,
            0,
        );
        assert_pdep_pext_const_mask_lowering(
            "pdep_w16_sparse_imm_mask_dst_aliases_src_masks_source",
            true,
            0,
            0xffff_0000_0000_0005,
            0xa55a,
            OpWidth::W16,
            0,
        );
        assert_pdep_pext_const_mask_lowering(
            "pext_w16_sparse_imm_mask_dst_aliases_src_masks_source",
            false,
            0,
            0xffff_0000_0000_a55a,
            0xa55a,
            OpWidth::W16,
            0,
        );
    }
    #[test]
    fn lowers_eor_x_high_bit_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0x8000_0000_0000_0000_u64 as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b10, 1, 1, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_eor_x_repeated_alternating_bit_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0x5555_5555_5555_5555),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b10, 0, 0, 60, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_andnot_x_high_bit_imm_as_and_inverse_mask() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0x8000_0000_0000_0000_u64 as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 0, 62, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bics_w_high_bits_imm_as_ands_inverse_mask() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0xffff_ff00),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(0, 0b11, 0, 0, 7, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_test_x_high_bit_imm_to_zero_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Test {
                src1: x(1),
                src2: SrcOperand::Imm64(0x8000_0000_0000_0000_u64 as i64),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b11, 1, 1, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_truncate_x_to_w8_as_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Truncate {
                dst: x(0),
                src: x(1),
                from_width: OpWidth::W64,
                to_width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield(1, 0b10, 0, 7).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_truncate_x_to_w16_as_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Truncate {
                dst: x(0),
                src: x(1),
                from_width: OpWidth::W64,
                to_width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield(1, 0b10, 0, 15).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_truncate_x_to_w32_as_w_mov_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Truncate {
                dst: x(0),
                src: x(1),
                from_width: OpWidth::W64,
                to_width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_truncate_as_mov_or_noop() {
        let truncate_cases = [
            (
                OpKind::Truncate {
                    dst: x(0),
                    src: x(0),
                    from_width: OpWidth::W64,
                    to_width: OpWidth::W64,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::Truncate {
                    dst: x(0),
                    src: x(0),
                    from_width: OpWidth::W64,
                    to_width: OpWidth::W32,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Truncate {
                    dst: x(0),
                    src: x(1),
                    from_width: OpWidth::W64,
                    to_width: OpWidth::W64,
                },
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in truncate_cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in expected_words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_truncate_imm_to_w8_as_movz_masked() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Truncate {
                dst: x(0),
                src: VReg::Imm(0x1234_56ef),
                from_width: OpWidth::W64,
                to_width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xef, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_truncate_imm_to_x_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Truncate {
                dst: x(0),
                src: VReg::Imm(0x2468),
                from_width: OpWidth::W64,
                to_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x2468, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_truncate_imm_to_x_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Truncate {
                dst: x(0),
                src: VReg::Imm(-1),
                from_width: OpWidth::W64,
                to_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ctz_in_place_as_rbit_clz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Ctz {
                dst: x(0),
                src: x(0),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1_regs(1, 0b000000, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(1, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev16_x_sequence() {
        let lo = VReg::virt(0);
        let hi = VReg::virt(1);
        let lo_shifted = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: lo,
                src1: x(1),
                src2: SrcOperand::Imm64(0x00ff_00ff_00ff_00ff),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::And {
                dst: hi,
                src1: x(1),
                src2: SrcOperand::Imm64(0xff00_ff00_ff00_ff00_u64 as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: lo_shifted,
                src: lo,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi_shifted,
                src: hi,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: lo_shifted,
                src2: SrcOperand::Reg(hi_shifted),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1(1, 0b000001).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev16_x_imm_src_as_movz() {
        let lo = VReg::virt(0);
        let hi = VReg::virt(1);
        let lo_shifted = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let src = VReg::Imm(0x12);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: lo,
                src1: src,
                src2: SrcOperand::Imm64(0x00ff_00ff_00ff_00ff),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::And {
                dst: hi,
                src1: src,
                src2: SrcOperand::Imm64(0xff00_ff00_ff00_ff00_u64 as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: lo_shifted,
                src: lo,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi_shifted,
                src: hi,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: lo_shifted,
                src2: SrcOperand::Reg(hi_shifted),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x1200, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev16_x_imm_all_ones_as_movn() {
        let lo = VReg::virt(0);
        let hi = VReg::virt(1);
        let lo_shifted = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let src = VReg::Imm(-1);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: lo,
                src1: src,
                src2: SrcOperand::Imm64(0x00ff_00ff_00ff_00ff),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::And {
                dst: hi,
                src1: src,
                src2: SrcOperand::Imm64(0xff00_ff00_ff00_ff00_u64 as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: lo_shifted,
                src: lo,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi_shifted,
                src: hi,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: lo_shifted,
                src2: SrcOperand::Reg(hi_shifted),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev16_w_sequence() {
        let lo = VReg::virt(0);
        let hi = VReg::virt(1);
        let lo_shifted = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: lo,
                src1: x(1),
                src2: SrcOperand::Imm64(0x00ff_00ff),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::And {
                dst: hi,
                src1: x(1),
                src2: SrcOperand::Imm64(0xff00_ff00),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: lo_shifted,
                src: lo,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi_shifted,
                src: hi,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: lo_shifted,
                src2: SrcOperand::Reg(hi_shifted),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1(0, 0b000001).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev16_w_imm_all_ones_as_movn() {
        let lo = VReg::virt(0);
        let hi = VReg::virt(1);
        let lo_shifted = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let src = VReg::Imm(-1);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: lo,
                src1: src,
                src2: SrcOperand::Imm64(0x00ff_00ff),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::And {
                dst: hi,
                src1: src,
                src2: SrcOperand::Imm64(0xff00_ff00),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: lo_shifted,
                src: lo,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi_shifted,
                src: hi,
                amount: SrcOperand::Imm(8),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: lo_shifted,
                src2: SrcOperand::Reg(hi_shifted),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev16_w_with_masked_immediates() {
        let lo = VReg::virt(0);
        let hi = VReg::virt(1);
        let lo_shifted = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: lo,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_00ff_00ff),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::And {
                dst: hi,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_ff00_ff00),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: lo_shifted,
                src: lo,
                amount: SrcOperand::Imm(72),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi_shifted,
                src: hi,
                amount: SrcOperand::Imm64(72),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: lo_shifted,
                src2: SrcOperand::Reg(hi_shifted),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1(0, 0b000001).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bswap_w8_as_mov_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: x(0),
                src: x(1),
                width: OpWidth::W8,
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
    fn lowers_bswap_w8_imm_as_movz_full_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: x(0),
                src: VReg::Imm(0x1234),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x1234, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_rbit_w32_imm_as_movz_movk_reversed() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Rbit {
                dst: x(0),
                src: VReg::Imm(0x0123_4567),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xc480, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b11, 1, 0xe6a2, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bswap_w16_as_rev16_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: x(0),
                src: x(1),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1(0, 0b000001).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bswap_w16_imm_as_movz_swapped() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: x(0),
                src: VReg::Imm(0x1_0000_1234),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x3412, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bswap_w32_imm_all_ones_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: x(0),
                src: VReg::Imm(-1),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bswap_x_imm_all_ones_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: x(0),
                src: VReg::Imm(-1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev32_x_sequence() {
        let lo_rev = VReg::virt(0);
        let hi = VReg::virt(1);
        let hi_rev = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: lo_rev,
                src: x(1),
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi,
                src: x(1),
                amount: SrcOperand::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: hi_rev,
                src: hi,
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: hi_shifted,
                src: hi_rev,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: hi_shifted,
                src2: SrcOperand::Reg(lo_rev),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1(1, 0b000010).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev32_x_imm_src_as_movz() {
        let lo_rev = VReg::virt(0);
        let hi = VReg::virt(1);
        let hi_rev = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let src = VReg::Imm(0x1200_0000);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: lo_rev,
                src,
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi,
                src,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: hi_rev,
                src: hi,
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: hi_shifted,
                src: hi_rev,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: hi_shifted,
                src2: SrcOperand::Reg(lo_rev),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x12, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev32_x_imm_all_ones_as_movn() {
        let lo_rev = VReg::virt(0);
        let hi = VReg::virt(1);
        let hi_rev = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let src = VReg::Imm(-1);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: lo_rev,
                src,
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi,
                src,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: hi_rev,
                src: hi,
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: hi_shifted,
                src: hi_rev,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: hi_shifted,
                src2: SrcOperand::Reg(lo_rev),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_rev32_x_with_masked_shift_counts() {
        let lo_rev = VReg::virt(0);
        let hi = VReg::virt(1);
        let hi_rev = VReg::virt(2);
        let hi_shifted = VReg::virt(3);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: lo_rev,
                src: x(1),
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shr {
                dst: hi,
                src: x(1),
                amount: SrcOperand::Imm64(96),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Bswap {
                dst: hi_rev,
                src: hi,
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: hi_shifted,
                src: hi_rev,
                amount: SrcOperand::Imm(96),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: hi_shifted,
                src2: SrcOperand::Reg(lo_rev),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp1(1, 0b000010).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_x_all_ones_imm_src_as_orr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(0),
                src: VReg::Imm(0xff),
                lsb: 8,
                width_bits: 8,
                op_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 56, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_x_all_ones_imm_src_from_input_as_orr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: VReg::Imm(0xff),
                lsb: 8,
                width_bits: 8,
                op_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 56, 7, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_x_zero_imm_src_as_and() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: VReg::Imm(0),
                lsb: 8,
                width_bits: 8,
                op_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 48, 55, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_w_full_width_imm_src_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: VReg::Imm(0x1234),
                lsb: 0,
                width_bits: 32,
                op_width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x1234, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_w_full_width_negative_imm_src_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: VReg::Imm(-15),
                lsb: 0,
                width_bits: 32,
                op_width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0xe, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_x_full_width_all_ones_imm_src_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: VReg::Imm(-1),
                lsb: 0,
                width_bits: 64,
                op_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_x_full_width_negative_imm_src_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: VReg::Imm(-15),
                lsb: 0,
                width_bits: 64,
                op_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0xe, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_full_width_reg_as_mov_or_noop() {
        let bfi_cases = [
            (
                OpKind::Bfi {
                    dst: x(0),
                    dst_in: x(1),
                    src: x(0),
                    lsb: 0,
                    width_bits: 64,
                    op_width: OpWidth::W64,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::Bfi {
                    dst: x(0),
                    dst_in: x(1),
                    src: x(0),
                    lsb: 0,
                    width_bits: 32,
                    op_width: OpWidth::W32,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Bfi {
                    dst: x(0),
                    dst_in: x(0),
                    src: x(2),
                    lsb: 0,
                    width_bits: 64,
                    op_width: OpWidth::W64,
                },
                vec![enc_mov_reg(1, 0, 2), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in bfi_cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            for word in expected_words {
                expected.extend_from_slice(&word.to_le_bytes());
            }
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_bfi_x_encodable_imm_src_as_and_orr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: VReg::Imm(0x3c),
                lsb: 8,
                width_bits: 8,
                op_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 48, 55, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 54, 3, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bfi_when_dst_aliases_src() {
        assert_bfi_lowering(
            "bfi_x_dst_aliases_src",
            0,
            1,
            0xaaaa_bbbb_ccdd_eeff,
            0,
            0x1234,
            8,
            8,
            OpWidth::W64,
        );
        assert_bfi_lowering(
            "bfi_w_dst_aliases_src",
            0,
            1,
            0xfedc_ba98,
            0,
            0x7654_3210,
            4,
            12,
            OpWidth::W32,
        );
    }
    #[test]
    fn lowers_subword_rol_reg() {
        assert_rol_reg_lowering("rol_w8_reg", 1, 0x81, 2, 1, OpWidth::W8, 0);
        assert_rol_reg_lowering("rol_w8_dst_aliases_count", 1, 0x81, 2, 9, OpWidth::W8, 2);
        assert_rol_reg_lowering("rol_w8_dst_aliases_src", 1, 0x81, 2, 1, OpWidth::W8, 1);
        assert_rol_reg_lowering(
            "rol_w16_dst_aliases_src_and_count",
            1,
            0x8001,
            1,
            0x8001,
            OpWidth::W16,
            1,
        );
    }
    #[test]
    fn lowers_shrd_w8_imm_as_shift_bfi_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Shrd {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Imm(3),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 3, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 27, 2, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_shrd_w16_imm_as_shift_bfi_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Shrd {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Imm(5),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 5, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 21, 4, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_shrd_subword_imm_src_masks_destination_before_right_shift() {
        assert_shrd_imm_src_lowering(
            "shrd_w8_imm_src_zero_insert_masks_dst",
            0xffff_ffff_ffff_12a5,
            0x18,
            3,
            OpWidth::W8,
        );
        assert_shrd_imm_src_lowering(
            "shrd_w8_imm_src_encodable_masks_dst",
            0x200,
            1,
            3,
            OpWidth::W8,
        );
        assert_shrd_imm_src_lowering(
            "shrd_w16_imm_src_zero_insert_masks_dst",
            0xffff_ffff_ffff_92a5,
            0x1800,
            5,
            OpWidth::W16,
        );
        assert_shrd_imm_src_lowering(
            "shrd_w16_imm_src_encodable_masks_dst",
            0x2_0000,
            1,
            5,
            OpWidth::W16,
        );
    }
    #[test]
    fn lowers_x86_subword_carry_rotate_partial_write_matrix() {
        let flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
        for width in [OpWidth::W8, OpWidth::W16] {
            for right in [false, true] {
                for carry_in in [false, true] {
                    let initial = match width {
                        OpWidth::W8 => 0xaaaa_bbbb_cccc_dd81,
                        OpWidth::W16 => 0xaaaa_bbbb_cccc_8001,
                        _ => unreachable!(),
                    };
                    let kind = if right {
                        OpKind::Rcr {
                            dst: x86(X86Reg::Rax),
                            src: x86(X86Reg::Rax),
                            amount: SrcOperand::Imm(1),
                            width,
                            flags,
                        }
                    } else {
                        OpKind::Rcl {
                            dst: x86(X86Reg::Rax),
                            src: x86(X86Reg::Rax),
                            amount: SrcOperand::Imm(1),
                            width,
                            flags,
                        }
                    };
                    let code = lower_single_op(kind);
                    let old_nzcv = 0b1101 | (u8::from(carry_in) << 1);
                    let (expected_low, expected_carry, effective) =
                        ref_rotate_carry(initial, 1, carry_in, width, right);
                    let expected =
                        (initial & !width_mask(width)) | (expected_low & width_mask(width));
                    let expected_nzcv = expected_rotate_carry_nzcv(
                        old_nzcv,
                        expected_low,
                        expected_carry,
                        effective,
                        width,
                        flags,
                        right,
                    );

                    let (out, out_nzcv, sp) = run_aarch64_code(
                        &code,
                        &[
                            (0, initial),
                            (16, 0x1616_1616_1616_1616),
                            (17, 0x1717_1717_1717_1717),
                        ],
                        old_nzcv,
                    );
                    let op = if right { "RCR" } else { "RCL" };
                    assert_eq!(
                        out[0], expected,
                        "{op} {width:?} carry={carry_in} partial write"
                    );
                    assert_eq!(
                        out_nzcv, expected_nzcv,
                        "{op} {width:?} carry={carry_in} flags"
                    );
                    assert_eq!(out[16], 0x1616_1616_1616_1616, "{op} x16 scratch");
                    assert_eq!(out[17], 0x1717_1717_1717_1717, "{op} x17 scratch");
                    assert_eq!(sp, 0x8000, "{op} stack");
                }
            }
        }
    }
    #[test]
    fn lowers_ctz_w8_as_sentinel_rbit_clz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Ctz {
                dst: x(0),
                src: x(1),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 24, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000000, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ctz_w16_as_sentinel_rbit_clz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Ctz {
                dst: x(0),
                src: x(1),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 16, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000000, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsf_flag_setting_x_as_source_test_rbit_clz_ubfx() {
        let code = lower_single_op(OpKind::Bsf {
            dst: x(0),
            src: x(1),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        });
        let src = 0x8000_0000_0000_0010;
        let old_nzcv = 0b1111;
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(1, src)], old_nzcv);

        assert_eq!(out[0], ref_bsf(src, OpWidth::W64));
        assert_eq!(
            out_nzcv,
            expected_logic_source_nzcv(old_nzcv, src, OpWidth::W64, FlagUpdate::All)
        );
        assert_eq!(out[1], src);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_ctz_subword_masks_source_runtime() {
        let code = lower_single_op(OpKind::Ctz {
            dst: x(0),
            src: x(1),
            width: OpWidth::W8,
        });
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(1, 0x1000)], 0b1011);
        assert_eq!(out[0], 8);
        assert_eq!(out_nzcv, 0b1011);
        assert_eq!(sp, 0x8000);

        let code = lower_single_op(OpKind::Ctz {
            dst: x(0),
            src: x(1),
            width: OpWidth::W16,
        });
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(1, 0x20_0000)], 0b0110);
        assert_eq!(out[0], 16);
        assert_eq!(out_nzcv, 0b0110);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_bsf_flag_setting_w16_masks_source_test() {
        let code = lower_single_op(OpKind::Bsf {
            dst: x(0),
            src: x(1),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        });
        let src = 0xffff_0000_0000_0080;
        let old_nzcv = 0b0011;
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(1, src)], old_nzcv);

        assert_eq!(out[0], ref_bsf(src, OpWidth::W16));
        assert_eq!(
            out_nzcv,
            expected_logic_source_nzcv(old_nzcv, src, OpWidth::W16, FlagUpdate::All)
        );
        assert_eq!(out[1], src);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_bsf_flag_setting_imm_nonzero_before_result_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsf {
                dst: x(0),
                src: VReg::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsf_w16_as_sentinel_rbit_clz_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsf {
                dst: x(0),
                src: x(1),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 16, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000000, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 3, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsf_w8_as_sentinel_rbit_clz_ubfx() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsf {
                dst: x(0),
                src: x(1),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 24, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000000, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 2, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsr_flag_setting_x_as_source_test_orr_clz_eor_mask() {
        let code = lower_single_op(OpKind::Bsr {
            dst: x(0),
            src: x(1),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        });
        let src = 0x8000_0000_0000_0010;
        let old_nzcv = 0b1010;
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(1, src)], old_nzcv);

        assert_eq!(out[0], ref_bsr(src, OpWidth::W64));
        assert_eq!(
            out_nzcv,
            expected_logic_source_nzcv(old_nzcv, src, OpWidth::W64, FlagUpdate::All)
        );
        assert_eq!(out[1], src);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_bsr_flag_setting_w8_masks_source_test() {
        let code = lower_single_op(OpKind::Bsr {
            dst: x(0),
            src: x(1),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        });
        let src = 0xffff_ffff_ffff_ff80;
        let old_nzcv = 0b0011;
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(1, src)], old_nzcv);

        assert_eq!(out[0], ref_bsr(src, OpWidth::W8));
        assert_eq!(
            out_nzcv,
            expected_logic_source_nzcv(old_nzcv, src, OpWidth::W8, FlagUpdate::All)
        );
        assert_eq!(out[1], src);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_bsr_flag_setting_imm_zero_as_zf_test_before_result_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsr {
                dst: x(0),
                src: VReg::Imm(0),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 31, 31).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_popcnt_runtime() {
        assert_popcnt_lowering(
            "popcnt_w8_masks_source",
            0,
            1,
            0xffff_ffff_ffff_ffb6,
            OpWidth::W8,
        );
        assert_popcnt_lowering(
            "popcnt_w16_counts_low_halfword",
            0,
            1,
            0x1234_5678_0000_f0f1,
            OpWidth::W16,
        );
        assert_popcnt_lowering(
            "popcnt_w32_counts_low_word",
            0,
            1,
            0xffff_ffff_8000_0001,
            OpWidth::W32,
        );
        assert_popcnt_lowering(
            "popcnt_x_counts_full_register",
            0,
            1,
            0xffff_ffff_0000_8001,
            OpWidth::W64,
        );
        assert_popcnt_lowering(
            "popcnt_x_in_place",
            2,
            2,
            0x8421_ffff_0000_0001,
            OpWidth::W64,
        );
    }
    #[test]
    fn lowers_bsf_flag_setting_runtime() {
        assert_bit_scan_lowering(
            "bsf_x_flags_nonzero_negative_source",
            false,
            0,
            1,
            0x8000_0000_0000_0010,
            OpWidth::W64,
            FlagUpdate::All,
            0b1111,
        );
        assert_bit_scan_lowering(
            "bsf_w_flags_zero_source",
            false,
            0,
            1,
            0,
            OpWidth::W32,
            FlagUpdate::All,
            0b1011,
        );
        assert_bit_scan_lowering(
            "bsf_w16_flags_alias_masks_source",
            false,
            1,
            1,
            0xffff_0000_0000_8000,
            OpWidth::W16,
            FlagUpdate::All,
            0b1111,
        );
    }
    #[test]
    fn lowers_bsr_flag_setting_runtime() {
        assert_bit_scan_lowering(
            "bsr_w_flags_nonzero_negative_source",
            true,
            0,
            1,
            0x8000_0010,
            OpWidth::W32,
            FlagUpdate::All,
            0b1111,
        );
        assert_bit_scan_lowering(
            "bsr_x_flags_alias_zero_source",
            true,
            1,
            1,
            0,
            OpWidth::W64,
            FlagUpdate::All,
            0b1011,
        );
        assert_bit_scan_lowering(
            "bsr_w8_flags_alias_masks_source",
            true,
            1,
            1,
            0xffff_0000_0000_0080,
            OpWidth::W8,
            FlagUpdate::All,
            0b1111,
        );
    }
    #[test]
    fn lowers_x86_w16_bit_scan_partial_writes_and_preserves_non_z_flags() {
        let zf_only = FlagUpdate::Specific(FlagSet::ZF);
        let cases = [
            (
                "bsf-w16-dst-src-alias",
                false,
                x86(X86Reg::Rax),
                x86(X86Reg::Rax),
                0xaaaa_bbbb_cccc_0100,
                0xaaaa_bbbb_cccc_0100,
                OpWidth::W16,
                0b1111,
            ),
            (
                "bsr-w16-zero-source",
                true,
                x86(X86Reg::Rax),
                x86(X86Reg::Rbx),
                0xaaaa_bbbb_cccc_7777,
                0xbbbb_cccc_dddd_0000,
                OpWidth::W16,
                0b1011,
            ),
            (
                "bsf-w32-preserves-cf-sf-of",
                false,
                x86(X86Reg::Rax),
                x86(X86Reg::Rbx),
                0xaaaa_bbbb_cccc_7777,
                0xbbbb_cccc_8000_0000,
                OpWidth::W32,
                0b0011,
            ),
        ];
        let sentinels = [
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
            (14, 0x1414_1414_1414_1414),
        ];

        for (label, reverse, dst, src, dst_value, src_value, width, old_nzcv) in cases {
            let op = if reverse {
                OpKind::Bsr {
                    dst,
                    src,
                    width,
                    flags: zf_only,
                }
            } else {
                OpKind::Bsf {
                    dst,
                    src,
                    width,
                    flags: zf_only,
                }
            };
            let code = lower_single_op(op);
            let low = if reverse {
                ref_bsr(src_value, width)
            } else {
                ref_bsf(src_value, width)
            };
            let expected = if width == OpWidth::W16 {
                (dst_value & !0xffff) | low
            } else {
                low
            };
            let expected_nzcv = expected_logic_source_nzcv(old_nzcv, src_value, width, zf_only);
            let mut regs = sentinels.to_vec();
            regs.push((0, dst_value));
            if src != dst {
                regs.push((3, src_value));
            }
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

            assert_eq!(out[0], expected, "{label}: result");
            if src != dst {
                assert_eq!(out[3], src_value, "{label}: source");
            }
            for (index, value) in sentinels {
                assert_eq!(out[index as usize], value, "{label}: x{index} scratch");
            }
            assert_eq!(out_nzcv, expected_nzcv, "{label}: NZCV");
            assert_eq!(sp, 0x8000, "{label}: stack");
        }
    }
    #[test]
    fn lowers_bsr_w16_as_ubfx_orr_clz_eor_mask() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsr {
                dst: x(0),
                src: x(1),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 0, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 0, 4, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_bsr_w8_as_ubfx_orr_clz_eor_mask() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bsr {
                dst: x(0),
                src: x(1),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 0, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp1_regs(0, 0b000100, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 0, 4, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn rejects_truncate_w128_lowering() {
        for (name, src, from_width) in [
            ("register", x(1), OpWidth::W128),
            ("immediate", VReg::Imm(0x1_0000_0000), OpWidth::W64),
        ] {
            let err = try_lower_single_op(OpKind::Truncate {
                dst: x(0),
                src,
                from_width,
                to_width: OpWidth::W128,
            })
            .unwrap_err();
            assert!(
                matches!(err, LowerError::UnsupportedOp { .. }),
                "{name}: {err:?}"
            );
        }
    }
    #[test]
    fn lowers_flag_setting_subword_add_sub_runtime() {
        assert_subword_addsub_flags_lowering(
            "add_w8_sets_zero_and_carry",
            false,
            0,
            1,
            0xff,
            SrcOperand::Reg(x(2)),
            1,
            OpWidth::W8,
            0b1010,
        );
        assert_subword_addsub_flags_lowering(
            "add_w8_sets_negative_and_overflow",
            false,
            0,
            1,
            0x7f,
            SrcOperand::Reg(x(2)),
            1,
            OpWidth::W8,
            0b0101,
        );
        assert_subword_addsub_flags_lowering(
            "sub_w16_imm_sets_no_borrow_and_overflow",
            true,
            0,
            1,
            0x8000,
            SrcOperand::Imm(1),
            1,
            OpWidth::W16,
            0b0000,
        );
        assert_subword_addsub_flags_lowering(
            "sub_w8_reg_sets_borrow_and_negative",
            true,
            0,
            1,
            0,
            SrcOperand::Reg(x(2)),
            1,
            OpWidth::W8,
            0b1111,
        );
        assert_subword_addsub_flags_lowering(
            "add_w16_dst_aliases_src2",
            false,
            2,
            1,
            0x00ff,
            SrcOperand::Reg(x(2)),
            0xff01,
            OpWidth::W16,
            0b0010,
        );
        assert_subword_addsub_flags_lowering(
            "add_w8_imm_masks_operand",
            false,
            0,
            1,
            1,
            SrcOperand::Imm(0x1ff),
            0x1ff,
            OpWidth::W8,
            0b0101,
        );
    }
    #[test]
    fn lowers_flag_setting_subword_addsub_carry_runtime() {
        assert_subword_addsub_carry_flags_lowering(
            "adc_w8_carry_in_sets_zero_and_carry",
            false,
            0,
            1,
            2,
            0xff,
            0,
            OpWidth::W8,
            0b0010,
        );
        assert_subword_addsub_carry_flags_lowering(
            "adc_w8_carry_in_sets_negative_and_overflow",
            false,
            0,
            1,
            2,
            0x7f,
            0,
            OpWidth::W8,
            0b1010,
        );
        assert_subword_addsub_carry_flags_lowering(
            "sbb_w16_no_borrow_sets_no_borrow_and_overflow",
            true,
            0,
            1,
            2,
            0x8000,
            1,
            OpWidth::W16,
            0b0010,
        );
        assert_subword_addsub_carry_flags_lowering(
            "sbb_w8_borrow_in_sets_borrow_and_negative",
            true,
            0,
            1,
            2,
            0,
            0,
            OpWidth::W8,
            0b0000,
        );
        assert_subword_addsub_carry_flags_lowering(
            "adc_w16_dst_aliases_src2",
            false,
            2,
            1,
            2,
            0xffff,
            0,
            OpWidth::W16,
            0b0010,
        );
        assert_subword_addsub_carry_flags_lowering(
            "sbb_w8_dst_aliases_src1",
            true,
            1,
            1,
            2,
            0,
            0,
            OpWidth::W8,
            0b0000,
        );
    }
    #[test]
    fn lowers_subword_addsub_carry_with_immediate_source_runtime() {
        assert_addsub_carry_lowering(
            "adc_w8_imm_carry_in_sets_zero_and_carry",
            false,
            true,
            x(0),
            1,
            0xff,
            SrcOperand::Imm(0),
            0,
            OpWidth::W8,
            0b0010,
        );
        assert_addsub_carry_lowering(
            "sbb_w16_imm_uses_borrow",
            true,
            true,
            x(0),
            1,
            0,
            SrcOperand::Imm(1),
            1,
            OpWidth::W16,
            0b0000,
        );
        assert_addsub_carry_lowering(
            "adc_w8_imm_masks_operand",
            false,
            true,
            x(0),
            1,
            0x7f,
            SrcOperand::Imm(0x100),
            0x100,
            OpWidth::W8,
            0b0010,
        );
        assert_addsub_carry_lowering(
            "sbb_w16_imm_no_flags_preserves_nzcv",
            true,
            false,
            x(0),
            1,
            0,
            SrcOperand::Imm64(-1),
            u64::MAX,
            OpWidth::W16,
            0b1011,
        );
        assert_addsub_carry_lowering(
            "adc_w16_imm_virtual_dst_sets_flags",
            false,
            true,
            VReg::virt(0),
            1,
            0xffff,
            SrcOperand::Imm(0),
            0,
            OpWidth::W16,
            0b0010,
        );
        assert_addsub_carry_lowering(
            "adc_w8_imm_dst_aliases_src1",
            false,
            false,
            x(1),
            1,
            1,
            SrcOperand::Imm(1),
            1,
            OpWidth::W8,
            0b0000,
        );
    }
    #[test]
    fn lowers_subword_cmp_test_runtime() {
        assert_cmp_lowering(
            "cmp_w8_imm_sets_zero_and_carry",
            1,
            0xff,
            SrcOperand::Imm(0xff),
            0xff,
            OpWidth::W8,
            0b0001,
        );
        assert_cmp_lowering(
            "cmp_w16_reg_sets_borrow_and_negative",
            1,
            0,
            SrcOperand::Reg(x(2)),
            1,
            OpWidth::W16,
            0b0111,
        );
        assert_test_lowering(
            "test_w8_imm_sets_negative_and_clears_cv",
            1,
            0xffff_ffff_ffff_ff80,
            SrcOperand::Imm(0x80),
            0x80,
            OpWidth::W8,
            0b0011,
        );
        assert_test_lowering(
            "test_w16_reg_sets_zero",
            1,
            0x00f0,
            SrcOperand::Reg(x(2)),
            0xff00,
            OpWidth::W16,
            0b1011,
        );
    }
    #[test]
    fn lowers_flag_setting_subword_multiply_runtime() {
        assert_subword_mul_flags_lowering(
            "mulu_w8_zero_sets_zero",
            false,
            0,
            1,
            SrcOperand::Reg(x(2)),
            0,
            0xff,
            OpWidth::W8,
            0b1011,
        );
        assert_subword_mul_flags_lowering(
            "mulu_w8_overflow_sets_carry_and_overflow",
            false,
            0,
            1,
            SrcOperand::Reg(x(2)),
            0xff,
            2,
            OpWidth::W8,
            0b0100,
        );
        assert_subword_mul_flags_lowering(
            "mulu_w16_imm_neg_one_overflows",
            false,
            0,
            1,
            SrcOperand::Imm64(-1),
            2,
            u64::MAX,
            OpWidth::W16,
            0b0001,
        );
        assert_subword_mul_flags_lowering(
            "muls_w8_no_overflow_negative",
            true,
            0,
            1,
            SrcOperand::Reg(x(2)),
            0xfe,
            3,
            OpWidth::W8,
            0b0111,
        );
        assert_subword_mul_flags_lowering(
            "muls_w8_overflow_sets_carry_and_overflow",
            true,
            0,
            1,
            SrcOperand::Reg(x(2)),
            0x7f,
            2,
            OpWidth::W8,
            0b0000,
        );
        assert_subword_mul_flags_lowering(
            "muls_w16_imm_neg_one_no_overflow",
            true,
            0,
            1,
            SrcOperand::Imm64(-1),
            1,
            u64::MAX,
            OpWidth::W16,
            0b0100,
        );
        assert_subword_mul_flags_lowering(
            "muls_w16_min_neg_one_overflows",
            true,
            0,
            1,
            SrcOperand::Imm64(-1),
            0x8000,
            u64::MAX,
            OpWidth::W16,
            0b0010,
        );
        assert_subword_mul_flags_lowering(
            "mulu_w16_dst_aliases_src2",
            false,
            2,
            1,
            SrcOperand::Reg(x(2)),
            0x1234,
            5,
            OpWidth::W16,
            0b1111,
        );
        assert_subword_mul_flags_lowering(
            "muls_w8_dst_aliases_src1",
            true,
            1,
            1,
            SrcOperand::Reg(x(2)),
            0x80,
            1,
            OpWidth::W8,
            0b1000,
        );
    }
    #[test]
    fn lowers_flag_setting_subword_logical_runtime() {
        assert_subword_logic_flags_lowering(
            "and_w8_reg_sets_zero",
            0b00,
            false,
            x(0),
            1,
            SrcOperand::Reg(x(2)),
            0xf0,
            0x0f,
            OpWidth::W8,
            0b1011,
        );
        assert_subword_logic_flags_lowering(
            "and_w8_virtual_dst_sets_zero",
            0b00,
            false,
            VReg::virt(0),
            1,
            SrcOperand::Reg(x(2)),
            0xf0,
            0x0f,
            OpWidth::W8,
            0b0011,
        );
        assert_subword_logic_flags_lowering(
            "andnot_w16_imm_clears_carry_overflow",
            0b00,
            true,
            x(0),
            1,
            SrcOperand::Imm(0x00ff),
            0x12ff,
            0x00ff,
            OpWidth::W16,
            0b1111,
        );
        assert_subword_logic_flags_lowering(
            "or_w8_reg_sets_negative",
            0b01,
            false,
            x(0),
            1,
            SrcOperand::Reg(x(2)),
            0x80,
            0x01,
            OpWidth::W8,
            0b0111,
        );
        assert_subword_logic_flags_lowering(
            "xor_w16_imm_sets_zero",
            0b10,
            false,
            x(0),
            1,
            SrcOperand::Imm(0xffff),
            0xffff,
            0xffff,
            OpWidth::W16,
            0b1001,
        );
        assert_subword_logic_flags_lowering(
            "or_w16_dst_aliases_src2",
            0b01,
            false,
            x(2),
            1,
            SrcOperand::Reg(x(2)),
            0x0100,
            0x8001,
            OpWidth::W16,
            0b0011,
        );
    }
    #[test]
    fn lowers_flag_setting_subword_neg_runtime() {
        assert_subword_neg_flags_lowering(
            "neg_w8_zero_sets_zero_and_carry",
            x(0),
            1,
            0,
            OpWidth::W8,
            0b1001,
        );
        assert_subword_neg_flags_lowering(
            "neg_w8_min_sets_overflow",
            x(0),
            1,
            0x80,
            OpWidth::W8,
            0b0110,
        );
        assert_subword_neg_flags_lowering(
            "neg_w8_virtual_dst_sets_negative",
            VReg::virt(0),
            1,
            1,
            OpWidth::W8,
            0b0111,
        );
        assert_subword_neg_flags_lowering(
            "neg_w16_dst_aliases_src",
            x(1),
            1,
            0x1234,
            OpWidth::W16,
            0b1111,
        );
    }
