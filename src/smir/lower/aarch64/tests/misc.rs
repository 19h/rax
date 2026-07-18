//! tests::misc tests

use super::*;
use crate::smir::lower::aarch64::*;

    // Regression for issue #12: the BFXIL fusion (Bfx + Bfi{lsb:0}) must not fire
    // when the Bfx destination is an architectural register.
    #[test]
    fn issue_12_bfxil_fusion_preserves_arch_bfx_write() {
        let code = lower_ops(vec![
            OpKind::Bfx {
                dst: x(2),
                src: x(1),
                lsb: 4,
                width_bits: 8,
                sign_extend: false,
                op_width: OpWidth::W64,
            },
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(0),
                src: x(2),
                lsb: 0,
                width_bits: 8,
                op_width: OpWidth::W64,
            },
        ]);
        let (regs, _, _) = run_aarch64_code(&code, &[(1, 0xAB0), (2, 0xDEAD), (0, 0xFF00)], 0);
        assert_eq!(
            regs[2], 0xAB,
            "Bfx must write x2 (BFXIL fusion must not drop it)"
        );
        assert_eq!(regs[0], 0xFFAB, "final Bfi result: low byte replaced");
    }
    // Regression for issue #8: the CLS fusion (Sar->Xor->Clz->Sub) must not collapse
    // to `cls x0, x1` when its three intermediates are architectural registers — the
    // guest-visible asr/eor/clz writes must survive.
    #[test]
    fn issue_8_cls_fusion_preserves_arch_intermediate_writes() {
        let code = lower_ops(vec![
            OpKind::Sar {
                dst: x(2),
                src: x(1),
                amount: SrcOperand::Imm(63),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Xor {
                dst: x(3),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Clz {
                dst: x(4),
                src: x(3),
                width: OpWidth::W64,
            },
            OpKind::Sub {
                dst: x(0),
                src1: x(4),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ]);
        let (regs, _, _) = run_aarch64_code(
            &code,
            &[(1, 0xFF), (2, 0xDEAD), (3, 0xDEAD), (4, 0xDEAD)],
            0,
        );
        assert_eq!(regs[2], 0, "asr result (sign_mask) must be written");
        assert_eq!(regs[3], 0xFF, "eor result (normalized) must be written");
        assert_eq!(regs[4], 56, "clz result (leading) must be written");
        assert_eq!(regs[0], 55, "final cls result");
    }
    // Regression for the native ARM facet of issue #61: the same mixed-register
    // helper path must reserve host X30 even when the operand is ArmReg::X(30).
    #[test]
    fn rejects_arm_x30_identity_mapping_x30_is_link_register() {
        for kind in [
            OpKind::Mov {
                dst: x(30),
                src: SrcOperand::Imm(0x4141_4141),
                width: OpWidth::W64,
            },
            OpKind::Mov {
                dst: x(16),
                src: SrcOperand::Reg(x(30)),
                width: OpWidth::W64,
            },
            OpKind::Cmp {
                src1: x(30),
                src2: SrcOperand::Reg(x(16)),
                width: OpWidth::W64,
            },
        ] {
            let err = try_lower_single_op(kind).unwrap_err();
            assert!(
                matches!(err, LowerError::InvalidRegister(_)),
                "X30 must be rejected (host X30 = LR): {err:?}"
            );
        }

        assert!(
            try_lower_single_op(OpKind::Mov {
                dst: x(29),
                src: SrcOperand::Reg(x(16)),
                width: OpWidth::W64,
            })
            .is_ok(),
            "X29 (host X29, guest-backed) must still lower"
        );
    }
    #[test]
    fn zero_base_extended_lowering_is_sp_independent() {
        // #58: a zero base (src1 == Imm(0), or an all-ones logical identity) was
        // encoded as Rn = 31 in the add/sub extended-register and immediate
        // encodings, where 31 means SP/WSP (not XZR/WZR). The lowered code then
        // computed `SP +/- extend(src)`, leaking the (here, emulated) stack
        // pointer into guest state. The harness runs with SP = 0x8000, so a
        // correct lowering must produce SP-independent results.
        let x10: u64 = 0x1_2345_6789;
        let scratch = 0x1616_1616_1616_1616;
        let uxtw = x10 & 0xFFFF_FFFF; // 0x2345_6789
        let sxtb = ((x10 as u8) as i8 as i64) as u64; // sign-extend low byte

        let ext = |reg, extend, shift| SrcOperand::Extended { reg, extend, shift };
        let code = lower_ops(vec![
            // 036: 0 + (uxtw(x10) << 2)  -> UBFIZ, no SP.
            OpKind::Add {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: ext(x(10), ExtendOp::Uxtw, 2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            // 036: 0 - uxtw(x10)  -> UBFIZ then XZR-based negate.
            OpKind::Sub {
                dst: x(1),
                src1: VReg::Imm(0),
                src2: ext(x(10), ExtendOp::Uxtw, 0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            // 030: 0 | (uxtw(x10) << 1)  -> zero-base logical, value == src.
            OpKind::Or {
                dst: x(2),
                src1: VReg::Imm(0),
                src2: ext(x(10), ExtendOp::Uxtw, 1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            // 030: 0 ^ uxtw(x10).
            OpKind::Xor {
                dst: x(3),
                src1: VReg::Imm(0),
                src2: ext(x(10), ExtendOp::Uxtw, 0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            // 036: 0 + sxtb(x10)  -> SBFIZ, no SP.
            OpKind::Add {
                dst: x(4),
                src1: VReg::Imm(0),
                src2: ext(x(10), ExtendOp::Sxtb, 0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            // 035: subword (W16) 0 + (uxtw(x10) << 1); SP's bit 15 would land in
            // the 16-bit result window if the base were SP.
            OpKind::Add {
                dst: x(5),
                src1: VReg::Imm(0),
                src2: ext(x(10), ExtendOp::Uxtw, 1),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            // Flag-only 0 - uxtw(x10) uses a saved scratch, not SP.
            OpKind::Sub {
                dst: VReg::virt(0),
                src1: VReg::Imm(0),
                src2: ext(x(10), ExtendOp::Uxtw, 0),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::TestCondition {
                dst: x(6),
                cond: Condition::Negative,
            },
            // Flag-only TST -1, sxtb(x10) also uses the scratch path.
            OpKind::Test {
                src1: VReg::Imm(-1),
                src2: ext(x(10), ExtendOp::Sxtb, 0),
                width: OpWidth::W64,
            },
            OpKind::TestCondition {
                dst: x(7),
                cond: Condition::Negative,
            },
        ]);

        let (out, _nzcv, sp) = run_aarch64_code(&code, &[(10, x10), (16, scratch)], 0);
        assert_eq!(out[0], uxtw << 2, "add uxtw<<2");
        assert_eq!(out[1], 0u64.wrapping_sub(uxtw), "sub uxtw (negate)");
        assert_eq!(out[2], uxtw << 1, "or uxtw<<1");
        assert_eq!(out[3], uxtw, "xor uxtw");
        assert_eq!(out[4], sxtb, "add sxtb");
        assert_eq!(out[5], (uxtw << 1) & 0xFFFF, "subword add uxtw<<1 (W16)");
        assert_eq!(out[6], 1, "flag-only sub uxtw is negative");
        assert_eq!(out[7], 1, "flag-only test sxtb is negative");
        assert_eq!(out[16], scratch, "scratch register restored");
        assert_eq!(sp, 0x8000, "stack pointer restored");
    }
    #[test]
    fn lowers_sbb_x_nonzero_imm_with_destination_scratch() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sbb {
                dst: x(0),
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
        expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&enc_addsub_carry_regs(1, 1, 0, 0, 1, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_general_immediate_div_runtime() {
        assert_div_runtime_lowering(
            "divu_x_general_imm_with_remainder",
            false,
            0,
            Some(3),
            1,
            SrcOperand::Imm64(0x12345),
            None,
            0x1234_5678_9abc_def0,
            0x12345,
            OpWidth::W64,
        );
        assert_div_runtime_lowering(
            "divs_w_general_imm_with_remainder",
            true,
            0,
            Some(3),
            1,
            SrcOperand::Imm(7),
            None,
            0xffff_ff85,
            7,
            OpWidth::W32,
        );
    }
    #[test]
    fn lowers_div_remainder_when_outputs_alias_sources() {
        assert_div_w64_lowering(
            "divu_quot_rem_aliases_dividend",
            false,
            1,
            Some(1),
            1,
            SrcOperand::Reg(x(2)),
            Some(2),
            0x1234_5678_9abc_def0,
            0x101,
            FlagUpdate::None,
        );
        assert_div_w64_lowering(
            "divu_outputs_alias_both_sources",
            false,
            1,
            Some(2),
            1,
            SrcOperand::Reg(x(2)),
            Some(2),
            0x1234_5678_9abc_def0,
            0x101,
            FlagUpdate::None,
        );
        assert_div_w64_lowering(
            "divs_outputs_alias_both_sources",
            true,
            2,
            Some(1),
            1,
            SrcOperand::Reg(x(2)),
            Some(2),
            0xffff_ffff_f8a4_32eb,
            0x141,
            FlagUpdate::None,
        );
    }
    #[test]
    fn lowers_vnavg_integer_runtime() {
        fn apply_vnavg(
            a_low: u64,
            a_high: u64,
            b_low: u64,
            b_high: u64,
            elem_bytes: usize,
            lanes: usize,
            signed: bool,
        ) -> (u64, u64) {
            fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
                let mut word = [0u8; 8];
                word[..len].copy_from_slice(&bytes[offset..offset + len]);
                u64::from_le_bytes(word)
            }

            let mut a = [0u8; 16];
            let mut b = [0u8; 16];
            let mut out = [0u8; 16];
            a[..8].copy_from_slice(&a_low.to_le_bytes());
            a[8..].copy_from_slice(&a_high.to_le_bytes());
            b[..8].copy_from_slice(&b_low.to_le_bytes());
            b[8..].copy_from_slice(&b_high.to_le_bytes());

            let elem_bits = elem_bytes * 8;
            let mask = if elem_bits == 64 {
                u64::MAX
            } else {
                (1u64 << elem_bits) - 1
            };
            let ext = |value: u64| -> i128 {
                if signed {
                    if elem_bits == 64 {
                        value as i64 as i128
                    } else {
                        let shift = 64 - elem_bits;
                        ((value << shift) as i64 >> shift) as i128
                    }
                } else {
                    (value & mask) as i128
                }
            };

            for lane in 0..lanes {
                let off = lane * elem_bytes;
                let av = ext(read_lane(&a, off, elem_bytes));
                let bv = ext(read_lane(&b, off, elem_bytes));
                let result = ((av - bv) >> 1) as u64 & mask;
                out[off..off + elem_bytes].copy_from_slice(&result.to_le_bytes()[..elem_bytes]);
            }

            let mut low = [0u8; 8];
            let mut high = [0u8; 8];
            low.copy_from_slice(&out[..8]);
            high.copy_from_slice(&out[8..]);
            (u64::from_le_bytes(low), u64::from_le_bytes(high))
        }

        let a_low = 0x807f_00ff_7f80_ff00;
        let a_high = 0x7fff_8000_ffff_0001;
        let b_low = 0x7f80_ff00_0080_00ff;
        let b_high = 0x8000_7fff_0001_ffff;
        let code = lower_ops(vec![
            OpKind::VNavg {
                dst: v(0),
                src1: v(1),
                src2: v(2),
                elem: VecElementType::I8,
                lanes: 16,
                signed: true,
            },
            OpKind::VNavg {
                dst: v(3),
                src1: v(1),
                src2: v(2),
                elem: VecElementType::I8,
                lanes: 16,
                signed: false,
            },
            OpKind::VNavg {
                dst: v(4),
                src1: v(1),
                src2: v(2),
                elem: VecElementType::I16,
                lanes: 8,
                signed: true,
            },
            OpKind::VNavg {
                dst: v(5),
                src1: v(1),
                src2: v(2),
                elem: VecElementType::I32,
                lanes: 4,
                signed: false,
            },
            OpKind::VNavg {
                dst: v(6),
                src1: v(1),
                src2: v(2),
                elem: VecElementType::I32,
                lanes: 2,
                signed: true,
            },
        ]);

        let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
            &code,
            &[],
            &[(1, a_low, a_high), (2, b_low, b_high)],
        );
        assert_eq!(
            simd[0],
            apply_vnavg(a_low, a_high, b_low, b_high, 1, 16, true)
        );
        assert_eq!(
            simd[3],
            apply_vnavg(a_low, a_high, b_low, b_high, 1, 16, false)
        );
        assert_eq!(
            simd[4],
            apply_vnavg(a_low, a_high, b_low, b_high, 2, 8, true)
        );
        assert_eq!(
            simd[5],
            apply_vnavg(a_low, a_high, b_low, b_high, 4, 4, false)
        );
        assert_eq!(
            simd[6],
            apply_vnavg(a_low, a_high, b_low, b_high, 4, 2, true)
        );
    }
    #[test]
    fn lowers_vpermute_v128_encodings() {
        let words = code_words(&lower_single_op(OpKind::VPermute {
            dst: v(0),
            src1: v(1),
            src2: None,
            indices: v(2),
            elem: VecElementType::I8,
            width: VecWidth::V128,
            overwrite_table: false,
        }));

        let (imm_n, immr, imms) = Aarch64Lowerer::logical_bitmask_imm(0x0f, OpWidth::W32).unwrap();
        let mut expected = vec![enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31)];
        for lane in 0..16 {
            let imm5 = (lane << 1) | 1;
            expected.push(enc_simd_umov(16, 2, imm5, false));
            expected.push(enc_logical_imm(0, 0b00, imm_n, immr, imms, 16, 16));
            expected.push(enc_simd_ins_general(0, 16, imm5));
        }
        expected.push(enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31));
        expected.push(enc_simd_tbl(0, 1, 0, 1, 0, 0));
        expected.push(0xd65f_03c0);

        assert_eq!(words, expected);
    }
    #[test]
    fn lowers_vpermute_v128_runtime() {
        fn pair_bytes(pair: (u64, u64)) -> [u8; 16] {
            let mut bytes = [0u8; 16];
            bytes[..8].copy_from_slice(&pair.0.to_le_bytes());
            bytes[8..].copy_from_slice(&pair.1.to_le_bytes());
            bytes
        }

        fn pair_from_bytes(bytes: [u8; 16]) -> (u64, u64) {
            (
                u64::from_le_bytes(bytes[..8].try_into().unwrap()),
                u64::from_le_bytes(bytes[8..].try_into().unwrap()),
            )
        }

        fn ref_vpermb(table: (u64, u64), indices: (u64, u64)) -> (u64, u64) {
            let table_bytes = pair_bytes(table);
            let index_bytes = pair_bytes(indices);
            let mut out = [0u8; 16];
            for lane in 0..16 {
                out[lane] = table_bytes[(index_bytes[lane] & 0x0f) as usize];
            }
            pair_from_bytes(out)
        }

        fn ref_vpermb2(table1: (u64, u64), table2: (u64, u64), indices: (u64, u64)) -> (u64, u64) {
            let table1_bytes = pair_bytes(table1);
            let table2_bytes = pair_bytes(table2);
            let index_bytes = pair_bytes(indices);
            let mut table = [0u8; 32];
            table[..16].copy_from_slice(&table1_bytes);
            table[16..].copy_from_slice(&table2_bytes);
            let mut out = [0u8; 16];
            for lane in 0..16 {
                out[lane] = table[(index_bytes[lane] & 0x1f) as usize];
            }
            pair_from_bytes(out)
        }

        let table = (0x7060_5040_3020_1000, 0xf0e0_d0c0_b0a0_9080);
        let table2 = (0x1716_1514_1312_1110, 0x1f1e_1d1c_1b1a_1918);
        let alias_table = (0x8765_4321_0fed_cba9, 0x7766_5544_3322_1100);
        let overwrite_table = (0x1020_3040_5060_7080, 0x90a0_b0c0_d0e0_f001);
        let indices = (0x0f10_1102_0304_0506, 0x0718_091a_0b1c_0d0e);
        let alias_indices = (0xfefd_fc03_0211_100f, 0x8e8d_8c8b_8a89_8887);
        let two_table_indices = (0x1f20_0011_1203_1405, 0x1607_1809_1a0b_1c0d);
        let scratch30 = (0x3030_3030_3030_3030, 0x3030_3030_3030_3030);
        let scratch31 = (0x3131_3131_3131_3131, 0x3131_3131_3131_3131);
        let code = lower_ops(vec![
            OpKind::VPermute {
                dst: v(0),
                src1: v(1),
                src2: None,
                indices: v(2),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                overwrite_table: false,
            },
            OpKind::VPermute {
                dst: v(3),
                src1: v(1),
                src2: None,
                indices: v(3),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                overwrite_table: false,
            },
            OpKind::VPermute {
                dst: v(4),
                src1: v(4),
                src2: None,
                indices: v(2),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                overwrite_table: false,
            },
            OpKind::VPermute {
                dst: v(5),
                src1: v(1),
                src2: Some(v(6)),
                indices: v(5),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                overwrite_table: false,
            },
            OpKind::VPermute {
                dst: v(7),
                src1: v(7),
                src2: Some(v(6)),
                indices: v(2),
                elem: VecElementType::I8,
                width: VecWidth::V128,
                overwrite_table: true,
            },
        ]);

        let (regs, simd, sp) = run_aarch64_code_with_regs_and_simd(
            &code,
            &[(16, 0x1616_1616_1616_1616)],
            &[
                (1, table.0, table.1),
                (2, indices.0, indices.1),
                (3, alias_indices.0, alias_indices.1),
                (4, alias_table.0, alias_table.1),
                (5, two_table_indices.0, two_table_indices.1),
                (6, table2.0, table2.1),
                (7, overwrite_table.0, overwrite_table.1),
                (30, scratch30.0, scratch30.1),
                (31, scratch31.0, scratch31.1),
            ],
        );
        assert_eq!(simd[0], ref_vpermb(table, indices));
        assert_eq!(simd[1], table);
        assert_eq!(simd[2], indices);
        assert_eq!(simd[3], ref_vpermb(table, alias_indices));
        assert_eq!(simd[4], ref_vpermb(alias_table, indices));
        assert_eq!(simd[5], ref_vpermb2(table, table2, two_table_indices));
        assert_eq!(simd[6], table2);
        assert_eq!(simd[7], ref_vpermb2(overwrite_table, table2, indices));
        assert_eq!(simd[30], scratch30);
        assert_eq!(simd[31], scratch31);
        assert_eq!(regs[16], 0x1616_1616_1616_1616);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_vmpsadbw_v128_runtime() {
        fn pair_bytes(pair: (u64, u64)) -> [u8; 16] {
            let mut bytes = [0u8; 16];
            bytes[..8].copy_from_slice(&pair.0.to_le_bytes());
            bytes[8..].copy_from_slice(&pair.1.to_le_bytes());
            bytes
        }

        fn pair_from_bytes(bytes: [u8; 16]) -> (u64, u64) {
            (
                u64::from_le_bytes(bytes[..8].try_into().unwrap()),
                u64::from_le_bytes(bytes[8..].try_into().unwrap()),
            )
        }

        fn ref_vmpsadbw(src1: (u64, u64), src2: (u64, u64), imm: u8) -> (u64, u64) {
            let src1 = pair_bytes(src1);
            let src2 = pair_bytes(src2);
            // SRC1 provides the sliding 4-byte window (start = imm[2]*4); SRC2 the
            // FIXED 4-byte block (= imm[1:0]*4). Matches x86 MPSADBW. (#33)
            let src1_window_base = (((imm >> 2) & 0x1) as usize) * 4;
            let src2_block_base = ((imm & 0x3) as usize) * 4;
            let mut out = [0u8; 16];
            for lane in 0..8 {
                let mut sad = 0u16;
                for offset in 0..4 {
                    let lhs = src1[src1_window_base + lane + offset] as i16;
                    let rhs = src2[src2_block_base + offset] as i16;
                    sad = sad.wrapping_add((lhs - rhs).unsigned_abs());
                }
                out[lane * 2..lane * 2 + 2].copy_from_slice(&sad.to_le_bytes());
            }
            pair_from_bytes(out)
        }

        let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));
        let initial_nzcv = NZCV_N | NZCV_C;
        let src1 = (0x00ff_1020_3040_5060, 0x7f80_90a0_b0c0_d0e0);
        let src2 = (0xf0e0_d0c0_b0a0_9080, 0x0011_2233_4455_6677);
        let alias_src1 = (0x0102_0304_0506_0708, 0xf1e2_d3c4_b5a6_9788);
        let alias_src2 = (0x8877_6655_4433_2211, 0x1020_3040_5060_7080);
        let scratch30 = (0x3030_3030_3030_3030, 0x3030_3030_3030_3030);
        let scratch31 = (0x3131_3131_3131_3131, 0x3131_3131_3131_3131);
        let code = lower_ops(vec![
            OpKind::Mov {
                dst: nzcv,
                src: SrcOperand::Imm(initial_nzcv),
                width: OpWidth::W32,
            },
            OpKind::VMpsadbw {
                dst: v(0),
                src1: v(1),
                src2: v(2),
                mask: None,
                width: VecWidth::V128,
                imm: 0b00_10,
                zeroing: false,
            },
            OpKind::VMpsadbw {
                dst: v(3),
                src1: v(3),
                src2: v(2),
                mask: None,
                width: VecWidth::V128,
                imm: 0b11_01,
                zeroing: false,
            },
            OpKind::VMpsadbw {
                dst: v(4),
                src1: v(1),
                src2: v(4),
                mask: None,
                width: VecWidth::V128,
                imm: 0b11_10,
                zeroing: false,
            },
            OpKind::Mov {
                dst: x(0),
                src: SrcOperand::Reg(nzcv),
                width: OpWidth::W32,
            },
        ]);

        let (regs, simd, sp) = run_aarch64_code_with_regs_and_simd(
            &code,
            &[(12, 0x1212_1212_1212_1212), (16, 0x1616_1616_1616_1616)],
            &[
                (1, src1.0, src1.1),
                (2, src2.0, src2.1),
                (3, alias_src1.0, alias_src1.1),
                (4, alias_src2.0, alias_src2.1),
                (30, scratch30.0, scratch30.1),
                (31, scratch31.0, scratch31.1),
            ],
        );

        assert_eq!(simd[0], ref_vmpsadbw(src1, src2, 0b00_10));
        assert_eq!(simd[1], src1);
        assert_eq!(simd[2], src2);
        assert_eq!(simd[3], ref_vmpsadbw(alias_src1, src2, 0b11_01));
        assert_eq!(simd[4], ref_vmpsadbw(src1, alias_src2, 0b11_10));
        assert_eq!(simd[30], scratch30);
        assert_eq!(simd[31], scratch31);
        assert_eq!(regs[0] & NZCV_MASK as u64, initial_nzcv as u64);
        assert_eq!(regs[12], 0x1212_1212_1212_1212);
        assert_eq!(regs[16], 0x1616_1616_1616_1616);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn lowers_cwd_x_as_asr63() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cwd {
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
        expected.extend_from_slice(&enc_bitfield(1, 0b00, 63, 63).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cwd_w_as_asr31_zero_ext() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cwd {
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
        expected.extend_from_slice(&enc_bitfield(0, 0b00, 31, 31).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cwd_w8_as_sbfm_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cwd {
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
        expected.extend_from_slice(&enc_bitfield(0, 0b00, 7, 7).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cwd_w16_as_sbfm_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cwd {
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
        expected.extend_from_slice(&enc_bitfield(0, 0b00, 15, 15).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    // Regression for issue #32: FP16 Advanced SIMD arithmetic requires FEAT_FP16.
    // On a host without it the lowering must bail to the interpreter rather than
    // emit an UNDEFINED host FADD/FSUB/.4h that would SIGILL. When the host has
    // FEAT_FP16 it lowers normally.
    #[test]
    fn issue_32_fp16_arith_gated_on_host_feat_fp16() {
        fn lower_with_fp16(available: bool) -> Result<Vec<u8>, LowerError> {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(
                0,
                OpKind::VFP16Arith {
                    dst: v(0),
                    src1: v(1),
                    src2: v(2),
                    mask: None,
                    op: Avx10FP16Op::Add,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V128,
                    zeroing: false,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            lowerer.set_fp16_available_for_test(available);
            lowerer.lower_function(&func)?;
            lowerer.finalize()
        }

        assert!(
            matches!(
                lower_with_fp16(false),
                Err(LowerError::UnsupportedOp { .. })
            ),
            "FP16 arithmetic must bail to the interpreter when the host lacks FEAT_FP16"
        );
        assert!(
            lower_with_fp16(true).is_ok(),
            "FP16 arithmetic must lower natively when the host supports FEAT_FP16"
        );
    }
    #[test]
    fn lowers_clmul_runtime() {
        assert_clmul_runtime(
            "pmpyw_identity",
            SrcOperand::Reg(x(2)),
            SrcOperand::Reg(x(3)),
            1,
            0x1234_5678,
            32,
            1,
            false,
            (0, 0),
            true,
        );
        assert_clmul_runtime(
            "pmpyw_high_word",
            SrcOperand::Reg(x(2)),
            SrcOperand::Reg(x(3)),
            0x8000_0000,
            0x8000_0000,
            32,
            1,
            false,
            (0, 0),
            true,
        );
        assert_clmul_runtime(
            "pmpyw_acc",
            SrcOperand::Reg(x(2)),
            SrcOperand::Reg(x(3)),
            0x1234_5678,
            2,
            32,
            1,
            true,
            (0xaaaa_aaaa, 0x5555_5555),
            true,
        );
        assert_clmul_runtime(
            "vpmpyh_interleaved",
            SrcOperand::Reg(x(2)),
            SrcOperand::Reg(x(3)),
            0x0001_ffff,
            0x0003_0002,
            16,
            2,
            false,
            (0, 0),
            true,
        );
        assert_clmul_runtime(
            "immediates_without_high_dst",
            SrcOperand::Imm(0x1234_5678),
            SrcOperand::Imm64(2),
            0x1234_5678,
            2,
            32,
            1,
            false,
            (0, 0x9999_9999),
            false,
        );
    }
    #[test]
    fn rejects_clmul_unsupported_shape() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::ClMul {
                dst: x(0),
                dst_hi: Some(x(1)),
                src1: SrcOperand::Reg(x(2)),
                src2: SrcOperand::Reg(x(3)),
                elem_bits: 8,
                lanes: 4,
                acc: false,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();
        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn lowers_rep_stos_runtime() {
        assert_rep_stos_runtime(
            "b2_count3",
            MemWidth::B2,
            0,
            1,
            2,
            0x9000,
            0xabcd,
            3,
            0x1122_3344_5566_7788,
        );
        assert_rep_stos_runtime(
            "b8_count1",
            MemWidth::B8,
            0,
            1,
            2,
            0x9010,
            0x0123_4567_89ab_cdef,
            1,
            0,
        );
        assert_rep_stos_runtime(
            "zero_count",
            MemWidth::B4,
            0,
            1,
            2,
            0x9020,
            0xdead_cafe,
            0,
            0x0123_4567_89ab_cdef,
        );
        assert_rep_stos_runtime("dst_src_alias", MemWidth::B8, 0, 0, 2, 0x9030, 0x9030, 1, 0);
    }
    #[test]
    fn rejects_rep_stos_unsupported_width() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::RepStos {
                dst: x(0),
                src: x(1),
                count: x(2),
                width: MemWidth::B16,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();
        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn lowers_io_in_runtime() {
        let old_nzcv = 0b1011;
        for (label, dst, dst_reg, width) in [
            ("arm_dst", x(1), 1, MemWidth::B1),
            ("x86_dst", x86(X86Reg::Rax), 0, MemWidth::B4),
            ("apx_dst", x86(X86Reg::R18), 18, MemWidth::B2),
        ] {
            let code = lower_single_op(OpKind::IoIn {
                dst,
                port: x86(X86Reg::Rdx),
                width,
            });
            let regs = [
                (dst_reg, 0xffff_ffff_ffff_ffff),
                (2, 0x03f8),
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
                (18, 0x1818_1818_1818_1818),
            ];
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

            assert_eq!(out[dst_reg as usize], 0, "{label}: destination");
            assert_eq!(out[2], 0x03f8, "{label}: port preserved");
            assert_eq!(out[16], 0x1616_1616_1616_1616, "{label}: x16");
            assert_eq!(out[17], 0x1717_1717_1717_1717, "{label}: x17");
            assert_eq!(out_nzcv, old_nzcv, "{label}: NZCV");
            assert_eq!(sp, 0x8000, "{label}: stack pointer");
        }
    }
    #[test]
    fn lowers_io_out_runtime() {
        let code = lower_single_op(OpKind::IoOut {
            port: x86(X86Reg::Rdx),
            value: x86(X86Reg::Rax),
            width: MemWidth::B2,
        });
        let old_nzcv = 0b1101;
        let regs = [
            (0, 0xfeed_face_cafe_beef),
            (2, 0x03f8),
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
        ];
        let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

        assert_eq!(out[0], 0xfeed_face_cafe_beef);
        assert_eq!(out[2], 0x03f8);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);
    }
    #[test]
    fn rejects_io_unsupported_widths() {
        for kind in [
            OpKind::IoIn {
                dst: x86(X86Reg::Rax),
                port: x86(X86Reg::Rdx),
                width: MemWidth::B8,
            },
            OpKind::IoOut {
                port: x86(X86Reg::Rdx),
                value: x86(X86Reg::Rax),
                width: MemWidth::B8,
            },
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.push_op(0, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();
            let mut lowerer = Aarch64Lowerer::new();
            let err = lowerer.lower_function(&func).unwrap_err();
            assert!(matches!(err, LowerError::UnsupportedOp { .. }));
        }
    }
    #[test]
    fn fuses_lifted_ldclr_sequence() {
        let inverted = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
                dst: inverted,
                src: x(2),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0,
            OpKind::AtomicRmw {
                dst: x(0),
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
        expected.extend_from_slice(&enc_atomic_rmw(3, 0, 1, 0, 0b001).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_ldclr_base_offset_runtime() {
        let mem_addr = 0x9000_u64;
        let offset = 0x28_i64;
        let base = mem_addr - offset as u64;
        let src_value = 0x00f0_u64;
        let mem_value = 0x0ff0_u64;
        let inverted = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
                dst: inverted,
                src: x(2),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0,
            OpKind::AtomicRmw {
                dst: x(0),
                addr: Address::BaseOffset {
                    base: x(1),
                    offset,
                    disp_size: DispSize::Auto,
                },
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

        let regs = [
            (1, base),
            (2, src_value),
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
        ];
        let old_nzcv = 0b1010;
        let (out, out_nzcv, sp, mem) =
            run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

        assert_eq!(out[0], mem_value);
        assert_eq!(out[1], base);
        assert_eq!(out[2], src_value);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);
        assert_eq!(mem, mem_value & !src_value);
    }
    #[test]
    fn fuses_lifted_ldclr_base_index_scale_runtime() {
        let mem_addr = 0x9000_u64;
        let index = 5_u64;
        let disp = 0x28_i32;
        let base = mem_addr - index * 4 - disp as u64;
        let src_value = 0x00f0_u64;
        let mem_value = 0x0ff0_u64;
        let inverted = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Not {
                dst: inverted,
                src: x(2),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0,
            OpKind::AtomicRmw {
                dst: x(0),
                addr: Address::BaseIndexScale {
                    base: Some(x(1)),
                    index: x(3),
                    scale: 4,
                    disp,
                    disp_size: DispSize::Auto,
                },
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

        let regs = [
            (1, base),
            (2, src_value),
            (3, index),
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
        ];
        let old_nzcv = 0b0010;
        let (out, out_nzcv, sp, mem) =
            run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

        assert_eq!(out[0], mem_value);
        assert_eq!(out[1], base);
        assert_eq!(out[2], src_value);
        assert_eq!(out[3], index);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);
        assert_eq!(mem, mem_value & !src_value);
    }
    #[test]
    fn lowers_cas_lifted_shape_direct() {
        let success = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Cas {
                dst: x(2),
                success,
                addr: Address::Direct(x(1)),
                expected: x(2),
                new_val: x(0),
                width: MemWidth::B8,
                order: MemoryOrder::AcqRel,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_cas(3, 1, 1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_cas_lifted_shape_base_offset_runtime() {
        let mem_addr = 0x9000_u64;
        let offset = 0x70_i64;
        let base = mem_addr - offset as u64;
        let old_value = 0x1111_2222_3333_4444;
        let new_value = 0x5555_6666_7777_8888;
        let success = VReg::virt(0);
        let code = lower_single_op(OpKind::Cas {
            dst: x(2),
            success,
            addr: Address::BaseOffset {
                base: x(1),
                offset,
                disp_size: DispSize::Auto,
            },
            expected: x(2),
            new_val: x(0),
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        });

        let regs = [
            (0, new_value),
            (1, base),
            (2, old_value),
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
        ];
        let old_nzcv = 0b1110;
        let (out, out_nzcv, sp, mem) =
            run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, old_value, MemWidth::B8);

        assert_eq!(out[0], new_value);
        assert_eq!(out[1], base);
        assert_eq!(out[2], old_value);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);
        assert_eq!(mem, new_value);
    }
    #[test]
    fn lowers_cas_lifted_shape_base_index_scale_runtime() {
        let mem_addr = 0x9000_u64;
        let index = 11_u64;
        let disp = 0x38_i32;
        let base = mem_addr - index * 8 - disp as u64;
        let old_value = 0x1111_2222_3333_4444;
        let new_value = 0x9999_aaaa_bbbb_cccc;
        let success = VReg::virt(0);
        let code = lower_single_op(OpKind::Cas {
            dst: x(2),
            success,
            addr: Address::BaseIndexScale {
                base: Some(x(1)),
                index: x(3),
                scale: 8,
                disp,
                disp_size: DispSize::Auto,
            },
            expected: x(2),
            new_val: x(0),
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        });

        let regs = [
            (0, new_value),
            (1, base),
            (2, old_value),
            (3, index),
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
        ];
        let old_nzcv = 0b1101;
        let (out, out_nzcv, sp, mem) =
            run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, old_value, MemWidth::B8);

        assert_eq!(out[0], new_value);
        assert_eq!(out[1], base);
        assert_eq!(out[2], old_value);
        assert_eq!(out[3], index);
        assert_eq!(out[16], 0x1616_1616_1616_1616);
        assert_eq!(out[17], 0x1717_1717_1717_1717);
        assert_eq!(out_nzcv, old_nzcv);
        assert_eq!(sp, 0x8000);
        assert_eq!(mem, new_value);
    }
    #[test]
    fn lowers_cas_with_observable_success() {
        assert_cas_lowering(
            "cas_observable_success",
            2,
            Some(3),
            2,
            0,
            MemWidth::B8,
            0x1111_2222_3333_4444,
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
        );
        assert_cas_lowering(
            "cas_observable_failure",
            2,
            Some(3),
            2,
            0,
            MemWidth::B8,
            0x9999_aaaa_bbbb_cccc,
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
        );
        assert_cas_lowering(
            "cas_observable_success_aliases_destination",
            2,
            Some(2),
            2,
            0,
            MemWidth::B8,
            0x1111_2222_3333_4444,
            0x1111_2222_3333_4444,
            0x5555_6666_7777_8888,
        );
    }
    #[test]
    fn fuses_lifted_extract_sequence() {
        let lo = VReg::virt(0);
        let hi = VReg::virt(1);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Shr {
                dst: lo,
                src: x(2),
                amount: SrcOperand::Imm(13),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: hi,
                src: x(1),
                amount: SrcOperand::Imm(51),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Or {
                dst: x(0),
                src1: lo,
                src2: SrcOperand::Reg(hi),
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
        expected.extend_from_slice(&enc_extract(1, 1, 2, 13).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_cls_w_with_masked_imms_as_cls() {
        let sign_mask = VReg::virt(0);
        let normalized = VReg::virt(1);
        let leading = VReg::virt(2);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Sar {
                dst: sign_mask,
                src: x(1),
                amount: SrcOperand::Imm64(95),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Xor {
                dst: normalized,
                src1: x(1),
                src2: SrcOperand::Reg(sign_mask),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Clz {
                dst: leading,
                src: normalized,
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Sub {
                dst: x(0),
                src1: leading,
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
        expected.extend_from_slice(&enc_dp1(0, 0b000101).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_logical_identity_x_same_reg_as_noop() {
        let cases = [
            OpKind::And {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm64(-1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Or {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Xor {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::And {
                dst: x(0),
                src1: x(0),
                src2: SrcOperand::Reg(x(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Or {
                dst: x(0),
                src1: x(0),
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
    fn lowers_logical_zero_base_x_same_dst_as_noop() {
        let cases = [
            OpKind::Or {
                dst: x(0),
                src1: VReg::Imm(0),
                src2: SrcOperand::Reg(x(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            OpKind::Xor {
                dst: x(0),
                src1: VReg::Imm(0),
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
    fn lowers_sparse_logical_immediate_via_scratch() {
        assert_sparse_logic_imm_lowering(
            "orr_x_sparse_imm",
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(0x55),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            1,
            0x8000_0000_0000_1000,
            Some(0),
            0x8000_0000_0000_1055,
            0b0011,
        );
        assert_sparse_logic_imm_lowering(
            "orr_w16_sparse_imm",
            OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm(0x1234),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            1,
            0xffff_0000_0000_00c0,
            Some(0),
            0x12f4,
            0b0011,
        );
    }
    #[test]
    fn lowers_inverted_sparse_logical_immediate_via_scratch() {
        assert_sparse_logic_imm_lowering(
            "andnot_x_sparse_inverse_imm",
            OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Imm64(!0x55_i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            1,
            0x8000_0000_0000_10d5,
            Some(0),
            0x55,
            0b0011,
        );
    }
    #[test]
    fn lowers_zero_extend_w8_to_w16_as_uxtb() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::ZeroExtend {
                dst: x(0),
                src: x(1),
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
        expected.extend_from_slice(&enc_bitfield(0, 0b10, 0, 7).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_sign_extend_w8_to_w16_as_sxtb_uxth() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::SignExtend {
                dst: x(0),
                src: x(1),
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
        expected.extend_from_slice(&enc_bitfield(0, 0b00, 0, 7).to_le_bytes());
        expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_x86_w16_extends_as_partial_register_merges_runtime() {
        let rax = 0xaaaa_bbbb_cccc_dd00;
        let rcx = 0x1111_2222_3333_44ab;
        let rdx = 0xdddd_eeee_ffff_0080;
        let rbx = 0xbbbb_cccc_dddd_1234;
        let code = lower_ops(vec![
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            OpKind::SignExtend {
                dst: x86(X86Reg::Rdx),
                src: x86(X86Reg::Rdx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            // O2 constant propagation may replace an architectural source
            // with an immediate even though MOVSX/MOVZX encode r/m inputs.
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rbx),
                src: VReg::Imm(0x12ab),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
        ]);
        let old_nzcv = 0b1010;
        let (out, out_nzcv, sp) = run_aarch64_code(
            &code,
            &[
                (0, rax),
                (1, rcx),
                (2, rdx),
                (3, rbx),
                (15, 0x1515_1515_1515_1515),
            ],
            old_nzcv,
        );

        assert_eq!(out[0], 0xaaaa_bbbb_cccc_00ab, "MOVZX ax,cl merge");
        assert_eq!(out[1], rcx, "MOVZX source");
        assert_eq!(out[2], 0xdddd_eeee_ffff_ff80, "CBW-style alias merge");
        assert_eq!(out[3], 0xbbbb_cccc_dddd_00ab, "constant-folded merge");
        assert_eq!(out[15], 0x1515_1515_1515_1515, "mapped sentinel");
        assert_eq!(out_nzcv, old_nzcv, "extensions preserve flags");
        assert_eq!(sp, 0x8000, "scratch spills balance the stack");
    }
    #[test]
    fn lowers_set_cf_runtime() {
        for (label, value, old_nzcv) in [
            ("set clear carry", true, 0b1001),
            ("set existing carry", true, 0b0110),
            ("clear set carry", false, 0b1111),
            ("clear existing clear", false, 0b0101),
        ] {
            let code = lower_single_op(OpKind::SetCF { value });
            let sentinels = [
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
                (15, 0x1515_1515_1515_1515),
            ];
            let expected_nzcv = (old_nzcv & !0b0010) | if value { 0b0010 } else { 0 };

            let (out, out_nzcv, sp) = run_aarch64_code(&code, &sentinels, old_nzcv);
            assert_eq!(out_nzcv, expected_nzcv, "{label}: NZCV");
            assert_eq!(sp, 0x8000, "{label}: stack restored");
            for (reg, value) in sentinels {
                assert_eq!(out[reg as usize], value, "{label}: x{reg} restored");
            }
        }
    }
    #[test]
    fn lowers_cmc_cf_as_cfinv() {
        let code = lower_ops_with_flagm_features(vec![OpKind::CmcCF], true, true);

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_flagm(0b000).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn rejects_breakpoint_op_to_deopt() {
        // A guest Breakpoint must bail to the interpreter, not lower to a host BRK
        // (which raises SIGTRAP and kills the emulator). (#16)
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(0, OpKind::Breakpoint);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn rejects_undefined_op_to_deopt() {
        // A guest Undefined must bail to the interpreter, not lower to a host UDF
        // (which raises SIGILL). (#16)
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Undefined {
                opcode: 0xffff_ffff,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn rejects_swi_op_in_native_lowerer() {
        for imm in [0x1234, 0x1_0000] {
            let func = func_with_ops(vec![OpKind::Swi { imm }]);

            let mut lowerer = Aarch64Lowerer::new();
            let err = lowerer.lower_function(&func).unwrap_err();
            assert!(
                matches!(err, LowerError::UnsupportedOp { .. }),
                "SWI imm {imm:#x}: {err:?}"
            );
            assert_eq!(lowerer.finalize().unwrap(), Vec::<u8>::new());
        }
    }
    #[test]
    fn rejects_breakpoint_trap_terminator_to_deopt() {
        // A Breakpoint trap terminator must bail to the interpreter, not host BRK. (#16)
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Breakpoint,
        });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn rejects_system_call_trap_terminator_in_native_lowerer() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::SystemCall,
        });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
        assert_eq!(lowerer.finalize().unwrap(), Vec::<u8>::new());
    }
    #[test]
    fn rejects_halt_trap_terminator_in_native_lowerer() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
        assert_eq!(lowerer.finalize().unwrap(), Vec::<u8>::new());
    }
    #[test]
    fn rejects_undefined_trap_terminator_to_deopt() {
        // An Undefined trap terminator must bail to the interpreter, not host UDF. (#16)
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Undefined,
        });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn rejects_unreachable_terminator_to_deopt() {
        // Unreachable must bail to the interpreter, not host UDF (SIGILL). (#16)
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.set_terminator(Terminator::Unreachable);
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn direct_guest_calls_require_explicit_frontier_exit_mode() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let continuation = builder.create_block(0x1004);
        builder.set_terminator(Terminator::Call {
            target: CallTarget::GuestAddr(0x2345_6780),
            args: Vec::new(),
            continuation,
        });
        builder.switch_to_block(continuation);
        builder.set_terminator(Terminator::Return { values: Vec::new() });
        let func = builder.finish();

        let mut disabled = Aarch64Lowerer::new();
        assert!(matches!(
            disabled.lower_function(&func),
            Err(LowerError::UnsupportedOp { .. })
        ));

        let mut enabled = Aarch64Lowerer::new();
        enabled.set_guest_call_exits(true);
        enabled
            .lower_function(&func)
            .expect("direct guest call must lower as a configured native exit");
        let code = enabled.finalize().expect("finalize direct call exit");
        assert!(!code.is_empty());
        assert!(
            code.windows(4)
                .any(|word| word == 0xd65f_03c0u32.to_le_bytes())
        );
    }
    #[test]
    fn lowers_branch_terminator_as_b() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        let target = builder.create_block(4);
        builder.set_terminator(Terminator::Branch { target });
        builder.switch_to_block(target);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let result = lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_b(1).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(result.block_offsets.get(&target), Some(&4));
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_immediate_switch_as_single_b() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        let case0 = builder.create_block(4);
        let case1 = builder.create_block(8);
        let default = builder.create_block(12);
        builder.set_terminator(Terminator::Switch {
            index: VReg::Imm(1),
            targets: vec![case0, case1],
            default,
        });
        builder.switch_to_block(case0);
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.switch_to_block(case1);
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.switch_to_block(default);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_b(2).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    // Regression for issue #18: a SMIR IndirectBranch (guest `BR Xn` / `RET`) must
    // NOT lower to a native `br Xn` — the identity-mapped register holds
    // guest-controlled data, so branching through it is a host control-flow hijack.
    // The default remains fail-closed; a separately gated AArch32 mode records
    // a dispatcher exit without executing the guest value as a host address.
    #[test]
    fn rejects_indirect_branch_to_deopt() {
        for target in [x(3), x86(X86Reg::R16), VReg::Imm(0), x86(X86Reg::R31)] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.set_terminator(Terminator::IndirectBranch {
                target,
                possible_targets: vec![],
            });
            let func = builder.finish();

            let mut lowerer = Aarch64Lowerer::new();
            assert!(
                lowerer.lower_function(&func).is_err(),
                "IndirectBranch to {target:?} must bail to the interpreter, not emit a native br"
            );
        }
    }
    #[test]
    fn configured_aarch32_indirect_exit_rejects_unvalidated_shapes() {
        for (target, possible_targets) in [(VReg::Imm(0), Vec::new()), (x(0), vec![BlockId(1)])] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0);
            builder.set_terminator(Terminator::IndirectBranch {
                target,
                possible_targets,
            });
            let mut lowerer = Aarch64Lowerer::new();
            lowerer.set_guest_indirect_exits(true);
            assert!(matches!(
                lowerer.lower_function(&builder.finish()),
                Err(LowerError::UnsupportedOp { .. }) | Err(LowerError::InvalidRegister(_))
            ));
        }
    }
    #[test]
    fn lowers_prefetch_pcrel_as_prfm_literal() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Prefetch {
                addr: Address::PcRel {
                    offset: 12,
                    disp_size: DispSize::Auto,
                    base: None,
                },
                write: false,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_prfm_lit(0, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_prefetch_base_offset_as_prfm() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Prefetch {
                addr: Address::BaseOffset {
                    base: x(1),
                    offset: 24,
                    disp_size: DispSize::Auto,
                },
                write: false,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_ldst_uimm(3, 0b10, 3).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_prefetch_write_base_index_scale_as_prfm_reg() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Prefetch {
                addr: Address::BaseIndexScale {
                    base: Some(x(1)),
                    index: x(2),
                    scale: 8,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                write: true,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&(enc_ldst_reg(3, 0b10, 2, 0b011, 1) | 0b10000).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_ubfiz_sequence() {
        let extracted = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfx {
                dst: extracted,
                src: x(1),
                lsb: 0,
                width_bits: 8,
                sign_extend: false,
                op_width: OpWidth::W64,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: x(0),
                src: extracted,
                amount: SrcOperand::Imm(4),
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
        expected.extend_from_slice(&enc_bitfield(1, 0b10, 60, 7).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_sbfiz_w_sequence() {
        let extracted = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfx {
                dst: extracted,
                src: x(1),
                lsb: 0,
                width_bits: 8,
                sign_extend: true,
                op_width: OpWidth::W32,
            },
        );
        builder.push_op(
            0,
            OpKind::Shl {
                dst: x(0),
                src: extracted,
                amount: SrcOperand::Imm(8),
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
        expected.extend_from_slice(&enc_bitfield(0, 0b00, 24, 7).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_lifted_bfxil_sequence() {
        let extracted = VReg::virt(0);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Bfx {
                dst: extracted,
                src: x(1),
                lsb: 8,
                width_bits: 8,
                sign_extend: false,
                op_width: OpWidth::W64,
            },
        );
        builder.push_op(
            0,
            OpKind::Bfi {
                dst: x(0),
                dst_in: x(0),
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

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_bitfield(1, 0b01, 8, 15).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_bfxil_when_dst_aliases_src() {
        assert_fused_bfxil_lowering(
            "bfxil_x_dst_aliases_src",
            0,
            1,
            0xaaaa_bbbb_ccdd_eeff,
            0,
            0x1234_5678_9abc_def0,
            8,
            8,
            OpWidth::W64,
        );
        assert_fused_bfxil_lowering(
            "bfxil_w_dst_aliases_src",
            0,
            1,
            0xfedc_ba98,
            0,
            0x7654_3210,
            12,
            8,
            OpWidth::W32,
        );
    }
    #[test]
    fn fuses_ldpsw_pair_sequence() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Load {
                dst: x(0),
                addr: Address::Direct(x(1)),
                width: MemWidth::B8,
                sign: SignExtend::Sign,
            },
        );
        builder.push_op(
            0,
            OpKind::Load {
                dst: x(2),
                addr: Address::BaseOffset {
                    base: x(1),
                    offset: 8,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B8,
                sign: SignExtend::Sign,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_ldp(0b01, 0b10, true, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_ldpsw_pre_index_sequence() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0,
            OpKind::Load {
                dst: x(0),
                addr: Address::Direct(x(1)),
                width: MemWidth::B8,
                sign: SignExtend::Sign,
            },
        );
        builder.push_op(
            0,
            OpKind::Load {
                dst: x(2),
                addr: Address::BaseOffset {
                    base: x(1),
                    offset: 8,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B8,
                sign: SignExtend::Sign,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_ldp(0b01, 0b11, true, 2).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn fuses_ldpsw_post_index_sequence() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(
            0,
            OpKind::Load {
                dst: x(0),
                addr: Address::Direct(x(1)),
                width: MemWidth::B8,
                sign: SignExtend::Sign,
            },
        );
        builder.push_op(
            0,
            OpKind::Load {
                dst: x(2),
                addr: Address::BaseOffset {
                    base: x(1),
                    offset: 8,
                    disp_size: DispSize::Auto,
                },
                width: MemWidth::B8,
                sign: SignExtend::Sign,
            },
        );
        builder.push_op(
            0,
            OpKind::Add {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Imm(-8),
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
        expected.extend_from_slice(&enc_ldp(0b01, 0b01, true, -2).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
    #[test]
    fn lowers_x86_w16_unary_count_partial_write_alias_matrix() {
        let rax = x86(X86Reg::Rax);
        let rbx = x86(X86Reg::Rbx);
        let rax_value = 0xaaaa_bbbb_cccc_1000;
        let rbx_value = 0xbbbb_cccc_dddd_00f0;
        let cases = [
            (
                "clz-distinct",
                OpKind::Clz {
                    dst: rbx,
                    src: rax,
                    width: OpWidth::W16,
                },
                3,
                (rbx_value & !0xffff) | 3,
            ),
            (
                "ctz-in-place",
                OpKind::Ctz {
                    dst: rax,
                    src: rax,
                    width: OpWidth::W16,
                },
                0,
                (rax_value & !0xffff) | 12,
            ),
            (
                "popcnt-distinct",
                OpKind::Popcnt {
                    dst: rbx,
                    src: rax,
                    width: OpWidth::W16,
                },
                3,
                (rbx_value & !0xffff) | 1,
            ),
        ];
        let sentinels = [
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
            (14, 0x1414_1414_1414_1414),
        ];

        for (label, op, dst, expected) in cases {
            let code = lower_single_op(op);
            let mut regs = sentinels.to_vec();
            regs.extend([(0, rax_value), (3, rbx_value)]);
            let old_nzcv = 0b1011;
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

            assert_eq!(out[dst], expected, "{label}: result");
            if dst != 0 {
                assert_eq!(out[0], rax_value, "{label}: source");
            }
            for (index, value) in sentinels {
                assert_eq!(out[index as usize], value, "{label}: x{index} scratch");
            }
            assert_eq!(out_nzcv, old_nzcv, "{label}: NZCV");
            assert_eq!(sp, 0x8000, "{label}: stack");
        }
    }
    #[test]
    fn lowers_x86_count_w16_nf_partial_write_alias_matrix() {
        let initial = [
            0xaaaa_bbbb_cccc_f0f0,
            0x1111_2222_3333_0100,
            0xdddd_eeee_ffff_0000,
            0xbbbb_cccc_dddd_8000,
        ];
        let reg = |index: u8| match index {
            0 => x86(X86Reg::Rax),
            1 => x86(X86Reg::Rcx),
            2 => x86(X86Reg::Rdx),
            3 => x86(X86Reg::Rbx),
            _ => unreachable!("unexpected test register x{index}"),
        };
        let cases = [
            ("popcnt-distinct", X86CountKind::Popcnt, 0, 3, 1),
            ("tzcnt-dst-src-alias", X86CountKind::Tzcnt, 1, 1, 8),
            ("lzcnt-distinct", X86CountKind::Lzcnt, 3, 0, 0),
        ];
        let sentinels = [
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
            (15, 0x1515_1515_1515_1515),
            (14, 0x1414_1414_1414_1414),
        ];

        for (label, kind, dst, src, result) in cases {
            let code = lower_single_op(OpKind::X86Count {
                dst: reg(dst),
                src: reg(src),
                width: OpWidth::W16,
                kind,
                flags: FlagUpdate::None,
            });
            let expected = (initial[dst as usize] & !0xffff) | result;
            let mut regs = sentinels.to_vec();
            regs.extend(
                initial
                    .iter()
                    .enumerate()
                    .map(|(index, value)| (index as u8, *value)),
            );
            let old_nzcv = 0b1011;
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
            assert_eq!(out_nzcv, old_nzcv, "{label}: NZCV");
            assert_eq!(sp, 0x8000, "{label}: stack");
        }
    }
    #[test]
    fn rejects_malformed_x86_count_shapes() {
        for op in [
            OpKind::X86Count {
                dst: x(0),
                src: x(1),
                width: OpWidth::W8,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::None,
            },
            OpKind::X86Count {
                dst: x(0),
                src: x(1),
                width: OpWidth::W64,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::All,
            },
            OpKind::X86Count {
                dst: x(0),
                src: x(1),
                width: OpWidth::W32,
                kind: X86CountKind::Lzcnt,
                flags: FlagUpdate::Specific(FlagSet::OF),
            },
            OpKind::X86Count {
                dst: x(0),
                src: VReg::Imm(1),
                width: OpWidth::W32,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::None,
            },
        ] {
            assert!(try_lower_single_op(op).is_err());
        }
    }
