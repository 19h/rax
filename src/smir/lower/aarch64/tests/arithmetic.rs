//! tests::arithmetic tests

use super::*;
use crate::smir::lower::aarch64::*;

    #[test]
    fn mem_helper_address_width_selects_w32_wrapping_arithmetic() {
        let address = Address::BaseIndexScale {
            base: Some(x(1)),
            index: x(2),
            scale: 4,
            disp: 8,
            disp_size: DispSize::Auto,
        };

        let mut w32 = Aarch64Lowerer::new();
        w32.set_mem_helper_addr_width(OpWidth::W32);
        w32.emit_mem_helper_addr(&address).expect("W32 address");
        let w32_words: Vec<u32> = w32
            .code
            .as_slice()
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        let mut w64 = Aarch64Lowerer::new();
        w64.emit_mem_helper_addr(&address).expect("W64 address");
        let w64_words: Vec<u32> = w64
            .code
            .as_slice()
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        assert_eq!(w32_words.len(), 4);
        assert_eq!(w64_words.len(), 4);
        assert_eq!(w32_words[0], w64_words[0], "base state load is W64");
        assert_eq!(w32_words[1], w64_words[1], "index state load is W64");
        assert_eq!(
            w32_words[2] ^ w64_words[2],
            1 << 31,
            "scaled addition differs only by the sf width bit"
        );
        assert_eq!(
            w32_words[3] ^ w64_words[3],
            1 << 31,
            "displacement addition differs only by the sf width bit"
        );
    }
    // Regression for issue #9: the mem-offset fusion (Add + Load) must not collapse
    // `add x0, x1, x2; ldr x3, [x0]` to `ldr x3, [x1, x2]` when x0 is architectural —
    // the guest-visible ADD write to x0 must survive.
    #[test]
    fn issue_9_mem_offset_fusion_preserves_arch_add_write() {
        let code = lower_ops(vec![
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Load {
                dst: x(3),
                addr: Address::Direct(x(0)),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        ]);
        let (regs, _, _, _) = run_aarch64_code_with_memory(
            &code,
            &[(1, 0x100), (2, 0x40)],
            0,
            0x140,
            0xCAFE,
            MemWidth::B8,
        );
        assert_eq!(
            regs[0], 0x140,
            "Add must write x0 (mem-offset fusion must not drop it)"
        );
        assert_eq!(regs[3], 0xCAFE, "loaded value");
    }
    #[test]
    fn lowers_mov_x_negative_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Mov {
                dst: x(0),
                src: SrcOperand::Imm(-15),
                width: OpWidth::W64,
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
    fn lowers_add_x_zero_source_reg_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(VReg::Imm(0)),
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
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sub_w_zero_source_reg_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(VReg::Imm(0)),
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
    fn lowers_add_x_zero_imm_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0),
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
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_addsub_x_zero_same_reg_as_noop() {
        let cases = [
            OpKind::Add {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Add {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Sub {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Sub {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm64(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ];

        for kind in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_add_w_zero_same_reg_as_self_mov_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm(0),
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
        expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_x_zero_base_reg_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(1)),
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
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_x_zero_base_same_reg_as_noop() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(0)),
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
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_w_zero_base_same_reg_as_self_mov_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(0)),
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
        expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sub_w_zero_imm_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0),
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
    fn lowers_add_x_zero_base_imm_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0x1234),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x1234, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_w_zero_base_neg_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(-0x34),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0x33, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_x_zero_base_neg_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(-0x34),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0x33, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sub_w_zero_base_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0x34),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0x33, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sub_x_zero_base_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0x34),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0x33, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_zero_base_addsub_register_sources() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(1)),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);

        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(1)),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 1, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);

        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: VReg::virt(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Shifted {
                    reg: x(1),
                    shift: ShiftOp::Lsl,
                    amount: 2,
                },
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 1, 0, 2, 31, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);

        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Shifted {
                    reg: x(1),
                    shift: ShiftOp::Ror,
                    amount: 9,
                },
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
        expected.extend_from_slice(&enc_extract(1, 1, 1, 9).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);

        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Extended {
                    reg: x(1),
                    extend: ExtendOp::Uxtw,
                    shift: 2,
                },
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
        // 0 + uxtw(x1)<<2 == UBFIZ x0, x1, #2, #32 (no SP base).
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 62, 31, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adds_x_zero_base_zero_imm_as_adds_zero_regs() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 1, 0, 0, 0, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_subs_w_zero_base_masked_zero_imm_as_subs_zero_regs() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm64(0x1_0000_0000),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 1, 0, 0, 0, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adds_x_zero_base_nonzero_imm_as_movz_adds_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 1, 0, 0, 0, 31, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_subs_w_zero_base_nonzero_imm_as_movz_subs_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 1, 0, 0, 0, 31, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adds_x_zero_base_positive_imm_without_destination_scratch_as_msr_nzcv_xzr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: VReg::virt(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_msr_sysreg(31, 3, 4, 2, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_subs_w_zero_base_masked_neg_one_imm_without_destination_scratch_as_msr_nzcv_xzr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: VReg::virt(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm64(0xffff_ffff),
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
        expected.extend_from_slice(&enc_msr_sysreg(31, 3, 4, 2, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn rejects_adds_zero_base_negative_imm_without_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: VReg::virt(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(-1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn lowers_add_w8_reg_as_add_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 0, 0, 0, 0, 0, 1, 2).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_w8_zero_base_reg_as_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(1)),
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
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_w16_zero_source_reg_as_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(VReg::Imm(0)),
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
    fn lowers_adds_w_imm_masked_neg_one_as_subs_one() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0xffff_ffff),
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
        expected.extend_from_slice(&enc_addsub_imm_regs(0, 1, 1, 0, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_w16_imm_masked_neg_one_as_sub_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0xffff),
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
        expected.extend_from_slice(&enc_addsub_imm_regs(0, 1, 0, 0, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_add_w16_zero_base_imm_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0x1234),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x1234, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cmp_w_imm_masked_neg_one_as_cmn_one() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cmp {
                src1: x(1),
                src2: SrcOperand::Imm64(0xffff_ffff),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_imm_regs(0, 0, 1, 0, 1, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cmp_x_zero_base_neg_one_imm_as_msr_nzcv_xzr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cmp {
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(-1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_msr_sysreg(31, 3, 4, 2, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cmp_w_zero_base_masked_neg_one_imm_as_msr_nzcv_xzr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cmp {
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm64(0xffff_ffff),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_msr_sysreg(31, 3, 4, 2, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cmp_x_zero_base_positive_imm_as_subs_zr_one() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cmp {
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        // CMP 0, 1 sets NZCV = 0b1000; routed through the ccmp fallback
        // (subs xzr,xzr,xzr; ccmp ...) instead of `cmp sp, #1`, whose Rn = 31
        // is SP and would take the flags from SP - 1.
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 1, 0, 0, 31, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xfa5f_13e8u32.to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sub_w16_large_imm_as_split_sub_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x1234),
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
        expected.extend_from_slice(&enc_addsub_imm_regs(0, 1, 0, 0, 0x234, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_imm_regs(0, 1, 0, 1, 0x1, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sub_w8_zero_base_imm_as_movz_negated() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0x34),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xcc, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sub_w8_zero_source_reg_as_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(VReg::Imm(0)),
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
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adc_w8_reg_as_adc_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 0, 0, 0, 1, 2).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sbb_w16_reg_as_sbc_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 1, 0, 0, 1, 2).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adc_x_zero_base_reg_as_adc_zero_reg_base() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(1)),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(1, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sbcs_w_zero_base_zero_imm_as_sbcs_zero_regs() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: VReg::virt(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 1, 1, 31, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adc_x_zero_imm_as_adc_zero_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(1, 0, 0, 0, 1, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sbcs_w_masked_zero_imm_as_sbcs_zero_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0000_0000),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 1, 1, 0, 1, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adc_w8_masked_zero_imm_as_adc_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x100),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 0, 0, 0, 1, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adc_w8_zero_base_neg_one_imm_alias_as_sbc_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Imm(0xff),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 1, 0, 0, 31, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sbb_w8_neg_one_imm_alias_as_adc_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(-1),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 0, 0, 1, 1, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adc_w16_masked_neg_one_imm_alias_as_sbc_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm64(0xffff),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 1, 0, 1, 1, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sbb_x_neg_one_imm_alias_as_adc_zero_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(-1),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(1, 0, 0, 1, 1, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sbcs_w_masked_neg_one_imm_without_destination_scratch_as_adcs_zero_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: VReg::virt(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0xffff_ffff),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 0, 1, 31, 1, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adc_x_neg_one_imm_alias_as_sbc_zero_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(-1),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(1, 1, 0, 1, 1, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_adcs_w_masked_neg_one_imm_without_destination_scratch_as_sbcs_zero_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Adc {
                dst: VReg::virt(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0xffff_ffff),
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
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 1, 1, 31, 1, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sbcs_w_nonzero_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_carry_regs(0, 1, 1, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_inc_x_as_add_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Inc {
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
        expected.extend_from_slice(&enc_addsub_imm(1, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_dec_w_as_sub_imm_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Dec {
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
        expected.extend_from_slice(&enc_addsub_imm(0, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_inc_w8_as_add_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Inc {
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
        expected.extend_from_slice(&enc_addsub_imm_regs(0, 0, 0, 0, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_inc_w8_zero_as_movz_one() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Inc {
                dst: x(0),
                src: VReg::Imm(0),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_inc_w8_imm_wrap_as_movz_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Inc {
                dst: x(0),
                src: VReg::Imm(0xff),
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
    fn lowers_dec_w16_as_sub_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Dec {
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
        expected.extend_from_slice(&enc_addsub_imm_regs(0, 1, 0, 0, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_dec_w16_zero_as_movz_all_ones() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Dec {
                dst: x(0),
                src: VReg::Imm(0),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xffff, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_dec_w_zero_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Dec {
                dst: x(0),
                src: VReg::Imm(0),
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
    fn lowers_dec_x_zero_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Dec {
                dst: x(0),
                src: VReg::Imm(0),
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
    fn lowers_dec_x_imm_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Dec {
                dst: x(0),
                src: VReg::Imm(17),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 16, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_neg_w8_as_sub_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Neg {
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_neg_w8_zero_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Neg {
                dst: x(0),
                src: VReg::Imm(0),
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
    fn lowers_neg_w8_imm_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Neg {
                dst: x(0),
                src: VReg::Imm(3),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xfd, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_neg_w_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Neg {
                dst: x(0),
                src: VReg::Imm(0x1234),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0x1233, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_neg_x_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Neg {
                dst: x(0),
                src: VReg::Imm(0x1234),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0x1233, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_neg_w16_as_sub_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Neg {
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_w8_as_mul_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp3_regs(0, 0b000, 0, 0, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muls_w16_as_mul_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp3_regs(0, 0b000, 0, 0, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_x_imm_zero_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm(0),
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
    fn lowers_muls_w16_imm_zero_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mul_zero_source_reg_as_movz() {
        let cases = [
            (
                OpKind::MulU {
                    dst_lo: x(0),
                    dst_hi: None,
                    src1: x(1),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                enc_mov_wide(1, 0b10, 0, 0, 0),
            ),
            (
                OpKind::MulS {
                    dst_lo: x(0),
                    dst_hi: None,
                    src1: x(1),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                enc_mov_wide(0, 0b10, 0, 0, 0),
            ),
        ];

        for (kind, movz) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&movz.to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_mulu_w_imm_masked_zero_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0000_0000),
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
    fn lowers_mulu_x_imm_one_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mul_x_imm_one_same_reg_as_noop() {
        let cases = [
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(0),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: None,
                src1: VReg::Imm(1),
                src2: SrcOperand::Reg(x(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ];

        for kind in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_mul_w_imm_one_same_reg_as_self_mov_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(0),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muls_w8_imm_one_as_mov_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm64(1),
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
        expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_w8_imm_masked_one_as_mov_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x101),
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
        expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_x_imm_power_of_two_as_lsl() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm(8),
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
        expected.extend_from_slice(&enc_bitfield(1, 0b10, 61, 60).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_x_left_imm_power_of_two_as_lsl() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: VReg::Imm(8),
                src2: SrcOperand::Reg(x(1)),
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
        expected.extend_from_slice(&enc_bitfield(1, 0b10, 61, 60).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muls_w16_imm_masked_power_of_two_as_lsl_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0008),
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
        expected.extend_from_slice(&enc_bitfield(0, 0b10, 29, 12).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_x_imm_neg_one_as_neg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm(-1),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muls_w16_imm_neg_one_as_neg_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm64(-1),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_w16_imm_masked_neg_one_as_neg_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_ffff),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_x_general_imm_runtime() {
        let src = 0x1020_3040_5060_7080;
        let imm = 0x1234_5678_9abc_def0_u64 as i64;
        let code = lower_single_op(OpKind::MulU {
            dst_lo: x(0),
            dst_hi: None,
            src1: x(1),
            src2: SrcOperand::Imm64(imm),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        });

        let old_nzcv = 0b1010;
        let (out, out_nzcv, sp) =
            run_aarch64_code(&code, &[(1, src), (16, 0x1616_1616_1616_1616)], old_nzcv);

        assert_eq!(out[0], ref_mul(src, imm as u64, false, OpWidth::W64));
        assert_eq!(out[1], src);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_muls_w8_general_imm_runtime() {
        let src = 0xaa55_aa55_aa55_0091;
        let imm = -7;
        let code = lower_single_op(OpKind::MulS {
            dst_lo: x(0),
            dst_hi: None,
            src1: x(16),
            src2: SrcOperand::Imm(imm),
            width: OpWidth::W8,
            flags: FlagUpdate::None,
        });

        let old_nzcv = 0b0101;
        let (out, out_nzcv, sp) =
            run_aarch64_code(&code, &[(16, src), (17, 0x1717_1717_1717_1717)], old_nzcv);

        assert_eq!(out[0], ref_mul(src, imm as u64, true, OpWidth::W8));
        assert_eq!(out[16], src);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_muladd_w8_as_madd_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulAdd {
                dst: x(0),
                acc: x(3),
                src1: x(1),
                src2: x(2),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp3_regs(0, 0b000, 0, 0, 1, 2, 3).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulsub_w16_as_msub_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulSub {
                dst: x(0),
                acc: x(3),
                src1: x(1),
                src2: x(2),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_dp3_regs(0, 0b000, 1, 0, 1, 2, 3).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muladd_x_imm_one_as_add() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulAdd {
                dst: x(0),
                acc: x(3),
                src1: VReg::Imm(1),
                src2: x(1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 0, 0, 0, 3, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulsub_w16_imm_one_as_sub_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulSub {
                dst: x(0),
                acc: x(3),
                src1: x(1),
                src2: VReg::Imm(1),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 3, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muladd_x_imm_neg_one_as_sub() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulAdd {
                dst: x(0),
                acc: x(3),
                src1: VReg::Imm(-1),
                src2: x(1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 3, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulsub_w8_imm_neg_one_as_add_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulSub {
                dst: x(0),
                acc: x(3),
                src1: x(1),
                src2: VReg::Imm(-1),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 0, 0, 0, 0, 0, 3, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulacc_masked_identity_imms_as_addsub() {
        let cases = [
            (
                OpKind::MulAdd {
                    dst: x(0),
                    acc: x(3),
                    src1: VReg::Imm(0x1_0001),
                    src2: x(1),
                    width: OpWidth::W16,
                },
                enc_addsub_shift_regs(0, 0, 0, 0, 0, 0, 3, 1),
                enc_bitfield_regs(0, 0b10, 0, 15, 0, 0),
            ),
            (
                OpKind::MulSub {
                    dst: x(0),
                    acc: x(3),
                    src1: x(1),
                    src2: VReg::Imm(0x1ff),
                    width: OpWidth::W8,
                },
                enc_addsub_shift_regs(0, 0, 0, 0, 0, 0, 3, 1),
                enc_bitfield_regs(0, 0b10, 0, 7, 0, 0),
            ),
        ];

        for (kind, addsub, mask) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&addsub.to_le_bytes());
            expected.extend_from_slice(&mask.to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_muladd_x_imm_zero_as_acc_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulAdd {
                dst: x(0),
                acc: x(3),
                src1: VReg::Imm(0),
                src2: x(1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_reg(1, 0, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulsub_w16_imm_masked_zero_as_acc_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulSub {
                dst: x(0),
                acc: x(3),
                src1: x(1),
                src2: VReg::Imm(0x1_0000),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 3, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muladd_x_two_imms_negative_product_as_sub_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulAdd {
                dst: x(0),
                acc: x(3),
                src1: VReg::Imm(-3),
                src2: VReg::Imm(5),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_imm_regs(1, 1, 0, 0, 15, 0, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulsub_w8_two_imms_wrapping_zero_product_as_acc_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulSub {
                dst: x(0),
                acc: x(3),
                src1: VReg::Imm(16),
                src2: VReg::Imm(16),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 3, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_x_two_imms_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: None,
                src1: VReg::Imm(7),
                src2: SrcOperand::Imm(9),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 63, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muls_w_two_imms_negative_product_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: None,
                src1: VReg::Imm(-3),
                src2: SrcOperand::Imm(5),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0xe, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_full_width_when_low_aliases_src1_as_high_then_low() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(1),
                dst_hi: Some(x(0)),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp3_regs(1, 0b110, 0, 0, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&enc_dp3_regs(1, 0b000, 0, 1, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muls_full_width_when_low_aliases_src2_as_high_then_low() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(2),
                dst_hi: Some(x(0)),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp3_regs(1, 0b010, 0, 0, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&enc_dp3_regs(1, 0b000, 0, 2, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_full_width_when_high_aliases_src1_as_low_then_high() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(0),
                dst_hi: Some(x(1)),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp3_regs(1, 0b000, 0, 0, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&enc_dp3_regs(1, 0b110, 0, 1, 1, 2, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_mulu_full_width_zero_source_when_outputs_alias_sources() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulU {
                dst_lo: x(1),
                dst_hi: Some(x(2)),
                src1: x(1),
                src2: SrcOperand::Reg(VReg::Imm(0)),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 2).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_muls_full_width_two_imms_as_movn_pair() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::MulS {
                dst_lo: x(0),
                dst_hi: Some(x(3)),
                src1: VReg::Imm(-2),
                src2: SrcOperand::Imm64(3),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 5, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_full_width_multiply_when_both_outputs_alias_sources() {
        assert_full_width_mul_lowering(
            "mulu_full_width_outputs_alias_sources",
            false,
            1,
            2,
            1,
            2,
            0xffff_0000_0000_0101,
            0x0002_0000_0000_0011,
        );
        assert_full_width_mul_lowering(
            "muls_full_width_outputs_alias_sources",
            true,
            2,
            1,
            1,
            2,
            0xffff_ffff_ffff_f123,
            0x0000_0000_0000_1357,
        );
    }
    #[test]
    fn lowers_sub64_full_multiply_with_all_architectural_alias_topologies() {
        assert_sub64_full_mul_lowering(
            "mulu_w32_outputs_alias_both_sources",
            false,
            OpWidth::W32,
            1,
            2,
            1,
            2,
            0xffff_ffff,
            0x8000_0003,
        );
        assert_sub64_full_mul_lowering(
            "muls_w32_high_aliases_lhs",
            true,
            OpWidth::W32,
            3,
            1,
            1,
            2,
            0x8000_0001,
            7,
        );
        assert_sub64_full_mul_lowering(
            "mulu_w32_shared_mulx_destination_keeps_high_half",
            false,
            OpWidth::W32,
            1,
            1,
            1,
            2,
            0xffff_fffe,
            0x8000_0003,
        );
        assert_sub64_full_mul_lowering(
            "mulu_w16_source_aliases_implicit_high_destination",
            false,
            OpWidth::W16,
            0,
            2,
            0,
            2,
            0xaaaa_bbbb_cccc_1234,
            0xdddd_eeee_ffff_0003,
        );
        assert_sub64_full_mul_lowering(
            "muls_w16_preserves_both_destination_upper_parts",
            true,
            OpWidth::W16,
            0,
            2,
            0,
            1,
            0xaaaa_bbbb_cccc_fffd,
            0x1111_2222_3333_0004,
        );
    }
    #[test]
    fn lowers_w64_mulx_shared_destination_as_observable_high_half() {
        assert_full_width_mul_lowering(
            "mulu_w64_shared_mulx_destination",
            false,
            1,
            1,
            1,
            2,
            0xffff_ffff_ffff_fffe,
            0x8000_0000_0000_0003,
        );
    }
    #[test]
    fn lowers_full_width_multiply_with_immediate_source() {
        assert_full_width_mul_imm_lowering(
            "mulu_full_width_imm",
            false,
            0,
            3,
            1,
            0xffff_0000_0000_0101,
            SrcOperand::Imm64(0x0002_0000_0000_0011),
            0x0002_0000_0000_0011,
        );
        assert_full_width_mul_imm_lowering(
            "muls_full_width_negative_imm",
            true,
            0,
            3,
            1,
            0xffff_ffff_ffff_f123,
            SrcOperand::Imm(-7),
            (-7_i64) as u64,
        );
        assert_full_width_mul_imm_lowering(
            "mulu_full_width_imm_low_aliases_src1",
            false,
            1,
            0,
            1,
            0x1234_5678_9abc_def0,
            SrcOperand::Imm64(0x12345),
            0x12345,
        );
        assert_full_width_mul_imm_lowering(
            "muls_full_width_imm_high_aliases_src1",
            true,
            0,
            1,
            1,
            0xffff_ffff_ffff_f123,
            SrcOperand::Imm(-3),
            (-3_i64) as u64,
        );
    }
    #[test]
    fn lowers_divu_x_with_remainder_as_udiv_msub() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: Some(x(3)),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp2_regs(1, 0b0010, 1, 2, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp3_regs(1, 0b000, 1, 3, 0, 2, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w_with_remainder_as_sdiv_msub_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: Some(x(3)),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp2_regs(0, 0b0011, 1, 2, 0).to_le_bytes());
        expected.extend_from_slice(&enc_dp3_regs(0, 0b000, 1, 3, 0, 2, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_x_with_remainder_when_quotient_aliases_dividend() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(1),
                rem: Some(x(3)),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp2_regs(1, 0b0010, 1, 2, 3).to_le_bytes());
        expected.extend_from_slice(&enc_dp3_regs(1, 0b000, 1, 3, 3, 2, 1).to_le_bytes());
        expected.extend_from_slice(&enc_dp2_regs(1, 0b0010, 1, 2, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w_with_remainder_when_quotient_aliases_divisor() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(2),
                rem: Some(x(3)),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_dp2_regs(0, 0b0011, 1, 2, 3).to_le_bytes());
        expected.extend_from_slice(&enc_dp3_regs(0, 0b000, 1, 3, 3, 2, 1).to_le_bytes());
        expected.extend_from_slice(&enc_dp2_regs(0, 0b0011, 1, 2, 2).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_x_imm_one_as_mov() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_w16_imm_masked_one_as_mov_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0001),
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
        expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_x_two_imms_as_mov_quot_rem() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: Some(x(3)),
                src1: VReg::Imm(100),
                src2: SrcOperand::Imm(7),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 14, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 2, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_x_two_imms_all_ones_quot_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: VReg::Imm(-1),
                src2: SrcOperand::Imm(1),
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
    fn lowers_divu_w8_two_masked_imms_as_mov_quot() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: VReg::Imm(0x123),
                src2: SrcOperand::Imm(6),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 5, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w8_two_imms_as_mov_quot_rem() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: Some(x(3)),
                src1: VReg::Imm(0xf6),
                src2: SrcOperand::Imm(3),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xfd, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xff, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_x_two_imms_negative_quot_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: None,
                src1: VReg::Imm(-15),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0xe, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn rejects_divs_w8_two_imms_overflow() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: None,
                src1: VReg::Imm(0x80),
                src2: SrcOperand::Imm(0xff),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn lowers_divs_w_imm_one_with_remainder_as_mov_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(3),
                rem: Some(x(0)),
                src1: x(1),
                src2: SrcOperand::Imm64(1),
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
        expected.extend_from_slice(&enc_mov_reg(0, 3, 1).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w_imm_masked_one_with_remainder_as_mov_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(3),
                rem: Some(x(0)),
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0000_0001),
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
        expected.extend_from_slice(&enc_mov_reg(0, 3, 1).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_x_imm_neg_one_as_neg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm(-1),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w_imm_masked_neg_one_as_neg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(3),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_ffff_ffff),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 3, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w16_imm_masked_neg_one_as_neg_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(3),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_ffff),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 3, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 3, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_x_imm_neg_one_with_remainder_as_neg_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: Some(x(3)),
                src1: x(1),
                src2: SrcOperand::Imm(-1),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w16_imm_neg_one_with_remainder_aliasing_source() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(3),
                rem: Some(x(1)),
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_ffff),
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
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 3, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 3, 3).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn rejects_divs_imm_neg_one_when_quotient_overlaps_remainder() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: Some(x(0)),
                src1: x(1),
                src2: SrcOperand::Imm(-1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn lowers_divu_x_imm_power_of_two_as_lsr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm(8),
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
        expected.extend_from_slice(&enc_bitfield(1, 0b10, 3, 63).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_w_imm_masked_power_of_two_as_lsr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_8000_0000),
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
        expected.extend_from_slice(&enc_bitfield(0, 0b10, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_w8_imm_power_of_two_as_lsr_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm(4),
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
        expected.extend_from_slice(&enc_bitfield(0, 0b10, 2, 7).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_w16_imm_masked_power_of_two_as_lsr_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(3),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0000_8000),
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
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 15, 15, 1, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_w_imm_power_of_two_remainder_before_aliasing_quotient() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(1),
                rem: Some(x(3)),
                src1: x(1),
                src2: SrcOperand::Imm64(32),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 0, 4, 3, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 5, 31, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_x_imm_power_of_two_remainder_after_aliasing_dividend() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: Some(x(1)),
                src1: x(1),
                src2: SrcOperand::Imm(16),
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
        expected.extend_from_slice(&enc_bitfield(1, 0b10, 4, 63).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 0, 3, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_w16_imm_power_of_two_remainder_before_aliasing_quotient() {
        assert_div_runtime_lowering(
            "divu_w16_imm_power_of_two_remainder_before_aliasing_quotient",
            false,
            1,
            Some(3),
            1,
            SrcOperand::Imm64(0x1_0000_0080),
            None,
            0x80ff,
            0x80,
            OpWidth::W16,
        );
    }
    #[test]
    fn lowers_divs_x_imm_power_of_two_as_bias_asr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm(8),
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
        expected.extend_from_slice(&enc_bitfield(1, 0b00, 63, 63).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 1, 61, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b00, 3, 63, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w_imm_masked_power_of_two_as_bias_asr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(3),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0000_0010),
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
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b00, 31, 31, 1, 3).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 0, 0, 1, 28, 3, 1, 3).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b00, 4, 31, 3, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_x_imm_power_of_two_in_place_as_bias_asr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(1),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm(8),
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
        expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b00, 63, 63, 1, 16).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 1, 61, 1, 1, 16).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(1, 0b00, 3, 63, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w_imm_power_of_two_in_place_as_bias_asr() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(1),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm64(0x1_0000_0010),
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
        expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b00, 31, 31, 1, 16).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_shift_regs(0, 0, 0, 1, 28, 1, 1, 16).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b00, 4, 31, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_imm_power_of_two_when_quotient_aliases_remainder() {
        assert_div_w64_lowering(
            "divu_imm_power_of_two_quot_rem_alias",
            false,
            0,
            Some(0),
            1,
            SrcOperand::Imm(8),
            None,
            0xfedc_ba98_7654_3217,
            8,
            FlagUpdate::None,
        );
    }
    #[test]
    fn executes_divu_x_imm_power_of_two_with_remainder() {
        assert_div_w64_lowering(
            "divu_imm_power_of_two_runtime",
            false,
            0,
            Some(3),
            1,
            SrcOperand::Imm(32),
            None,
            0x1234_5678_9abc_def0,
            32,
            FlagUpdate::None,
        );
    }
    #[test]
    fn lowers_divu_w8_imm_one_as_mov_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: None,
                src1: x(1),
                src2: SrcOperand::Imm(1),
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
        expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divs_w16_imm_one_with_remainder_as_mov_uxth_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivS {
                quot: x(3),
                rem: Some(x(0)),
                src1: x(1),
                src2: SrcOperand::Imm64(1),
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
        expected.extend_from_slice(&enc_mov_reg(0, 3, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 3, 3).to_le_bytes());
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_vmultiply_add52_v128_runtime() {
        const MASK52: u64 = 0x000f_ffff_ffff_ffff;

        fn ref_ifma52_lane(acc: u64, lhs: u64, rhs: u64, high: bool) -> u64 {
            let product = ((lhs & MASK52) as u128) * ((rhs & MASK52) as u128);
            let addend = if high {
                ((product >> 52) & MASK52 as u128) as u64
            } else {
                (product & MASK52 as u128) as u64
            };
            acc.wrapping_add(addend)
        }

        fn ref_ifma52(acc: (u64, u64), lhs: (u64, u64), rhs: (u64, u64), high: bool) -> (u64, u64) {
            (
                ref_ifma52_lane(acc.0, lhs.0, rhs.0, high),
                ref_ifma52_lane(acc.1, lhs.1, rhs.1, high),
            )
        }

        let acc_low = (0xffff_ffff_ffff_ff00, 0x0102_0304_0506_0708);
        let acc_high = (0x1111_2222_3333_4444, 0xffff_0000_ffff_0001);
        let alias_acc = (0x4444_3333_2222_1111, 0x8080_7070_6060_5050);
        let lhs = (0xffff_ffff_ffff_ffff, 0x000f_edcb_a987_6543);
        let rhs = (0x0012_3456_789a_bcde, 0x000f_ffff_ffff_ffff);
        let alias_rhs = (0x000f_0000_0000_0003, 0x0000_ffff_ffff_fff1);
        let code = lower_ops(vec![
            OpKind::VMultiplyAdd52 {
                dst: v(0),
                acc: v(1),
                src1: v(2),
                src2: v(3),
                mask: None,
                width: VecWidth::V128,
                high: false,
                zeroing: false,
            },
            OpKind::VMultiplyAdd52 {
                dst: v(4),
                acc: v(5),
                src1: v(2),
                src2: v(3),
                mask: None,
                width: VecWidth::V128,
                high: true,
                zeroing: false,
            },
            OpKind::VMultiplyAdd52 {
                dst: v(6),
                acc: v(6),
                src1: v(6),
                src2: v(7),
                mask: None,
                width: VecWidth::V128,
                high: true,
                zeroing: false,
            },
        ]);

        let (regs, simd, sp) = run_aarch64_code_with_regs_and_simd(
            &code,
            &[
                (13, 0x1313_1313_1313_1313),
                (14, 0x1414_1414_1414_1414),
                (15, 0x1515_1515_1515_1515),
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
            ],
            &[
                (1, acc_low.0, acc_low.1),
                (2, lhs.0, lhs.1),
                (3, rhs.0, rhs.1),
                (5, acc_high.0, acc_high.1),
                (6, alias_acc.0, alias_acc.1),
                (7, alias_rhs.0, alias_rhs.1),
            ],
        );

        assert_eq!(simd[0], ref_ifma52(acc_low, lhs, rhs, false));
        assert_eq!(simd[4], ref_ifma52(acc_high, lhs, rhs, true));
        assert_eq!(simd[6], ref_ifma52(alias_acc, alias_acc, alias_rhs, true));
        assert_eq!(simd[1], acc_low);
        assert_eq!(simd[2], lhs);
        assert_eq!(simd[3], rhs);
        assert_eq!(regs[13], 0x1313_1313_1313_1313);
        assert_eq!(regs[14], 0x1414_1414_1414_1414);
        assert_eq!(regs[15], 0x1515_1515_1515_1515);
        assert_eq!(regs[16], 0x1616_1616_1616_1616);
        assert_eq!(regs[17], 0x1717_1717_1717_1717);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_lea_direct_as_add_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Lea {
                dst: x(0),
                addr: Address::Direct(x(1)),
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_imm_regs(1, 0, 0, 0, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_lea_base_positive_offset_as_add_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Lea {
                dst: x(0),
                addr: Address::BaseOffset {
                    base: x(1),
                    offset: 0x123,
                    disp_size: DispSize::Auto,
                },
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_imm_regs(1, 0, 0, 0, 0x123, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_lea_base_negative_offset_as_sub_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Lea {
                dst: x(0),
                addr: Address::BaseOffset {
                    base: x(1),
                    offset: -0x2000,
                    disp_size: DispSize::Auto,
                },
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_addsub_imm_regs(1, 1, 0, 1, 2, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_lea_absolute_negative_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Lea {
                dst: x(0),
                addr: Address::Absolute(0xffff_ffff_ffff_fff1),
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
    fn lowers_lea_pcrel_negative_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Lea {
                dst: x(0),
                addr: Address::PcRel {
                    offset: -15,
                    disp_size: DispSize::Auto,
                    base: Some(0),
                },
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
    fn lowers_sign_extend_w8_imm_src_negative_to_x_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::SignExtend {
                dst: x(0),
                src: VReg::Imm(0xff),
                from_width: OpWidth::W8,
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
    fn lowers_sign_extend_w8_imm_src_negative_to_w16_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::SignExtend {
                dst: x(0),
                src: VReg::Imm(0x80),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xff80, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cmove_x_always_negative_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::CMove {
                dst: x(0),
                src: VReg::Imm(-15),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0xe, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cmove_w_imm_negative_as_movn_with_false_path_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::CMove {
                dst: x(0),
                src: VReg::Imm(-15),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0xe, 0).to_le_bytes());
        expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_flag_setting_inc_dec_runtime() {
        assert_inc_dec_flags_lowering(
            "inc_x_preserves_set_c_and_sets_overflow",
            false,
            0,
            1,
            0x7fff_ffff_ffff_ffff,
            OpWidth::W64,
            0b0010,
        );
        assert_inc_dec_flags_lowering(
            "dec_x_preserves_clear_c_and_sets_overflow",
            true,
            0,
            1,
            0x8000_0000_0000_0000,
            OpWidth::W64,
            0b1101,
        );
        assert_inc_dec_flags_lowering(
            "inc_w_sets_zero_and_preserves_c",
            false,
            0,
            1,
            0xffff_ffff,
            OpWidth::W32,
            0b1010,
        );
        assert_inc_dec_flags_lowering(
            "dec_w_sets_negative_and_preserves_clear_c",
            true,
            0,
            1,
            0,
            OpWidth::W32,
            0b0101,
        );
        assert_inc_dec_flags_lowering(
            "inc_x_dst_aliases_src_flags",
            false,
            1,
            1,
            41,
            OpWidth::W64,
            0b0010,
        );
        assert_inc_dec_flags_lowering(
            "inc_w8_sets_overflow_and_preserves_clear_c",
            false,
            0,
            1,
            0x7f,
            OpWidth::W8,
            0b0000,
        );
        assert_inc_dec_flags_lowering(
            "inc_w8_sets_zero_and_preserves_set_c",
            false,
            0,
            1,
            0xff,
            OpWidth::W8,
            0b0010,
        );
        assert_inc_dec_flags_lowering(
            "dec_w16_sets_overflow_and_preserves_set_c",
            true,
            0,
            1,
            0x8000,
            OpWidth::W16,
            0b0010,
        );
        assert_inc_dec_flags_lowering(
            "dec_w16_dst_aliases_src_preserves_clear_c",
            true,
            1,
            1,
            0,
            OpWidth::W16,
            0b0000,
        );
    }
    #[test]
    fn lowers_full_width_addsub_carry_with_immediate_source_runtime() {
        assert_addsub_carry_lowering(
            "adc_x_imm_carry_in_wraps_to_zero",
            false,
            true,
            x(0),
            1,
            u64::MAX,
            SrcOperand::Imm64(0),
            0,
            OpWidth::W64,
            0b0010,
        );
        assert_addsub_carry_lowering(
            "sbb_w_imm_uses_borrow",
            true,
            true,
            x(0),
            1,
            0,
            SrcOperand::Imm(1),
            1,
            OpWidth::W32,
            0b0000,
        );
        assert_addsub_carry_lowering(
            "adc_w_imm_no_flags_preserves_nzcv",
            false,
            false,
            x(1),
            1,
            0x7fff_ffff,
            SrcOperand::Imm(2),
            2,
            OpWidth::W32,
            0b1001,
        );
        assert_addsub_carry_lowering(
            "sbb_x_imm_neg_one_virtual_dst_sets_flags",
            true,
            true,
            VReg::virt(0),
            1,
            0x10,
            SrcOperand::Imm64(-1),
            u64::MAX,
            OpWidth::W64,
            0b0010,
        );
        assert_addsub_carry_lowering(
            "adc_x_imm_sign_bit_sets_negative",
            false,
            true,
            x(0),
            1,
            0,
            SrcOperand::Imm64(i64::MIN),
            0x8000_0000_0000_0000,
            OpWidth::W64,
            0b0000,
        );
    }
    #[test]
    fn lowers_x86_w16_single_result_signed_multiply_partial_write_alias_matrix() {
        let reg = |index: u8| match index {
            0 => x86(X86Reg::Rax),
            1 => x86(X86Reg::Rcx),
            2 => x86(X86Reg::Rdx),
            3 => x86(X86Reg::Rbx),
            _ => unreachable!("unexpected test register x{index}"),
        };
        let initial = [
            0xaaaa_bbbb_cccc_fffe,
            0x1111_2222_3333_7fff,
            0xdddd_eeee_ffff_0002,
            0xbbbb_cccc_dddd_0003,
        ];
        let cases = [
            (
                "destructive-reg-nf",
                0,
                0,
                SrcOperand::Reg(reg(3)),
                initial[3],
                FlagUpdate::None,
            ),
            (
                "ndd-dst-aliases-src2-nf",
                3,
                0,
                SrcOperand::Reg(reg(3)),
                initial[3],
                FlagUpdate::None,
            ),
            (
                "independent-imm16-nf",
                1,
                2,
                SrcOperand::Imm(0x1234),
                0x1234,
                FlagUpdate::None,
            ),
            (
                "destructive-reg-flags",
                0,
                0,
                SrcOperand::Reg(reg(3)),
                initial[3],
                FlagUpdate::All,
            ),
            (
                "destructive-imm-overflow-flags",
                1,
                1,
                SrcOperand::Imm(2),
                2,
                FlagUpdate::All,
            ),
        ];
        let sentinels = [
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
            (14, 0x1414_1414_1414_1414),
            (13, 0x1313_1313_1313_1313),
            (12, 0x1212_1212_1212_1212),
        ];

        for (label, dst, src1, src2, src2_value, flags) in cases {
            let code = lower_single_op(OpKind::MulS {
                dst_lo: reg(dst),
                dst_hi: None,
                src1: reg(src1),
                src2,
                width: OpWidth::W16,
                flags,
            });
            let low = ref_mul(initial[src1 as usize], src2_value, true, OpWidth::W16);
            let expected = (initial[dst as usize] & !0xffff) | low;
            let old_nzcv = 0b1011;
            let expected_nzcv = if flags.updates_any() {
                expected_mul_nzcv(initial[src1 as usize], src2_value, true, OpWidth::W16)
            } else {
                old_nzcv
            };
            let mut regs = sentinels.to_vec();
            regs.extend(
                initial
                    .iter()
                    .enumerate()
                    .map(|(index, value)| (index as u8, *value)),
            );
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

            assert_eq!(out[dst as usize], expected, "{label}: result");
            for index in 0..4 {
                if index != dst {
                    assert_eq!(
                        out[index as usize], initial[index as usize],
                        "{label}: x{index} preserved"
                    );
                }
            }
            for (index, value) in sentinels {
                assert_eq!(out[index as usize], value, "{label}: x{index} scratch");
            }
            assert_eq!(out_nzcv, expected_nzcv, "{label}: NZCV");
            assert_eq!(sp, 0x8000, "{label}: stack");
        }
    }
    #[test]
    fn rejects_lea_gprel_address() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Lea {
                dst: x(0),
                addr: Address::GpRel { offset: 4 },
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
