//! tests::logic tests

use super::*;
use crate::smir::lower::aarch64::*;

    #[test]
    fn mem_helper_w32_absolute_address_is_materialized_and_range_checked() {
        let mut lowerer = Aarch64Lowerer::new();
        lowerer.set_mem_helper_addr_width(OpWidth::W32);
        lowerer
            .emit_mem_helper_addr(&Address::Absolute(0xfedc_ba98))
            .expect("bounded W32 absolute address");
        let words: Vec<u32> = lowerer
            .code
            .as_slice()
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(
            words.len(),
            2,
            "MOVZ+MOVK materialize the complete W32 address"
        );
        assert!(
            words.iter().all(|word| word & 0x1f == 1),
            "address is written to W1"
        );

        let mut out_of_range = Aarch64Lowerer::new();
        out_of_range.set_mem_helper_addr_width(OpWidth::W32);
        assert!(matches!(
            out_of_range.emit_mem_helper_addr(&Address::Absolute(u64::from(u32::MAX) + 1)),
            Err(LowerError::InvalidOperand { .. })
        ));
        assert!(out_of_range.code.is_empty());
    }
    // Regression for issue #15: a W8 CMove must leave the FULL destination
    // unchanged on the false path and MERGE the low byte (preserving the upper 56
    // bits) on the true path for an x86 destination — not CSEL+UXTB, which zeroed
    // the upper bits and truncated even when the condition was false.
    #[test]
    fn issue_15_w8_cmove_preserves_x86_dst_on_false_and_merges_on_true() {
        let dst = x86(X86Reg::Rax);
        let src = x86(X86Reg::Rcx);
        let host_dst = Aarch64Lowerer::gpr_arm_or_x86(dst).unwrap();
        let host_src = Aarch64Lowerer::gpr_arm_or_x86(src).unwrap();
        assert_ne!(host_dst, host_src);

        let code = lower_single_op(OpKind::CMove {
            dst,
            src,
            cond: Condition::Eq,
            width: OpWidth::W8,
        });

        let sentinel = 0xBBBB_CCCC_DDDD_EE7Fu64;
        // ZF clear -> Eq false -> RAX must be FULLY unchanged.
        let (regs_false, _, _) =
            run_aarch64_code(&code, &[(host_dst, sentinel), (host_src, 0x12)], 0b0000);
        assert_eq!(
            regs_false[host_dst as usize], sentinel,
            "false W8 CMove must leave the destination fully unchanged",
        );
        // ZF set (0b0100) -> Eq true -> low byte = src low byte, upper bits kept.
        let (regs_true, _, _) =
            run_aarch64_code(&code, &[(host_dst, sentinel), (host_src, 0x12)], 0b0100);
        assert_eq!(
            regs_true[host_dst as usize], 0xBBBB_CCCC_DDDD_EE12,
            "true W8 CMove merges the low byte and preserves the upper bits",
        );
    }
    #[test]
    fn lowers_x86_sbb_borrow_normalization_complete_width_matrix() {
        const DST_UPPER: u64 = 0xAAAA_BBBB_CCCC_0000;
        const SRC_UPPER: u64 = 0x1111_2222_3333_0000;
        const SCRATCH16: u64 = 0x1616_1616_1616_1616;
        const SCRATCH17: u64 = 0x1717_1717_1717_1717;

        for flagm in [false, true] {
            for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                for borrow_in in [false, true] {
                    for immediate in [false, true] {
                        let mask = width.mask();
                        let dst_initial = (DST_UPPER & !mask) | 0;
                        let src_value = (SRC_UPPER & !mask) | 1;
                        let src2 = if immediate {
                            SrcOperand::Imm64(1)
                        } else {
                            SrcOperand::Reg(x86(X86Reg::Rcx))
                        };
                        let code = lower_ops_with_flagm_features(
                            vec![OpKind::Sbb {
                                dst: x86(X86Reg::Rax),
                                src1: x86(X86Reg::Rax),
                                src2,
                                width,
                                flags: FlagUpdate::None,
                            }],
                            flagm,
                            false,
                        );
                        let initial_nzcv = 0b1101 | (u8::from(borrow_in) << 1);
                        let result = ref_x86_sbb(0, 1, borrow_in, width);
                        let expected = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                            (dst_initial & !mask) | result
                        } else {
                            result
                        };
                        let (out, nzcv, sp) = run_aarch64_code(
                            &code,
                            &[
                                (0, dst_initial),
                                (1, src_value),
                                (16, SCRATCH16),
                                (17, SCRATCH17),
                            ],
                            initial_nzcv,
                        );

                        assert_eq!(
                            out[0], expected,
                            "SBB {width:?} flagm={flagm} borrow={borrow_in} immediate={immediate} result"
                        );
                        assert_eq!(out[1], src_value, "SBB {width:?} source");
                        assert_eq!(out[16], SCRATCH16, "SBB {width:?} x16 scratch");
                        assert_eq!(out[17], SCRATCH17, "SBB {width:?} x17 scratch");
                        assert_eq!(
                            nzcv, initial_nzcv,
                            "no-flag SBB {width:?} must preserve canonical x86 flags"
                        );
                        assert_eq!(sp, 0x8000, "SBB {width:?} stack");
                    }
                }
            }
        }
    }
    #[test]
    fn lowers_add_register_and_ret() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(0),
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

        assert_eq!(code, [0x20, 0x00, 0x02, 0x8b, 0xc0, 0x03, 0x5f, 0xd6]);
    }
    #[test]
    fn lowers_addsub_w_masked_zero_imm_as_mov_or_self_mov() {
        let cases = [
            (
                OpKind::Add {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Imm64(0x1_0000_0000),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                enc_mov_reg(0, 0, 0),
            ),
            (
                OpKind::Sub {
                    dst: x(3),
                    src1: x(1),
                    src2: SrcOperand::Imm64(0x1_0000_0000),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                enc_mov_reg(0, 3, 1),
            ),
        ];

        for (kind, expected_insn) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&expected_insn.to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_div_one_same_reg_as_noop_or_zero_ext() {
        let div_cases = [
            (
                OpKind::DivU {
                    quot: x(0),
                    rem: None,
                    src1: x(0),
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::DivS {
                    quot: x(0),
                    rem: None,
                    src1: x(0),
                    src2: SrcOperand::Imm64(1),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::DivU {
                    quot: x(0),
                    rem: None,
                    src1: x(0),
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W8,
                    flags: FlagUpdate::None,
                },
                vec![enc_bitfield_regs(0, 0b10, 0, 7, 0, 0), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in div_cases {
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
    fn lowers_divu_x_imm_power_of_two_with_remainder_as_lsr_and() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::DivU {
                quot: x(0),
                rem: Some(x(3)),
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
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 0, 2, 3, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_divu_w8_imm_power_of_two_with_remainder_as_lsr_and() {
        assert_div_runtime_lowering(
            "divu_w8_imm_power_of_two_with_remainder",
            false,
            0,
            Some(3),
            1,
            SrcOperand::Imm(8),
            None,
            0xff,
            8,
            OpWidth::W8,
        );
    }
    #[test]
    fn lowers_vpermute_v128_alias_and_two_table_encodings() {
        fn append_masked_index_build(expected: &mut Vec<u32>, dst: u32, indices: u32, mask: i64) {
            let (imm_n, immr, imms) =
                Aarch64Lowerer::logical_bitmask_imm(mask, OpWidth::W32).unwrap();
            expected.push(enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31));
            for lane in 0..16 {
                let imm5 = (lane << 1) | 1;
                expected.push(enc_simd_umov(16, indices, imm5, false));
                expected.push(enc_logical_imm(0, 0b00, imm_n, immr, imms, 16, 16));
                expected.push(enc_simd_ins_general(dst, 16, imm5));
            }
            expected.push(enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31));
        }

        let alias_words = code_words(&lower_single_op(OpKind::VPermute {
            dst: v(1),
            src1: v(1),
            src2: None,
            indices: v(2),
            elem: VecElementType::I8,
            width: VecWidth::V128,
            overwrite_table: false,
        }));
        let mut alias_expected = vec![
            enc_simd_ldst_simm_regs(0, 0b10, 0b11, -16, 31, 31),
            enc_simd_orr(31, 1, 1),
        ];
        append_masked_index_build(&mut alias_expected, 1, 2, 0x0f);
        alias_expected.push(enc_simd_tbl(1, 31, 1, 1, 0, 0));
        alias_expected.push(enc_simd_ldst_simm_regs(0, 0b11, 0b01, 16, 31, 31));
        alias_expected.push(0xd65f_03c0);
        assert_eq!(alias_words, alias_expected);

        let two_table_words = code_words(&lower_single_op(OpKind::VPermute {
            dst: v(0),
            src1: v(1),
            src2: Some(v(2)),
            indices: v(3),
            elem: VecElementType::I8,
            width: VecWidth::V128,
            overwrite_table: false,
        }));
        let mut two_table_expected = vec![
            enc_simd_ldst_simm_regs(0, 0b10, 0b11, -16, 30, 31),
            enc_simd_ldst_simm_regs(0, 0b10, 0b11, -16, 31, 31),
            enc_simd_orr(30, 1, 1),
            enc_simd_orr(31, 2, 2),
        ];
        append_masked_index_build(&mut two_table_expected, 0, 3, 0x1f);
        two_table_expected.push(enc_simd_tbl(0, 30, 0, 1, 1, 0));
        two_table_expected.push(enc_simd_ldst_simm_regs(0, 0b11, 0b01, 16, 31, 31));
        two_table_expected.push(enc_simd_ldst_simm_regs(0, 0b11, 0b01, 16, 30, 31));
        two_table_expected.push(0xd65f_03c0);
        assert_eq!(two_table_words, two_table_expected);
    }
    #[test]
    fn lowers_xchg_x_as_eor_swap() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xchg {
                reg1: x(0),
                reg2: x(1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_reg(1, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(1, 0b10, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(1, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_xchg_w_as_eor_swap_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xchg {
                reg1: x(0),
                reg2: x(1),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_xchg_w16_as_eor_swap_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xchg {
                reg1: x(0),
                reg2: x(1),
                width: OpWidth::W16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_xchg_w8_as_eor_swap_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xchg {
                reg1: x(0),
                reg2: x(1),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b10, 0, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    // Regression for issue #35: a large LEA displacement that does not fit an
    // add/sub immediate takes the scratch path, which must NOT spill to the guest
    // stack ([SP-16]) — that corrupts the word below SP and can fault on an
    // unmapped/misaligned SP. The result must be correct, SP unchanged, and the
    // word below SP untouched.
    #[test]
    fn issue_35_large_disp_lea_does_not_touch_stack() {
        let code = lower_ops(vec![OpKind::Lea {
            dst: x(0),
            addr: Address::BaseOffset {
                base: x(1),
                offset: 0x1_2345,
                disp_size: DispSize::Auto,
            },
        }]);
        let sentinel = 0xDEAD_BEEF_CAFE_F00Du64;
        // SP = 0x8000 in the harness; the buggy stack spill targets [SP-16] = 0x7ff0.
        let (regs, _, sp, below_sp) =
            run_aarch64_code_with_memory(&code, &[(1, 0x1000)], 0, 0x7ff0, sentinel, MemWidth::B8);
        assert_eq!(
            regs[0],
            0x1000 + 0x1_2345,
            "LEA result must be base + displacement"
        );
        assert_eq!(sp, 0x8000, "LEA must not modify SP");
        assert_eq!(below_sp, sentinel, "LEA must not write the word below SP");
    }
    #[test]
    fn lowers_cas_with_split_compare_and_destination() {
        assert_cas_lowering(
            "cas_split_destination",
            3,
            None,
            2,
            0,
            MemWidth::B8,
            0x1111_2222_3333_4444,
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
        );
        assert_cas_lowering(
            "cas_split_observable_byte_masks_expected",
            3,
            Some(4),
            2,
            0,
            MemWidth::B1,
            0x7f,
            0x1234_5678_9abc_de7f,
            0xaa,
        );
        assert_cas_lowering(
            "cas_split_success_aliases_destination",
            3,
            Some(3),
            2,
            0,
            MemWidth::B8,
            0x1111_2222_3333_4444,
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
        );
    }
    #[test]
    fn lowers_bfx_full_width_as_mov_or_noop() {
        let bfx_cases = [
            (
                OpKind::Bfx {
                    dst: x(0),
                    src: x(0),
                    lsb: 0,
                    width_bits: 64,
                    sign_extend: false,
                    op_width: OpWidth::W64,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::Bfx {
                    dst: x(0),
                    src: x(0),
                    lsb: 0,
                    width_bits: 32,
                    sign_extend: true,
                    op_width: OpWidth::W32,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Bfx {
                    dst: x(0),
                    src: x(1),
                    lsb: 0,
                    width_bits: 64,
                    sign_extend: true,
                    op_width: OpWidth::W64,
                },
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in bfx_cases {
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
    fn lowers_x86_bls_widths_flags_aliases_and_preserves_scratch_state() {
        let cases = [
            (X86BlsKind::Blsr, OpWidth::W64, 0_u64, 0b1011_u8),
            (X86BlsKind::Blsmsk, OpWidth::W32, 0_u64, 0b0101_u8),
            (
                X86BlsKind::Blsi,
                OpWidth::W64,
                0x8000_0000_0000_0000,
                0b0100_u8,
            ),
            (X86BlsKind::Blsr, OpWidth::W32, 0x8000_0018, 0b1111_u8),
            (X86BlsKind::Blsmsk, OpWidth::W64, 0x1200, 0b0001_u8),
            (X86BlsKind::Blsi, OpWidth::W32, 0x8000_0018, 0b1100_u8),
        ];
        for (kind, width, source, old_nzcv) in cases {
            let code = lower_single_op(OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                width,
                kind,
                flags: bls_flags(),
            });
            let mask = width.mask();
            let source = source & mask;
            let result = match kind {
                X86BlsKind::Blsr => source & source.wrapping_sub(1),
                X86BlsKind::Blsmsk => source ^ source.wrapping_sub(1),
                X86BlsKind::Blsi => source.wrapping_neg() & source,
            } & mask;
            let carry = match kind {
                X86BlsKind::Blsr | X86BlsKind::Blsmsk => source == 0,
                X86BlsKind::Blsi => source != 0,
            };
            let expected_nzcv = ((((result & width.sign_bit()) != 0) as u8) << 3)
                | (((result == 0) as u8) << 2)
                | ((carry as u8) << 1);
            let sentinels = [
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
                (15, 0x1515_1515_1515_1515),
            ];
            let mut regs = sentinels.to_vec();
            regs.push((1, source));
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
            assert_eq!(out[0], result, "{kind:?} {width:?} result");
            assert_eq!(out[1], source, "{kind:?} {width:?} source preserved");
            assert_eq!(out_nzcv, expected_nzcv, "{kind:?} {width:?} NZCV");
            assert_eq!(sp, 0x8000, "{kind:?} {width:?} stack restored");
            for (reg, value) in sentinels {
                assert_eq!(out[reg as usize], value, "{kind:?} restored x{reg}");
            }
        }

        let code = lower_ops_with_flagm_features(
            vec![OpKind::X86Bls {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsi,
                flags: bls_flags(),
            }],
            false,
            false,
        );
        assert!(!code_has_flagm(&code, 0b000));
        let (out, out_nzcv, sp) = run_aarch64_code(
            &code,
            &[
                (1, 0x18),
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
            ],
            0b1101,
        );
        assert_eq!(out[0], 0x8, "baseline-AArch64 BLSI result");
        assert_eq!(out_nzcv, 0b0010, "baseline-AArch64 BLSI flags");
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(sp, 0x8000);

        let old_nzcv = 0b1011;
        let code = lower_single_op(OpKind::X86Bls {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            kind: X86BlsKind::Blsi,
            flags: FlagUpdate::None,
        });
        let (out, out_nzcv, sp) = run_aarch64_code(
            &code,
            &[
                (0, 0x18),
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
            ],
            old_nzcv,
        );
        assert_eq!(out[0], 0x8, "aliased NF BLSI result");
        assert_eq!(out_nzcv, old_nzcv, "NF BLSI preserves every NZCV bit");
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_orr_x_low_mask_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x3f),
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
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 0, 5, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_orr_x_wrapping_mask_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0x8000_0000_0000_0001_u64 as i64),
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
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 1, 1, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_w_wrapping_mask_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0xf000_000f),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 4, 7, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_w_repeated_byte_mask_imm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x00ff_00ff),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 0, 39, 0, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_w16_imm_masked_all_ones_as_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
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
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_w8_zero_imm_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0),
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
    fn lowers_xor_w8_zero_imm_as_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0),
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
    fn lowers_and_x_zero_imm_as_movz_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_x_all_ones_imm_as_mov_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(-1),
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
    fn lowers_and_all_ones_left_imm_reg_as_mov_or_ands() {
        let cases = [
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                enc_mov_reg(1, 0, 2),
            ),
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                enc_logical_reg_n(0, 0b11, 0, 0, 2, 2),
            ),
        ];

        for (op, expected_insn) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&expected_insn.to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_and_all_ones_left_imm_extended_as_add_zero_base() {
        let cases = [
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtw,
                        shift: 0,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                // all_ones AND uxtw(x2) == uxtw(x2): UBFIZ x0, x2, #0, #32.
                vec![enc_bitfield_regs(1, 0b10, 0, 31, 2, 0)],
            ),
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtw,
                        shift: 2,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                // all_ones ANDS sxtw(x2)<<2: SBFIZ x0, x2, #2, #32 then
                // ANDS x0, x0, x0 (flags from result, not from SP).
                vec![
                    enc_bitfield_regs(1, 0b00, 62, 31, 2, 0),
                    enc_logical_reg_n(1, 0b11, 0, 0, 0, 0),
                ],
            ),
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                // all_ones ANDS uxtb(x2)<<1 (W32): UBFIZ w0, w2, #1, #8 then
                // ANDS w0, w0, w0.
                vec![
                    enc_bitfield_regs(0, 0b10, 31, 7, 2, 0),
                    enc_logical_reg_n(0, 0b11, 0, 0, 0, 0),
                ],
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
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_andnot_all_ones_left_imm_reg_as_mvn_or_flags() {
        let cases = [
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_logical_reg_n(1, 0b01, 1, 0, 31, 2), 0xd65f_03c0u32],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_logical_reg_n(0, 0b01, 1, 0, 31, 2), 0xd65f_03c0u32],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_logical_reg_n(1, 0b01, 1, 0, 31, 2),
                    enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_logical_reg_n(0, 0b01, 1, 0, 31, 2),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
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
    fn lowers_andnot_all_ones_left_imm_extended_as_add_mvn_or_flags() {
        let cases = [
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtw,
                        shift: 0,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![
                    enc_bitfield_regs(1, 0b10, 0, 31, 2, 0),
                    enc_logical_reg_n(1, 0b01, 1, 0, 31, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![
                    enc_bitfield_regs(0, 0b10, 31, 7, 2, 0),
                    enc_logical_reg_n(0, 0b01, 1, 0, 31, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtw,
                        shift: 2,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_bitfield_regs(1, 0b00, 62, 31, 2, 0),
                    enc_logical_reg_n(1, 0b01, 1, 0, 31, 0),
                    enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_bitfield_regs(0, 0b00, 31, 7, 2, 0),
                    enc_logical_reg_n(0, 0b01, 1, 0, 31, 0),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
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
    fn lowers_orr_all_ones_left_imm_reg_as_movn_or_flags() {
        let cases = [
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(1, 0b00, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(0, 0b00, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_mov_wide(1, 0b00, 0, 0, 0),
                    enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_mov_wide(0, 0b00, 0, 0, 0),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
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
    fn lowers_orr_all_ones_left_imm_extended_as_movn_or_flags() {
        let cases = [
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtw,
                        shift: 0,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(1, 0b00, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(0, 0b00, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtw,
                        shift: 2,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_mov_wide(1, 0b00, 0, 0, 0),
                    enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_mov_wide(0, 0b00, 0, 0, 0),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
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
    fn lowers_xor_all_ones_left_imm_reg_as_eon_or_flags() {
        let cases = [
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_logical_reg_n(1, 0b10, 1, 0, 31, 2), 0xd65f_03c0u32],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_logical_reg_n(0, 0b10, 1, 0, 31, 2), 0xd65f_03c0u32],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_logical_reg_n(1, 0b10, 1, 0, 31, 2),
                    enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_logical_reg_n(0, 0b10, 1, 0, 31, 2),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
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
    fn lowers_xor_all_ones_left_imm_extended_as_add_eon_or_flags() {
        let cases = [
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtw,
                        shift: 0,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![
                    enc_bitfield_regs(1, 0b10, 0, 31, 2, 0),
                    enc_logical_reg_n(1, 0b10, 1, 0, 31, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![
                    enc_bitfield_regs(0, 0b10, 31, 7, 2, 0),
                    enc_logical_reg_n(0, 0b10, 1, 0, 31, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtw,
                        shift: 2,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_bitfield_regs(1, 0b00, 62, 31, 2, 0),
                    enc_logical_reg_n(1, 0b10, 1, 0, 31, 0),
                    enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_bitfield_regs(0, 0b00, 31, 7, 2, 0),
                    enc_logical_reg_n(0, 0b10, 1, 0, 31, 0),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
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
    fn lowers_orr_x_zero_imm_as_mov_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
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
    fn lowers_logical_same_sources_as_mov_or_noop() {
        let cases = [
            (
                OpKind::And {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Reg(x(1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Reg(x(1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
            ),
            (
                OpKind::And {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(x(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(x(0)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
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
    fn lowers_logical_zero_source_reg_as_mov_or_noop() {
        let cases = [
            (
                OpKind::Or {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
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
    fn lowers_logical_zero_base_extended_as_zero_or_add_zero_base() {
        let cases = [
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtw,
                        shift: 0,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(1, 0b10, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_wide(0, 0b10, 0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtw,
                        shift: 2,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![enc_logical_reg_n(1, 0b11, 0, 0, 31, 31), 0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtw,
                        shift: 0,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_bitfield_regs(1, 0b10, 0, 31, 2, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_bitfield_regs(0, 0b10, 31, 7, 2, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::Or {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Sxtw,
                        shift: 2,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_bitfield_regs(1, 0b00, 62, 31, 2, 0),
                    enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
                ],
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Extended {
                        reg: x(2),
                        extend: ExtendOp::Uxtb,
                        shift: 1,
                    },
                    width: OpWidth::W32,
                    flags: FlagUpdate::All,
                },
                vec![
                    enc_bitfield_regs(0, 0b10, 31, 7, 2, 0),
                    enc_logical_reg_n(0, 0b11, 0, 31, 0, 0),
                    0xd65f_03c0u32,
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
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_and_reg_zero_operand_as_movz_zero() {
        let cases = [
            (
                OpKind::And {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Reg(x(1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                1,
            ),
            (
                OpKind::And {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                0,
            ),
        ];

        for (kind, sf) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&enc_mov_wide(sf, 0b10, 0, 0, 0).to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_andnot_zero_base_reg_as_movz_zero() {
        let cases = [
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Reg(x(1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                1,
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Reg(x(1)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                0,
            ),
        ];

        for (kind, sf) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&enc_mov_wide(sf, 0b10, 0, 0, 0).to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_andnot_zero_source_reg_as_mov_or_noop() {
        let cases = [
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: x(1),
                    src2: SrcOperand::Reg(VReg::Imm(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
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
    fn lowers_xor_reg_same_srcs_as_movz_zero() {
        let cases = [
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(x(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                1,
            ),
            (
                OpKind::Xor {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(x(0)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                0,
            ),
        ];

        for (kind, sf) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&enc_mov_wide(sf, 0b10, 0, 0, 0).to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_andnot_reg_same_srcs_as_movz_zero() {
        let cases = [
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(x(0)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                1,
            ),
            (
                OpKind::AndNot {
                    dst: x(0),
                    src1: x(0),
                    src2: SrcOperand::Reg(x(0)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
                0,
            ),
        ];

        for (kind, sf) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();

            let mut expected = Vec::new();
            expected.extend_from_slice(&enc_mov_wide(sf, 0b10, 0, 0, 0).to_le_bytes());
            expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
            assert_eq!(code, expected);
        }
    }
    #[test]
    fn lowers_xor_x_zero_base_reg_as_mov_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
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
    fn lowers_orr_w_zero_same_reg_as_self_mov_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
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
    fn lowers_orr_x_all_ones_imm_as_movn_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(-1),
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
    fn lowers_orr_w_all_ones_imm_as_movn_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(-1),
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
    fn lowers_eor_x_all_ones_imm_as_mvn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(-1),
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
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b01, 1, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_w8_reg_as_and_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
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
        expected.extend_from_slice(&enc_logical_reg(0, 0b00, 0, 1, 2).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_xor_w16_imm_as_eor_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x00ff),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 0, 7, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_andnot_w8_imm_as_and_inverse_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0xf0),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 0, 3, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_not_w8_as_mvn_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
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
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b01, 1, 0, 31, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_not_w16_zero_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
                dst: x(0),
                src: VReg::Imm(0),
                width: OpWidth::W16,
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
    fn lowers_not_x_imm_as_movz() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
                dst: x(0),
                src: VReg::Imm(-16),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 15, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_not_w_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
                dst: x(0),
                src: VReg::Imm(0x1234),
                width: OpWidth::W32,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0x1234, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_not_x_imm_as_movn() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
                dst: x(0),
                src: VReg::Imm(0x1234),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0x1234, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ands_x_zero_imm_as_ands_zero_regs() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(1),
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
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 0, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_test_x_zero_imm_as_ands_zero_regs() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Test {
                src1: x(1),
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
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_test_zero_imm_operand_as_ands_zero_regs() {
        let cases = [
            (
                OpKind::Test {
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                },
                enc_logical_reg_n(1, 0b11, 0, 31, 31, 31),
            ),
            (
                OpKind::Test {
                    src1: VReg::Imm(0x1_0000_0000),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                },
                enc_logical_reg_n(0, 0b11, 0, 31, 31, 31),
            ),
            (
                OpKind::Test {
                    src1: x(1),
                    src2: SrcOperand::Imm(0x100),
                    width: OpWidth::W8,
                },
                enc_logical_reg_n(0, 0b11, 0, 31, 31, 31),
            ),
            (
                OpKind::Test {
                    src1: VReg::Imm(0),
                    src2: SrcOperand::Shifted {
                        reg: x(2),
                        shift: ShiftOp::Ror,
                        amount: 4,
                    },
                    width: OpWidth::W16,
                },
                enc_logical_reg_n(0, 0b11, 0, 31, 31, 31),
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
    fn lowers_test_all_ones_left_imm_reg_as_self_ands() {
        let cases = [
            (
                OpKind::Test {
                    src1: VReg::Imm(-1),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W64,
                },
                enc_logical_reg_n(1, 0b11, 0, 31, 2, 2),
            ),
            (
                OpKind::Test {
                    src1: VReg::Imm(0x1_ffff_ffff),
                    src2: SrcOperand::Reg(x(2)),
                    width: OpWidth::W32,
                },
                enc_logical_reg_n(0, 0b11, 0, 31, 2, 2),
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
    fn lowers_andnot_x_zero_imm_as_mov_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
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
    fn lowers_andnot_x_all_ones_imm_as_movz_zero() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(-1),
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
    fn lowers_bics_w8_imm_with_flags_as_and_uxtb_ands_when_sign_clear() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0xf0),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 0, 3, 0, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_orrs_x_reg_with_flags_as_orr_ands() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b01, 0, 0, 1, 2).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_eors_w_reg_with_flags_as_eor_ands() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
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
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b10, 0, 0, 1, 2).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ands_w_low_mask_imm_to_zero_reg_for_virtual_dst() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: VReg::virt(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x1f),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b11, 0, 0, 4, 31, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_or_x_non_contiguous_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x55),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x55, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(1, 0b01, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_andnot_x_unencodable_inverse_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(!0x55_i64),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x55, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(1, 0b00, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ands_x_non_contiguous_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x55),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x55, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(1, 0b11, 0, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_or_w8_non_contiguous_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x5a),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x5a, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b01, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_andnot_w16_unencodable_inverse_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(!0x5a_i64),
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
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x5a, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b00, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ands_w8_non_contiguous_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x5a),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x5a, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg(0, 0b00, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_orr_x_sparse_imm_in_place_as_logical_imms() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(0x5),
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
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 0, 0, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 62, 0, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_xors_w_sparse_imm_in_place_as_logical_imms_and_tst() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm(0x21),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 0, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 27, 0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_x_sparse_clear_imm_in_place_as_logical_imms() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm64(!0x5),
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
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 63, 62, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(1, 0b00, 1, 61, 62, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_ands_w_sparse_clear_imm_in_place_as_logical_imms() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm(!0x21),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b11, 0, 31, 30, 0, 0).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b11, 0, 26, 30, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_orr_w8_sparse_imm_in_place_as_logical_imms_and_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(0x5),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 0, 0, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 30, 0, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_xor_w16_sparse_imm_in_place_as_logical_imms_and_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Xor {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(0x21),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 0, 0, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b10, 0, 27, 0, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_and_w16_sparse_clear_imm_in_place_as_logical_imms_and_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::And {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(!0x5),
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
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 31, 30, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 29, 30, 1, 1).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_same_width_extend_as_mov_or_noop() {
        let extend_cases = [
            (
                OpKind::ZeroExtend {
                    dst: x(0),
                    src: x(0),
                    from_width: OpWidth::W64,
                    to_width: OpWidth::W64,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::SignExtend {
                    dst: x(0),
                    src: x(0),
                    from_width: OpWidth::W64,
                    to_width: OpWidth::W64,
                },
                vec![0xd65f_03c0u32],
            ),
            (
                OpKind::ZeroExtend {
                    dst: x(0),
                    src: x(0),
                    from_width: OpWidth::W32,
                    to_width: OpWidth::W32,
                },
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                OpKind::SignExtend {
                    dst: x(0),
                    src: x(1),
                    from_width: OpWidth::W64,
                    to_width: OpWidth::W64,
                },
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
            ),
        ];

        for (op, expected_words) in extend_cases {
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
    fn lowers_masked_carry_xor_as_cfinv() {
        let code = lower_ops_with_flagm_features(vec![masked_carry_xor_op()], true, true);

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_flagm(0b000).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_masked_carry_xor_without_flagm_via_sysreg_fallback() {
        let code = lower_ops_with_flagm_features(vec![masked_carry_xor_op()], false, false);

        assert!(!code_has_flagm(&code, 0b000));
        for nzcv in 0_u8..16 {
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(16, 0x1616_1616_1616_1616)], nzcv);
            assert_eq!(out_nzcv, nzcv ^ 0b0010, "NZCV {nzcv:#06b}");
            assert_eq!(out[16], 0x1616_1616_1616_1616, "x16 preserved");
            assert_eq!(sp, 0x8000, "stack restored");
        }
    }
    #[test]
    fn guest_call_exit_mode_rejects_nonempty_arguments_and_non_guest_targets() {
        for (target, args) in [
            (CallTarget::GuestAddr(0x2000), vec![x(0)]),
            (CallTarget::Direct(FunctionId(9)), Vec::new()),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let continuation = builder.create_block(0x1004);
            builder.set_terminator(Terminator::Call {
                target,
                args,
                continuation,
            });
            builder.switch_to_block(continuation);
            builder.set_terminator(Terminator::Return { values: Vec::new() });
            let mut lowerer = Aarch64Lowerer::new();
            lowerer.set_guest_call_exits(true);
            assert!(matches!(
                lowerer.lower_function(&builder.finish()),
                Err(LowerError::UnsupportedOp { .. })
            ));
        }
    }
    #[test]
    fn configured_direct_interworking_calls_record_pc_t_and_preserve_native_state() {
        for (thumb, target, link, expected_flags) in [
            (
                true,
                0x2345_6782_u64,
                0x1004_i64,
                A64_EXIT_VALID | A64_EXIT_AARCH32_T_VALID | A64_EXIT_AARCH32_T,
            ),
            (
                false,
                0x2345_6780,
                0x1005,
                A64_EXIT_VALID | A64_EXIT_AARCH32_T_VALID,
            ),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let continuation = builder.create_block(0x1004);
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: x(14),
                    src: SrcOperand::Imm(link),
                    width: OpWidth::W32,
                },
            );
            builder.set_terminator(Terminator::Call {
                target: CallTarget::GuestAddrInterworking {
                    addr: target,
                    thumb,
                },
                args: Vec::new(),
                continuation,
            });
            builder.switch_to_block(continuation);
            builder.set_terminator(Terminator::Return { values: Vec::new() });
            let function = builder.finish();

            let mut disabled = Aarch64Lowerer::new();
            assert!(matches!(
                disabled.lower_function(&function),
                Err(LowerError::UnsupportedOp { .. })
            ));

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.set_guest_interworking_call_exits(true);
            lowerer.lower_function(&function).unwrap();
            let code = lowerer.finalize().unwrap();
            let state_base = 0x6000;
            let scratch = 0x9999_aaaa_bbbb_cccc;
            let old_nzcv = 0b1101;
            let (out, out_nzcv, sp, pc) = run_aarch64_code_with_memory(
                &code,
                &[(9, scratch), (14, 0xeeee_eeee), (28, state_base)],
                old_nzcv,
                state_base + u64::from(A64_GUEST_PC_OFFSET),
                u64::MAX,
                MemWidth::B8,
            );
            assert_eq!(pc, target);
            assert_eq!(out[9], scratch);
            assert_eq!(out[14], link as u64);
            assert_eq!(out[28], state_base);
            assert_eq!(out_nzcv, old_nzcv);
            assert_eq!(sp, 0x8000);
            let (_, _, _, flags) = run_aarch64_code_with_memory(
                &code,
                &[(9, scratch), (14, 0xeeee_eeee), (28, state_base)],
                old_nzcv,
                state_base + u64::from(A64_GUEST_EXIT_FLAGS_OFFSET),
                0,
                MemWidth::B8,
            );
            assert_eq!(flags, expected_flags as u64);
        }
    }
    #[test]
    fn configured_direct_interworking_calls_reject_invalid_pc_or_arguments() {
        for (target, thumb, args) in [
            (0x2001, true, Vec::new()),
            (0x2002, false, Vec::new()),
            (u64::from(u32::MAX) + 1, true, Vec::new()),
            (0x2000, true, vec![x(0)]),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let continuation = builder.create_block(0x1004);
            builder.set_terminator(Terminator::Call {
                target: CallTarget::GuestAddrInterworking {
                    addr: target,
                    thumb,
                },
                args,
                continuation,
            });
            builder.switch_to_block(continuation);
            builder.set_terminator(Terminator::Return { values: Vec::new() });
            let mut lowerer = Aarch64Lowerer::new();
            lowerer.set_guest_interworking_call_exits(true);
            assert!(matches!(
                lowerer.lower_function(&builder.finish()),
                Err(LowerError::InvalidOperand { .. }) | Err(LowerError::UnsupportedOp { .. })
            ));
        }
    }
    #[test]
    fn native_exit_edge_unconditional_records_pc_and_preserves_state() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let source = builder.current_block();
        let target = builder.create_block(0x1004);
        builder.set_terminator(Terminator::Branch { target });
        builder.switch_to_block(target);
        builder.push_op(
            0x1004,
            OpKind::Mov {
                dst: x(0),
                src: SrcOperand::Imm(0x55),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let resume_pc = 0x1234_5678_9abc_def0;
        let mut lowerer = Aarch64Lowerer::new();
        lowerer.set_native_exit_edges(HashMap::from([((source, target), resume_pc)]));
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let state_base = 0x6000;
        let state_pc = state_base + u64::from(A64_GUEST_PC_OFFSET);
        let prior_pc = 0x0fed_cba9_8765_4321;
        let x9 = 0x0909_0909_0909_0909;
        let x0 = 0x1111_2222_3333_4444;
        let old_nzcv = 0b1011;
        let (out, out_nzcv, sp, recorded_pc) = run_aarch64_code_with_memory(
            &code,
            &[(0, x0), (9, x9), (28, state_base)],
            old_nzcv,
            state_pc,
            prior_pc,
            MemWidth::B8,
        );

        assert_eq!(recorded_pc, resume_pc);
        assert_eq!(out[0], x0, "frontier target body must not execute");
        assert_eq!(out[9], x9, "native-exit scratch must be restored");
        assert_eq!(out[28], state_base, "guest-state pointer must be preserved");
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000, "native-exit stack spill must balance");
    }
    #[test]
    // Regression for issue #20: a register Switch must select the correct edge AND
    // preserve guest NZCV (later blocks read native flags as architectural state).
    // The old lowering compared each case with a flag-setting CMP (SUBS), corrupting
    // live flags. Each case block writes a distinct value so we can confirm the edge
    // taken; the seeded NZCV must be unchanged on every path.
    #[test]
    fn issue_20_register_switch_preserves_nzcv_and_branches() {
        fn run_switch(index: u64, nzcv_in: u8) -> (u64, u8) {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            let case0 = builder.create_block(4);
            let case1 = builder.create_block(8);
            let default = builder.create_block(12);
            builder.set_terminator(Terminator::Switch {
                index: x(1),
                targets: vec![case0, case1],
                default,
            });
            for (block, marker) in [(case0, 0xAAu64), (case1, 0xBB), (default, 0xCC)] {
                builder.switch_to_block(block);
                builder.push_op(
                    0,
                    OpKind::Mov {
                        dst: x(0),
                        src: SrcOperand::Imm(marker as i64),
                        width: OpWidth::W64,
                    },
                );
                builder.set_terminator(Terminator::Return { values: vec![] });
            }
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.lower_function(&func).unwrap();
            let code = lowerer.finalize().unwrap();
            let (out, out_nzcv, _) = run_aarch64_code(&code, &[(1, index)], nzcv_in);
            (out[0], out_nzcv)
        }

        let sentinel = 0b1011u8;
        assert_eq!(
            run_switch(0, sentinel),
            (0xAA, sentinel),
            "index 0 -> case 0"
        );
        assert_eq!(
            run_switch(1, sentinel),
            (0xBB, sentinel),
            "index 1 -> case 1"
        );
        assert_eq!(
            run_switch(5, sentinel),
            (0xCC, sentinel),
            "out-of-range -> default"
        );
    }
    #[test]
    fn configured_aarch32_indirect_exit_records_w32_pc_t_state_and_preserves_host_state() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.set_terminator(Terminator::IndirectBranch {
            target: x(3),
            possible_targets: Vec::new(),
        });
        let mut lowerer = Aarch64Lowerer::new();
        lowerer.set_guest_indirect_exits(true);
        lowerer.lower_function(&builder.finish()).unwrap();
        let code = lowerer.finalize().unwrap();

        assert!(
            code.chunks_exact(4).all(|bytes| {
                let word = u32::from_le_bytes(bytes.try_into().unwrap());
                word & 0xffff_fc1f != 0xd61f_0000
            }),
            "AArch32 dispatcher exit must not contain BR Xn"
        );

        let state_base = 0x6000;
        let target = 0xdead_beef_1234_5679;
        let scratch_sentinel = 0x1616_1616_1616_1616;
        let old_nzcv = 0b1011;
        let (out, out_nzcv, sp, pc) = run_aarch64_code_with_memory(
            &code,
            &[(3, target), (16, scratch_sentinel), (28, state_base)],
            old_nzcv,
            state_base + u64::from(A64_GUEST_PC_OFFSET),
            u64::MAX,
            MemWidth::B8,
        );
        assert_eq!(pc, 0x1234_5678, "PC must use zero-extended W32 target & !1");
        assert_eq!(out[3], target);
        assert_eq!(out[16], scratch_sentinel);
        assert_eq!(out[28], state_base);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);

        let (_, _, _, flags) = run_aarch64_code_with_memory(
            &code,
            &[(3, target), (16, scratch_sentinel), (28, state_base)],
            old_nzcv,
            state_base + u64::from(A64_GUEST_EXIT_FLAGS_OFFSET),
            0,
            MemWidth::B8,
        );
        assert_eq!(
            flags,
            (A64_EXIT_VALID | A64_EXIT_AARCH32_T_VALID | A64_EXIT_AARCH32_T) as u64
        );
    }
    #[test]
    fn configured_register_blx_and_blx_lr_record_old_target_after_link_write() {
        for (target_reg, snapshot) in [(3_u8, None), (14, Some(VReg::virt(77)))] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let continuation = builder.create_block(0x1004);
            let target = if let Some(snapshot) = snapshot {
                builder.push_op(
                    0x1000,
                    OpKind::Mov {
                        dst: snapshot,
                        src: SrcOperand::Reg(x(14)),
                        width: OpWidth::W32,
                    },
                );
                snapshot
            } else {
                x(target_reg)
            };
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: x(14),
                    src: SrcOperand::Imm(0x1004),
                    width: OpWidth::W32,
                },
            );
            builder.set_terminator(Terminator::Call {
                target: CallTarget::IndirectInterworking(target),
                args: Vec::new(),
                continuation,
            });
            builder.switch_to_block(continuation);
            builder.set_terminator(Terminator::Return { values: Vec::new() });

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.set_guest_interworking_call_exits(true);
            lowerer.lower_function(&builder.finish()).unwrap();
            let code = lowerer.finalize().unwrap();
            assert!(code.chunks_exact(4).all(|bytes| {
                let word = u32::from_le_bytes(bytes.try_into().unwrap());
                let masked = word & 0xffff_fc1f;
                masked != 0xd61f_0000 && masked != 0xd63f_0000
            }));

            let state_base = 0x6000;
            let guest_target = 0xdead_beef_1234_5679;
            let scratch_sentinel = 0x1616_1616_1616_1616;
            let work_sentinel = 0x1717_1717_1717_1717;
            let initial = if target_reg == 14 {
                vec![
                    (14, guest_target),
                    (16, scratch_sentinel),
                    (17, work_sentinel),
                    (28, state_base),
                ]
            } else {
                vec![(3, guest_target), (14, 0xeeee_eeee), (28, state_base)]
            };
            let (out, out_nzcv, sp, pc) = run_aarch64_code_with_memory(
                &code,
                &initial,
                0b1010,
                state_base + u64::from(A64_GUEST_PC_OFFSET),
                u64::MAX,
                MemWidth::B8,
            );
            assert_eq!(pc, 0x1234_5678);
            assert_eq!(out[14], 0x1004);
            if target_reg == 14 {
                assert_eq!(out[16], scratch_sentinel);
                assert_eq!(out[17], work_sentinel);
            } else {
                assert_eq!(out[3], guest_target);
            }
            assert_eq!(out_nzcv, 0b1010);
            assert_eq!(sp, 0x8000);
            let (_, _, _, flags) = run_aarch64_code_with_memory(
                &code,
                &initial,
                0b1010,
                state_base + u64::from(A64_GUEST_EXIT_FLAGS_OFFSET),
                0,
                MemWidth::B8,
            );
            assert_eq!(
                flags,
                (A64_EXIT_VALID | A64_EXIT_AARCH32_T_VALID | A64_EXIT_AARCH32_T) as u64
            );
        }
    }
    #[test]
    fn configured_register_blx_rejects_unsnapshotted_lr_and_malformed_virtual_target() {
        for (target, snapshot_op) in [
            (x(14), None),
            (x(15), None),
            (
                VReg::virt(77),
                Some(OpKind::Mov {
                    dst: VReg::virt(77),
                    src: SrcOperand::Reg(x(13)),
                    width: OpWidth::W32,
                }),
            ),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let continuation = builder.create_block(0x1004);
            if let Some(op) = snapshot_op {
                builder.push_op(0x1000, op);
            }
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: x(14),
                    src: SrcOperand::Imm(0x1004),
                    width: OpWidth::W32,
                },
            );
            builder.set_terminator(Terminator::Call {
                target: CallTarget::IndirectInterworking(target),
                args: Vec::new(),
                continuation,
            });
            builder.switch_to_block(continuation);
            builder.set_terminator(Terminator::Return { values: Vec::new() });
            let mut lowerer = Aarch64Lowerer::new();
            lowerer.set_guest_interworking_call_exits(true);
            assert!(matches!(
                lowerer.lower_function(&builder.finish()),
                Err(LowerError::UnsupportedOp { .. }) | Err(LowerError::InvalidRegister(_))
            ));
        }
    }
    #[test]
    fn fuses_lifted_bfxil_imm_source_as_and_orr() {
        let extracted = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfx {
                dst: extracted,
                src: VReg::Imm(0x3c0),
                lsb: 4,
                width_bits: 8,
                sign_extend: false,
                op_width: OpWidth::W64,
            },
        );
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(1),
                src: extracted,
                lsb: 0,
                width_bits: 8,
                op_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let clear_mask = (!0xff_u64) & OpWidth::W64.mask();
        let (clear_n, clear_immr, clear_imms) =
            Aarch64Lowerer::logical_bitmask_imm(clear_mask as i64, OpWidth::W64).unwrap();
        let (insert_n, insert_immr, insert_imms) =
            Aarch64Lowerer::logical_bitmask_imm(0x3c, OpWidth::W64).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(
            &enc_logical_imm(1, 0b00, clear_n, clear_immr, clear_imms, 0, 1).to_le_bytes(),
        );
        expected.extend_from_slice(
            &enc_logical_imm(1, 0b01, insert_n, insert_immr, insert_imms, 0, 0).to_le_bytes(),
        );
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_full_width_bfxil_as_mov_or_noop() {
        let bfxil_cases = [
            (x(0), x(1), x(0), 64, OpWidth::W64, vec![0xd65f_03c0u32]),
            (
                x(0),
                x(1),
                x(0),
                32,
                OpWidth::W32,
                vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0u32],
            ),
            (
                x(0),
                x(0),
                x(1),
                64,
                OpWidth::W64,
                vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0u32],
            ),
        ];

        for (dst, dst_in, src, width_bits, op_width, expected_words) in bfxil_cases {
            let extracted = VReg::virt(0);
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(
                0,
                OpKind::Bfx {
                    dst: extracted,
                    src,
                    lsb: 0,
                    width_bits,
                    sign_extend: false,
                    op_width,
                },
            );
            builder.push_op(
                0,
                OpKind::Bfi {
                    dst,
                    dst_in,
                    src: extracted,
                    lsb: 0,
                    width_bits,
                    op_width,
                },
            );
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
    fn lowers_rcl_rcr_register_counts_and_preserves_scratch_state() {
        assert_rotate_carry_lowering(
            "rcr_x_reg16",
            OpKind::Rcr {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W64,
                flags: rotate_flags(),
            },
            0x1234_5678_9abc_def0,
            16,
            0b0100,
            OpWidth::W64,
            rotate_flags(),
            true,
            0,
            Some(2),
        );

        assert_rotate_carry_lowering(
            "rcl_w16_reg18_mod_period_and_count_aliases_dst",
            OpKind::Rcl {
                dst: x(2),
                src: x(1),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W16,
                flags: rotate_flags(),
            },
            0x8001,
            18,
            0b1110,
            OpWidth::W16,
            rotate_flags(),
            false,
            2,
            Some(2),
        );

        assert_rotate_carry_lowering(
            "rcr_w8_reg18_zero_effect_restores_flags",
            OpKind::Rcr {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W8,
                flags: rotate_flags(),
            },
            0x5a,
            18,
            0b1011,
            OpWidth::W8,
            rotate_flags(),
            true,
            0,
            Some(2),
        );
    }
    #[test]
    fn lowers_rcl_flags_none_as_value_only_and_restores_nzcv() {
        assert_rotate_carry_lowering(
            "rcl_w32_flags_none",
            OpKind::Rcl {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            0x8000_0000,
            1,
            0b0011,
            OpWidth::W32,
            FlagUpdate::None,
            false,
            0,
            None,
        );
    }
    #[test]
    fn lowers_crc32c_exact_encodings_widths_aliases_and_feature_gate() {
        let rax = x86(X86Reg::Rax);
        let rcx = x86(X86Reg::Rcx);
        let rdx = x86(X86Reg::Rdx);
        let rbx = x86(X86Reg::Rbx);
        for (width, dst, crc, data, expected) in [
            (OpWidth::W8, rax, rcx, rdx, 0x1ac2_5020),
            (OpWidth::W16, rbx, rdx, rcx, 0x1ac1_5443),
            (OpWidth::W32, rax, rax, rbx, 0x1ac3_5800),
            (OpWidth::W64, rcx, rdx, rcx, 0x9ac1_5c41),
        ] {
            let code = try_lower_ops_with_crc_feature(
                vec![OpKind::Crc32C {
                    dst,
                    crc,
                    data,
                    data_width: width,
                }],
                true,
            )
            .unwrap();
            assert_eq!(code_words(&code)[0], expected, "CRC32C {width:?}");
        }

        let op = OpKind::Crc32C {
            dst: rax,
            crc: rax,
            data: rbx,
            data_width: OpWidth::W64,
        };
        assert!(try_lower_ops_with_crc_feature(vec![op.clone()], false).is_err());
        assert!(
            try_lower_ops_with_crc_feature(
                vec![OpKind::Crc32C {
                    dst: rax,
                    crc: rax,
                    data: rbx,
                    data_width: OpWidth::W128,
                }],
                true,
            )
            .is_err()
        );
    }
    #[test]
    fn lowers_crc32c_runtime_known_answers_zero_extension_and_flags() {
        let reference = |mut crc: u32, data: u64, width: OpWidth| {
            for byte in 0..(width.bits() / 8) {
                crc ^= ((data >> (byte * 8)) & 0xff) as u32;
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0x82f6_3b78 & 0_u32.wrapping_sub(crc & 1));
                }
            }
            u64::from(crc)
        };
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        let cases = [
            (OpWidth::W8, u64::MAX, 0x31, 0x6f0a_661c),
            (OpWidth::W16, 0x1234_5678, 0xabcd, 0xaae3_2043),
            (OpWidth::W32, 0x89ab_cdef, 0x0123_4567, 0x796a_b9a9),
            (
                OpWidth::W64,
                0xffff_ffff_dead_beef,
                0x0123_4567_89ab_cdef,
                0x3ab0_1437,
            ),
        ];
        for (width, crc, data, expected) in cases {
            let code = try_lower_ops_with_crc_feature(
                vec![OpKind::Crc32C {
                    dst: rax,
                    crc: rax,
                    data: rbx,
                    data_width: width,
                }],
                true,
            )
            .unwrap();
            let old_nzcv = 0b1011;
            let (out, out_nzcv, sp) = run_aarch64_code(
                &code,
                &[
                    (0, crc),
                    (3, data),
                    (16, 0x1616_1616_1616_1616),
                    (17, 0x1717_1717_1717_1717),
                ],
                old_nzcv,
            );
            assert_eq!(out[0], expected, "CRC32C {width:?} result");
            assert_eq!(expected, reference(crc as u32, data, width));
            assert_eq!(out[3], data, "CRC32C {width:?} data source");
            assert_eq!(out[16], 0x1616_1616_1616_1616, "scratch x16");
            assert_eq!(out[17], 0x1717_1717_1717_1717, "scratch x17");
            assert_eq!(out_nzcv, old_nzcv, "CRC32C {width:?} NZCV");
            assert_eq!(sp, 0x8000, "CRC32C {width:?} stack");
        }

        for width in [OpWidth::W8, OpWidth::W64] {
            let value = 0xa5a5_5a5a_dead_beef;
            let code = try_lower_ops_with_crc_feature(
                vec![OpKind::Crc32C {
                    dst: rax,
                    crc: rax,
                    data: rax,
                    data_width: width,
                }],
                true,
            )
            .unwrap();
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(0, value)], 0b0110);
            assert_eq!(out[0], reference(value as u32, value, width));
            assert_eq!(out_nzcv, 0b0110, "aliased CRC32C {width:?} NZCV");
            assert_eq!(sp, 0x8000, "aliased CRC32C {width:?} stack");
        }
    }
