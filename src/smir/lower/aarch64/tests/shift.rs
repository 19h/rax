//! tests::shift tests

use super::*;
use crate::smir::lower::aarch64::*;

#[test]
fn lowers_t16_selective_nzcv_logic_multiply_and_immediate_shifts_exhaustively() {
    let nz = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF));
    let nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));
    let values = [0_u64, 1, 0x4000_0001, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff];
    let scratch16 = 0x1616_1616_1616_1616;
    let scratch17 = 0x1717_1717_1717_1717;

    for logic in 0..4 {
        let op = match logic {
            0 => OpKind::And {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
                width: OpWidth::W32,
                flags: nz,
            },
            1 => OpKind::Or {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
                width: OpWidth::W32,
                flags: nz,
            },
            2 => OpKind::Xor {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
                width: OpWidth::W32,
                flags: nz,
            },
            _ => OpKind::AndNot {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Reg(x(2)),
                width: OpWidth::W32,
                flags: nz,
            },
        };
        let code = lower_single_op(op);
        for &lhs in &values {
            for &rhs in &values {
                let expected = match logic {
                    0 => lhs & rhs,
                    1 => lhs | rhs,
                    2 => lhs ^ rhs,
                    _ => lhs & !rhs,
                } & u64::from(u32::MAX);
                for old_nzcv in 0_u8..16 {
                    let (out, out_nzcv, sp) = run_aarch64_code(
                        &code,
                        &[(1, lhs), (2, rhs), (16, scratch16), (17, scratch17)],
                        old_nzcv,
                    );
                    assert_eq!(out[0], expected, "logic={logic} lhs={lhs:#x} rhs={rhs:#x}");
                    assert_eq!(
                        out_nzcv,
                        expected_logic_source_nzcv(old_nzcv, expected, OpWidth::W32, nz),
                        "logic={logic} lhs={lhs:#x} rhs={rhs:#x} old={old_nzcv:#x}"
                    );
                    assert_eq!(out[16], scratch16);
                    assert_eq!(out[17], scratch17);
                    assert_eq!(sp, 0x8000);
                }
            }
        }
    }

    let movs_code = lower_ops(vec![
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Imm(0x80),
            width: OpWidth::W32,
        },
        OpKind::And {
            dst: x(0),
            src1: x(0),
            src2: SrcOperand::Imm(-1),
            width: OpWidth::W32,
            flags: nz,
        },
    ]);
    let tst_code = lower_single_op(OpKind::And {
        dst: VReg::virt(0),
        src1: x(1),
        src2: SrcOperand::Reg(x(2)),
        width: OpWidth::W32,
        flags: nz,
    });
    for (name, code, expected) in [
        ("movs-immediate", &movs_code, 0x80),
        ("tst-flag-only", &tst_code, 0),
    ] {
        for old_nzcv in 0_u8..16 {
            let (out, out_nzcv, sp) = run_aarch64_code(
                code,
                &[
                    (1, 0xffff_0000),
                    (2, 0x0000_ffff),
                    (16, scratch16),
                    (17, scratch17),
                ],
                old_nzcv,
            );
            assert_eq!(
                out_nzcv,
                expected_logic_source_nzcv(old_nzcv, expected, OpWidth::W32, nz),
                "{name} old={old_nzcv:#x}"
            );
            if name == "movs-immediate" {
                assert_eq!(out[0], expected);
            }
            assert_eq!(out[16], scratch16);
            assert_eq!(out[17], scratch17);
            assert_eq!(sp, 0x8000);
        }
    }

    let mul_code = lower_single_op(OpKind::MulU {
        dst_lo: x(0),
        dst_hi: None,
        src1: x(1),
        src2: SrcOperand::Reg(x(0)),
        width: OpWidth::W32,
        flags: nz,
    });
    for &lhs in &values {
        for &rhs in &values {
            let expected = lhs.wrapping_mul(rhs) & u64::from(u32::MAX);
            for old_nzcv in 0_u8..16 {
                let (out, out_nzcv, sp) = run_aarch64_code(
                    &mul_code,
                    &[(0, rhs), (1, lhs), (16, scratch16), (17, scratch17)],
                    old_nzcv,
                );
                assert_eq!(out[0], expected, "mul {lhs:#x}*{rhs:#x}");
                assert_eq!(
                    out_nzcv,
                    expected_logic_source_nzcv(old_nzcv, expected, OpWidth::W32, nz),
                    "mul {lhs:#x}*{rhs:#x} old={old_nzcv:#x}"
                );
                assert_eq!(out[16], scratch16);
                assert_eq!(out[17], scratch17);
                assert_eq!(sp, 0x8000);
            }
        }
    }

    for shift in [ShiftOp::Lsl, ShiftOp::Lsr, ShiftOp::Asr] {
        let amounts: &[i64] = if shift == ShiftOp::Lsl {
            &[1, 2, 31]
        } else {
            &[1, 2, 31, 32]
        };
        for &amount in amounts {
            let kind = match shift {
                ShiftOp::Lsl => OpKind::Shl {
                    dst: x(0),
                    src: x(1),
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W32,
                    flags: nzc,
                },
                ShiftOp::Lsr => OpKind::Shr {
                    dst: x(0),
                    src: x(1),
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W32,
                    flags: nzc,
                },
                ShiftOp::Asr => OpKind::Sar {
                    dst: x(0),
                    src: x(1),
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W32,
                    flags: nzc,
                },
                _ => unreachable!(),
            };
            let code = lower_single_op(kind);
            for &source in &values {
                let source = source as u32;
                let result = match shift {
                    ShiftOp::Lsl => source.wrapping_shl(amount as u32),
                    ShiftOp::Lsr if amount >= 32 => 0,
                    ShiftOp::Lsr => source >> amount,
                    ShiftOp::Asr => ((source as i32) >> amount.min(31)) as u32,
                    _ => unreachable!(),
                };
                let carry = match shift {
                    ShiftOp::Lsl => (source >> (32 - amount)) & 1,
                    ShiftOp::Lsr | ShiftOp::Asr => (source >> (amount - 1)) & 1,
                    _ => unreachable!(),
                } as u8;
                let produced =
                    (((result >> 31) & 1) as u8) << 3 | ((result == 0) as u8) << 2 | carry << 1;
                for old_nzcv in 0_u8..16 {
                    let (out, out_nzcv, sp) = run_aarch64_code(
                        &code,
                        &[(1, u64::from(source)), (16, scratch16), (17, scratch17)],
                        old_nzcv,
                    );
                    assert_eq!(out[0], u64::from(result), "{shift:?} #{amount} {source:#x}");
                    assert_eq!(
                        out_nzcv,
                        produced | (old_nzcv & 1),
                        "{shift:?} #{amount} {source:#x} old={old_nzcv:#x}"
                    );
                    assert_eq!(out[16], scratch16);
                    assert_eq!(out[17], scratch17);
                    assert_eq!(sp, 0x8000);
                }
            }
        }
    }
}
#[test]
fn lowers_t16_t32_register_shifts_with_exact_low_byte_flags_and_aliasing() {
    fn expected(value: u32, raw_count: u64, shift: ShiftOp, carry_in: bool) -> (u32, bool) {
        let count = (raw_count & 0xff) as u32;
        if count == 0 {
            return (value, carry_in);
        }
        match shift {
            ShiftOp::Lsl if count < 32 => (value << count, (value >> (32 - count)) & 1 != 0),
            ShiftOp::Lsl if count == 32 => (0, value & 1 != 0),
            ShiftOp::Lsl => (0, false),
            ShiftOp::Lsr if count < 32 => (value >> count, (value >> (count - 1)) & 1 != 0),
            ShiftOp::Lsr if count == 32 => (0, value >> 31 != 0),
            ShiftOp::Lsr => (0, false),
            ShiftOp::Asr if count < 32 => (
                ((value as i32) >> count) as u32,
                (value >> (count - 1)) & 1 != 0,
            ),
            ShiftOp::Asr => {
                let sign = value >> 31 != 0;
                (if sign { u32::MAX } else { 0 }, sign)
            }
            ShiftOp::Ror => {
                let result = value.rotate_right(count % 32);
                (result, result >> 31 != 0)
            }
            ShiftOp::Rrx => unreachable!(),
        }
    }

    let nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));
    let values = [0_u32, 1, 0x7fff_ffff, 0x8000_0000, 0x8000_0001, u32::MAX];
    let counts = [
        0_u64,
        1,
        2,
        31,
        32,
        33,
        63,
        64,
        127,
        128,
        255,
        256,
        257,
        u64::MAX,
    ];
    let scratch = [
        (13, 0x1313_1313_1313_1313),
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];

    for shift in [ShiftOp::Lsl, ShiftOp::Lsr, ShiftOp::Asr, ShiftOp::Ror] {
        let code = lower_single_op(OpKind::ArmRegShift {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Reg(x(1)),
            shift,
            width: OpWidth::W32,
            flags: nzc,
        });
        for &value in &values {
            for &count in &counts {
                for old_nzcv in 0_u8..16 {
                    let (result, carry) = expected(value, count, shift, old_nzcv & 0b0010 != 0);
                    let mut regs = vec![(0, u64::from(value)), (1, count)];
                    regs.extend_from_slice(&scratch);
                    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
                    let expected_nzcv = ((result >> 31) as u8) << 3
                        | ((result == 0) as u8) << 2
                        | (carry as u8) << 1
                        | (old_nzcv & 1);
                    assert_eq!(
                        out[0],
                        u64::from(result),
                        "{shift:?} value={value:#x} count={count:#x}"
                    );
                    assert_eq!(
                        out_nzcv, expected_nzcv,
                        "{shift:?} value={value:#x} count={count:#x} old={old_nzcv:#x}"
                    );
                    for &(reg, sentinel) in &scratch {
                        assert_eq!(out[usize::from(reg)], sentinel, "x{reg}");
                    }
                    assert_eq!(sp, 0x8000);
                }
            }
        }

        let alias_code = lower_single_op(OpKind::ArmRegShift {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Reg(x(0)),
            shift,
            width: OpWidth::W32,
            flags: nzc,
        });
        for &value in &values {
            let (result, carry) = expected(value, u64::from(value), shift, true);
            let (out, out_nzcv, sp) =
                run_aarch64_code(&alias_code, &[(0, u64::from(value))], 0b0011);
            assert_eq!(out[0], u64::from(result), "alias {shift:?} {value:#x}");
            assert_eq!(out_nzcv & 0b0010 != 0, carry);
            assert_eq!(out_nzcv & 1, 1);
            assert_eq!(sp, 0x8000);
        }

        for &count in &[0_i64, 0x120, 0x1ff] {
            let immediate = lower_single_op(OpKind::ArmRegShift {
                dst: x(0),
                src: x(0),
                amount: SrcOperand::Imm(count),
                shift,
                width: OpWidth::W32,
                flags: nzc,
            });
            let (result, carry) = expected(0x8000_0001, count as u64, shift, true);
            let (out, out_nzcv, sp) = run_aarch64_code(&immediate, &[(0, 0x8000_0001)], 0b0011);
            assert_eq!(out[0], u64::from(result));
            assert_eq!(out_nzcv & 0b0010 != 0, carry);
            assert_eq!(out_nzcv & 1, 1);
            assert_eq!(sp, 0x8000);
        }

        let flagless = lower_single_op(OpKind::ArmRegShift {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Reg(x(1)),
            shift,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        let (result, _) = expected(0x8000_0001, 33, shift, false);
        for old_nzcv in 0_u8..16 {
            let (out, out_nzcv, sp) =
                run_aarch64_code(&flagless, &[(0, 0x8000_0001), (1, 33)], old_nzcv);
            assert_eq!(out[0], u64::from(result));
            assert_eq!(out_nzcv, old_nzcv);
            assert_eq!(sp, 0x8000);
        }

        // T32 independently encodes Rd, Rn (value), and Rm (count). Cover
        // every equality partition, both architectural flag modes, all
        // prior NZCV states, and the count boundaries that differ from
        // A64's modulo-32 variable shifts.
        for &(dst, src, amount_reg) in &[
            (0_u8, 1_u8, 2_u8),
            (0, 0, 2),
            (0, 1, 0),
            (0, 1, 1),
            (0, 0, 0),
        ] {
            for flags in [FlagUpdate::None, nzc] {
                let code = lower_single_op(OpKind::ArmRegShift {
                    dst: x(dst),
                    src: x(src),
                    amount: SrcOperand::Reg(x(amount_reg)),
                    shift,
                    width: OpWidth::W32,
                    flags,
                });
                for &(value, raw_count) in &[
                    (0x8000_0001_u64, 0_u64),
                    (0x8000_0001, 1),
                    (0x8000_0001, 31),
                    (0x8000_0001, 32),
                    (0x8000_0001, 33),
                    (0x8000_0001, 255),
                    (0x8000_0001, 256),
                    (0x7fff_ffff, 257),
                ] {
                    let mut input = [0xaaaa_aaaa_u64, 0xbbbb_bbbb, 0xcccc_cccc];
                    input[usize::from(src)] = value;
                    input[usize::from(amount_reg)] = raw_count;
                    let actual_value = input[usize::from(src)] as u32;
                    let actual_count = input[usize::from(amount_reg)];
                    for old_nzcv in 0_u8..16 {
                        let (result, carry) =
                            expected(actual_value, actual_count, shift, old_nzcv & 0b0010 != 0);
                        let regs = [(0, input[0]), (1, input[1]), (2, input[2])];
                        let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
                        assert_eq!(
                            out[usize::from(dst)],
                            u64::from(result),
                            "{shift:?} aliases=({dst},{src},{amount_reg}) count={actual_count:#x}"
                        );
                        let expected_nzcv = if flags == FlagUpdate::None {
                            old_nzcv
                        } else {
                            ((result >> 31) as u8) << 3
                                | ((result == 0) as u8) << 2
                                | (carry as u8) << 1
                                | (old_nzcv & 1)
                        };
                        assert_eq!(out_nzcv, expected_nzcv);
                        for reg in 0_u8..3 {
                            if reg != dst {
                                assert_eq!(out[usize::from(reg)], input[usize::from(reg)]);
                            }
                        }
                        assert_eq!(sp, 0x8000);
                    }
                }
            }
        }
    }
}
#[test]
fn lowers_all_a32_data_processing_register_shifts_with_exact_flags_and_aliasing() {
    use ArmDpRegShiftKind as Kind;

    fn shifted(value: u32, raw_count: u64, shift: ShiftOp, carry: bool) -> (u32, bool) {
        let count = (raw_count & 0xff) as u32;
        if count == 0 {
            return (value, carry);
        }
        match shift {
            ShiftOp::Lsl if count < 32 => (value << count, value >> (32 - count) & 1 != 0),
            ShiftOp::Lsl if count == 32 => (0, value & 1 != 0),
            ShiftOp::Lsl => (0, false),
            ShiftOp::Lsr if count < 32 => (value >> count, value >> (count - 1) & 1 != 0),
            ShiftOp::Lsr if count == 32 => (0, value >> 31 != 0),
            ShiftOp::Lsr => (0, false),
            ShiftOp::Asr if count < 32 => (
                ((value as i32) >> count) as u32,
                value >> (count - 1) & 1 != 0,
            ),
            ShiftOp::Asr => {
                let sign = value >> 31 != 0;
                (if sign { u32::MAX } else { 0 }, sign)
            }
            ShiftOp::Ror => {
                let result = value.rotate_right(count % 32);
                (result, result >> 31 != 0)
            }
            ShiftOp::Rrx => unreachable!(),
        }
    }

    fn add_carry(a: u32, b: u32, carry: bool) -> (u32, bool, bool) {
        let unsigned = u64::from(a) + u64::from(b) + u64::from(carry);
        let signed = i64::from(a as i32) + i64::from(b as i32) + i64::from(carry);
        (
            unsigned as u32,
            unsigned > u64::from(u32::MAX),
            signed < i64::from(i32::MIN) || signed > i64::from(i32::MAX),
        )
    }

    fn result(kind: Kind, lhs: u32, rhs: u32, carry: bool) -> (u32, Option<(bool, bool)>) {
        match kind {
            Kind::And | Kind::Tst => (lhs & rhs, None),
            Kind::Eor | Kind::Teq => (lhs ^ rhs, None),
            Kind::Sub | Kind::Cmp => {
                let (result, c, v) = add_carry(lhs, !rhs, true);
                (result, Some((c, v)))
            }
            Kind::Rsb => {
                let (result, c, v) = add_carry(rhs, !lhs, true);
                (result, Some((c, v)))
            }
            Kind::Add | Kind::Cmn => {
                let (result, c, v) = add_carry(lhs, rhs, false);
                (result, Some((c, v)))
            }
            Kind::Adc => {
                let (result, c, v) = add_carry(lhs, rhs, carry);
                (result, Some((c, v)))
            }
            Kind::Sbc => {
                let (result, c, v) = add_carry(lhs, !rhs, carry);
                (result, Some((c, v)))
            }
            Kind::Rsc => {
                let (result, c, v) = add_carry(rhs, !lhs, carry);
                (result, Some((c, v)))
            }
            Kind::Orr => (lhs | rhs, None),
            Kind::Mov => (rhs, None),
            Kind::Bic => (lhs & !rhs, None),
            Kind::Mvn => (!rhs, None),
        }
    }

    fn expected_nzcv(
        kind: Kind,
        result: u32,
        shifter_carry: bool,
        arithmetic: Option<(bool, bool)>,
        old: u8,
        set_flags: bool,
    ) -> u8 {
        if !set_flags {
            return old;
        }
        let nz = ((result >> 31) as u8) << 3 | (u8::from(result == 0) << 2);
        if let Some((carry, overflow)) = arithmetic {
            nz | (u8::from(carry) << 1) | u8::from(overflow)
        } else {
            debug_assert!(kind.is_logical());
            nz | (u8::from(shifter_carry) << 1) | (old & 1)
        }
    }

    fn op(kind: Kind, rd: u8, rn: u8, rm: u8, rs: u8, shift: ShiftOp, flags: FlagUpdate) -> OpKind {
        OpKind::ArmDpRegShift {
            kind,
            dst: kind.writes_result().then(|| x(rd)),
            rn: kind.uses_rn().then(|| x(rn)),
            rm: x(rm),
            rs: x(rs),
            shift,
            flags,
        }
    }

    let shifts = [ShiftOp::Lsl, ShiftOp::Lsr, ShiftOp::Asr, ShiftOp::Ror];
    let inputs = [
        (0_u64, 0x8000_0001_u64, 0_u64),
        (1, 0x8000_0001, 1),
        (0x7fff_ffff, 0x8000_0001, 31),
        (0x8000_0000, 0x7fff_ffff, 32),
        (0xffff_ffff, 0x8000_0000, 33),
        (0x4000_0001, 0xffff_ffff, 255),
        (0x8000_0001, 1, 256),
        (0x7fff_ffff, 0x8000_0001, 257),
    ];
    let scratch_sentinels = [
        (11_u8, 0x1111_1111_1111_1111_u64),
        (12, 0x1212_1212_1212_1212),
        (13, 0x1313_1313_1313_1313),
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];

    for opcode in 0_u8..16 {
        let kind = Kind::from_opcode(opcode).unwrap();
        let flag_update = FlagUpdate::Specific(if kind.is_logical() {
            FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF)
        } else {
            FlagSet::NZCV
        });
        for shift in shifts {
            let code = lower_single_op(op(kind, 0, 1, 2, 3, shift, flag_update));
            for &(lhs, rm, count) in &inputs {
                for old_nzcv in 0_u8..16 {
                    let carry_in = old_nzcv & 0b0010 != 0;
                    let (rhs, shifter_carry) = shifted(rm as u32, count, shift, carry_in);
                    let (expected, arithmetic) = result(kind, lhs as u32, rhs, carry_in);
                    let mut regs = vec![(0, 0xaaaa_aaaa_aaaa_aaaa), (1, lhs), (2, rm), (3, count)];
                    regs.extend_from_slice(&scratch_sentinels);
                    let (out, nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
                    if kind.writes_result() {
                        assert_eq!(out[0], u64::from(expected));
                    } else {
                        assert_eq!(out[0], 0xaaaa_aaaa_aaaa_aaaa);
                    }
                    assert_eq!(out[1], lhs);
                    assert_eq!(out[2], rm);
                    assert_eq!(out[3], count);
                    assert_eq!(
                        nzcv,
                        expected_nzcv(kind, expected, shifter_carry, arithmetic, old_nzcv, true,),
                        "{kind:?} {shift:?} lhs={lhs:#x} rm={rm:#x} count={count:#x} old={old_nzcv:#x}"
                    );
                    for &(reg, sentinel) in &scratch_sentinels {
                        assert_eq!(out[usize::from(reg)], sentinel, "x{reg}");
                    }
                    assert_eq!(sp, 0x8000);
                }
            }

            let flagless = lower_single_op(op(kind, 0, 1, 2, 3, shift, FlagUpdate::None));
            for &count in &[0_u64, 33] {
                for old_nzcv in 0_u8..16 {
                    let carry_in = old_nzcv & 0b0010 != 0;
                    let (rhs, shifter_carry) = shifted(0x8000_0001, count, shift, carry_in);
                    let (expected, arithmetic) = result(kind, 0x7fff_ffff, rhs, carry_in);
                    let (out, nzcv, sp) = run_aarch64_code(
                        &flagless,
                        &[
                            (0, 0xaaaa_aaaa_aaaa_aaaa),
                            (1, 0x7fff_ffff),
                            (2, 0x8000_0001),
                            (3, count),
                        ],
                        old_nzcv,
                    );
                    if kind.writes_result() {
                        assert_eq!(out[0], u64::from(expected));
                    }
                    assert_eq!(
                        nzcv,
                        expected_nzcv(kind, expected, shifter_carry, arithmetic, old_nzcv, false,)
                    );
                    assert_eq!(sp, 0x8000);
                }
            }
        }
    }

    // Every equality partition plus the highest admitted A32 registers.
    for &(rd, rn, rm, rs) in &[
        (0_u8, 1_u8, 2_u8, 3_u8),
        (0, 0, 2, 3),
        (0, 1, 0, 3),
        (0, 1, 2, 0),
        (0, 1, 1, 3),
        (0, 1, 2, 1),
        (0, 1, 2, 2),
        (0, 0, 0, 0),
        (14, 13, 12, 11),
    ] {
        for opcode in 0_u8..16 {
            let kind = Kind::from_opcode(opcode).unwrap();
            let flags = FlagUpdate::Specific(if kind.is_logical() {
                FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF)
            } else {
                FlagSet::NZCV
            });
            for shift in shifts {
                let code = lower_single_op(op(kind, rd, rn, rm, rs, shift, flags));
                let max_reg = usize::from(rd.max(rn).max(rm).max(rs));
                let mut initial = vec![0_u64; max_reg + 1];
                for (index, value) in initial.iter_mut().enumerate() {
                    *value = 0x1111_1111_u64.wrapping_mul(index as u64 + 1);
                }
                initial[usize::from(rm)] = 0x8000_0021;
                initial[usize::from(rs)] = 33;
                for old_nzcv in [0_u8, 0b0011] {
                    let carry_in = old_nzcv & 0b0010 != 0;
                    let (rhs, shifter_carry) = shifted(
                        initial[usize::from(rm)] as u32,
                        initial[usize::from(rs)],
                        shift,
                        carry_in,
                    );
                    let lhs = if kind.uses_rn() {
                        initial[usize::from(rn)] as u32
                    } else {
                        0
                    };
                    let (expected, arithmetic) = result(kind, lhs, rhs, carry_in);
                    let regs = initial
                        .iter()
                        .enumerate()
                        .map(|(reg, value)| (reg as u8, *value))
                        .collect::<Vec<_>>();
                    let (out, nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
                    if kind.writes_result() {
                        assert_eq!(out[usize::from(rd)], u64::from(expected));
                    }
                    for reg in 0..initial.len() {
                        if !kind.writes_result() || reg != usize::from(rd) {
                            assert_eq!(out[reg], initial[reg]);
                        }
                    }
                    assert_eq!(
                        nzcv,
                        expected_nzcv(kind, expected, shifter_carry, arithmetic, old_nzcv, true,),
                        "{kind:?} {shift:?} regs=({rd},{rn},{rm},{rs})"
                    );
                    assert_eq!(sp, 0x8000);
                }
            }
        }
    }
}
#[test]
fn rejects_invalid_aarch32_register_shift_contracts() {
    let nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));
    for kind in [
        OpKind::ArmRegShift {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Reg(x(1)),
            shift: ShiftOp::Lsl,
            width: OpWidth::W64,
            flags: nzc,
        },
        OpKind::ArmRegShift {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Reg(x(1)),
            shift: ShiftOp::Rrx,
            width: OpWidth::W32,
            flags: nzc,
        },
        OpKind::ArmRegShift {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Shifted {
                reg: x(1),
                shift: ShiftOp::Lsl,
                amount: 1,
            },
            shift: ShiftOp::Lsr,
            width: OpWidth::W32,
            flags: nzc,
        },
        OpKind::ArmRegShift {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Reg(x(1)),
            shift: ShiftOp::Asr,
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
    ] {
        assert!(try_lower_single_op(kind).is_err());
    }

    for kind in [
        OpKind::ArmDpRegShift {
            kind: ArmDpRegShiftKind::Add,
            dst: None,
            rn: Some(x(1)),
            rm: x(2),
            rs: x(3),
            shift: ShiftOp::Lsl,
            flags: FlagUpdate::Specific(FlagSet::NZCV),
        },
        OpKind::ArmDpRegShift {
            kind: ArmDpRegShiftKind::Tst,
            dst: Some(x(0)),
            rn: Some(x(1)),
            rm: x(2),
            rs: x(3),
            shift: ShiftOp::Lsr,
            flags: nzc,
        },
        OpKind::ArmDpRegShift {
            kind: ArmDpRegShiftKind::Mov,
            dst: Some(x(0)),
            rn: Some(x(1)),
            rm: x(2),
            rs: x(3),
            shift: ShiftOp::Ror,
            flags: nzc,
        },
        OpKind::ArmDpRegShift {
            kind: ArmDpRegShiftKind::And,
            dst: Some(x(0)),
            rn: Some(x(1)),
            rm: x(2),
            rs: x(3),
            shift: ShiftOp::Asr,
            flags: FlagUpdate::Specific(FlagSet::NZCV),
        },
        OpKind::ArmDpRegShift {
            kind: ArmDpRegShiftKind::Cmp,
            dst: None,
            rn: Some(x(1)),
            rm: x(2),
            rs: x(3),
            shift: ShiftOp::Rrx,
            flags: FlagUpdate::Specific(FlagSet::NZCV),
        },
    ] {
        assert!(try_lower_single_op(kind).is_err());
    }
}
#[test]
fn lowers_addsub_ror_sources() {
    let cases = [
        (
            OpKind::Add {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 13,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            enc_extract(1, 2, 2, 13),
            enc_addsub_shift_regs(1, 0, 0, 0, 0, 0, 1, 0),
        ),
        (
            OpKind::Sub {
                dst: x(0),
                src1: x(1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 7,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            enc_extract(0, 2, 2, 7),
            enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 1, 0),
        ),
    ];

    for (kind, rotate, addsub) in cases {
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
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
}
#[test]
fn lowers_addsub_effective_zero_ror_sources_as_register_sources() {
    let cases = [
        (
            OpKind::Add {
                dst: x(1),
                src1: x(1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 64,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            enc_addsub_shift_regs(1, 0, 0, 0, 0, 1, 1, 2),
        ),
        (
            OpKind::Sub {
                dst: VReg::virt(1),
                src1: x(1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 32,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            enc_addsub_shift_regs(0, 1, 1, 0, 0, 31, 1, 2),
        ),
    ];

    for (kind, addsub) in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(0, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&addsub.to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
}
#[test]
fn rejects_addsub_ror_source_when_dst_is_base() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Shifted {
                reg: x(2),
                shift: ShiftOp::Ror,
                amount: 13,
            },
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
fn lowers_cmp_w16_zero_base_zero_shifted_source_as_constant_flags() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cmp {
            src1: VReg::Imm(0),
            src2: SrcOperand::Shifted {
                reg: VReg::Imm(0),
                shift: ShiftOp::Ror,
                amount: 7,
            },
            width: OpWidth::W16,
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
fn lowers_muladd_x_imm_power_of_two_as_add_shifted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::MulAdd {
            dst: x(0),
            acc: x(3),
            src1: VReg::Imm(8),
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
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 0, 3, 0, 3, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_mulsub_w16_imm_masked_power_of_two_as_sub_shifted_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::MulSub {
            dst: x(0),
            acc: x(3),
            src1: x(1),
            src2: VReg::Imm(0x1_0004),
            width: OpWidth::W16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 2, 0, 3, 1).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shl_w8_imm_as_ubfiz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
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
    expected.extend_from_slice(&enc_bitfield(0, 0b10, 29, 4).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shr_w16_imm_as_ubfx() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
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
    expected.extend_from_slice(&enc_bitfield(0, 0b10, 5, 15).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_sar_w8_imm_as_sbfm_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
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
    expected.extend_from_slice(&enc_bitfield(0, 0b00, 3, 7).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shl_w16_imm_count_above_width_as_zero() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(17),
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
fn lowers_shift_x_zero_same_reg_as_noop() {
    let cases = [
        OpKind::Shl {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(0),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Shr {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm64(64),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Sar {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(0),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Ror {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm64(64),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Rol {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(64),
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
fn lowers_shift_w_zero_same_reg_as_self_mov_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(0),
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
fn lowers_carry_rotate_zero_effective_count_as_identity() {
    let cases = [
        (
            OpKind::Rcl {
                dst: x(0),
                src: x(0),
                amount: SrcOperand::Imm64(64),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            vec![enc_mov_reg(1, 0, 0), 0xd65f_03c0],
        ),
        (
            OpKind::Rcr {
                dst: x(0),
                src: x(0),
                amount: SrcOperand::Imm(32),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0],
        ),
        (
            OpKind::Rcl {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Imm(9),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
            vec![enc_bitfield_regs(0, 0b10, 0, 7, 1, 0), 0xd65f_03c0],
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
fn lowers_shift_zero_count_reg_as_immediate_zero() {
    let cases = [
        (
            OpKind::Shl {
                dst: x(0),
                src: x(0),
                amount: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            vec![0xd65f_03c0],
        ),
        (
            OpKind::Shr {
                dst: x(0),
                src: x(0),
                amount: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0],
        ),
        (
            OpKind::Sar {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            vec![enc_mov_reg(1, 0, 1), 0xd65f_03c0],
        ),
        (
            OpKind::Ror {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            vec![enc_bitfield_regs(0, 0b10, 0, 7, 1, 0), 0xd65f_03c0],
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
fn lowers_rol_zero_count_reg_as_immediate_zero() {
    let cases = [
        (
            OpKind::Rol {
                dst: x(0),
                src: x(0),
                amount: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            vec![0xd65f_03c0],
        ),
        (
            OpKind::Rol {
                dst: x(0),
                src: x(0),
                amount: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            vec![enc_mov_reg(0, 0, 0), 0xd65f_03c0],
        ),
        (
            OpKind::Rol {
                dst: x(0),
                src: x(1),
                amount: SrcOperand::Reg(VReg::Imm(0)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            vec![enc_bitfield_regs(0, 0b10, 0, 7, 1, 0), 0xd65f_03c0],
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
fn lowers_zero_imm_source_reg_count_shifts_as_movz() {
    let cases = [
        (
            OpKind::Shl {
                dst: x(0),
                src: VReg::Imm(0),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b10, 0, 0, 0),
        ),
        (
            OpKind::Shr {
                dst: x(1),
                src: VReg::Imm(0),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b10, 0, 0, 1),
        ),
        (
            OpKind::Sar {
                dst: x(2),
                src: VReg::Imm(0),
                amount: SrcOperand::Reg(x(3)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b10, 0, 0, 2),
        ),
        (
            OpKind::Ror {
                dst: x(3),
                src: VReg::Imm(0),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(1, 0b10, 0, 0, 3),
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
fn lowers_all_ones_imm_source_reg_count_asr_ror_as_constant() {
    let cases = [
        (
            OpKind::Sar {
                dst: x(0),
                src: VReg::Imm(-1),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b00, 0, 0, 0),
        ),
        (
            OpKind::Sar {
                dst: x(1),
                src: VReg::Imm(0xffff),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b10, 0, 0xffff, 1),
        ),
        (
            OpKind::Ror {
                dst: x(2),
                src: VReg::Imm(-1),
                amount: SrcOperand::Reg(x(3)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(1, 0b00, 0, 0, 2),
        ),
        (
            OpKind::Ror {
                dst: x(3),
                src: VReg::Imm(0xff),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b10, 0, 0xff, 3),
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
fn lowers_shl_x_imm_src_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
            dst: x(0),
            src: VReg::Imm(0x123),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x1230, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shl_w_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
            dst: x(0),
            src: VReg::Imm(-1),
            amount: SrcOperand::Imm(4),
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
    expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0xf, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shl_x_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
            dst: x(0),
            src: VReg::Imm(-1),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0xf, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shl_w8_imm_src_as_movz_masked() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
            dst: x(0),
            src: VReg::Imm(0x1f),
            amount: SrcOperand::Imm(4),
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
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xf0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shr_x_imm_src_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: x(0),
            src: VReg::Imm(0x1230),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x123, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shr_w_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: x(0),
            src: VReg::Imm(-1),
            amount: SrcOperand::Imm(0),
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
fn lowers_shr_x_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: x(0),
            src: VReg::Imm(-1),
            amount: SrcOperand::Imm(0),
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
fn lowers_shr_w8_imm_src_as_movz_masked() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: x(0),
            src: VReg::Imm(0x1f0),
            amount: SrcOperand::Imm(4),
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
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xf, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_sar_x_imm_src_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
            dst: x(0),
            src: VReg::Imm(0x1230),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x123, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_sar_w_imm_src_negative_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
            dst: x(0),
            src: VReg::Imm(-16),
            amount: SrcOperand::Imm(4),
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
fn lowers_sar_x_imm_src_negative_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
            dst: x(0),
            src: VReg::Imm(-16),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_sar_w8_imm_src_negative_as_movz_masked() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
            dst: x(0),
            src: VReg::Imm(0xf0),
            amount: SrcOperand::Imm(4),
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
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xff, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_ror_x_imm_src_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
            dst: x(0),
            src: VReg::Imm(0x1230),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x123, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_ror_w8_imm_src_wrap_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
            dst: x(0),
            src: VReg::Imm(0x81),
            amount: SrcOperand::Imm(1),
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
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xc0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_ror_w_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
            dst: x(0),
            src: VReg::Imm(-1),
            amount: SrcOperand::Imm(4),
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
fn lowers_ror_x_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
            dst: x(0),
            src: VReg::Imm(-1),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_zero_or_all_ones_imm_source_reg_count_as_constant() {
    let cases = [
        (
            OpKind::Rol {
                dst: x(0),
                src: VReg::Imm(0),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(1, 0b10, 0, 0, 0),
        ),
        (
            OpKind::Rol {
                dst: x(1),
                src: VReg::Imm(0xff),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b10, 0, 0xff, 1),
        ),
        (
            OpKind::Rol {
                dst: x(2),
                src: VReg::Imm(-1),
                amount: SrcOperand::Reg(x(3)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b00, 0, 0, 2),
        ),
        (
            OpKind::Rol {
                dst: x(3),
                src: VReg::Imm(0xffff),
                amount: SrcOperand::Reg(x(2)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            enc_mov_wide(0, 0b10, 0, 0xffff, 3),
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
fn lowers_ror_w8_imm_as_duplicate_extract() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 1, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 24, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 3, 10, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_x_imm_src_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: VReg::Imm(0x123),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x1230, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_w_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: VReg::Imm(-1),
            amount: SrcOperand::Imm(4),
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
fn lowers_rol_x_imm_src_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: VReg::Imm(-1),
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
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_w8_imm_src_wrap_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: VReg::Imm(0x81),
            amount: SrcOperand::Imm(1),
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
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x3, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_ror_w8_imm_masked_zero_as_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(8),
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
fn lowers_shl_w8_reg_with_count_guards() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_test_branch(2, 3, true, 24).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 4, true, 20).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 5, true, 16).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1000, 1, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_b(2).to_le_bytes());
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shr_w16_reg_with_count_guards_and_source_mask() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_test_branch(2, 4, true, 20).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 5, true, 16).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 0).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1001, 0, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_b(2).to_le_bytes());
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_sar_w8_reg_with_count_guards_and_sign_fill() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_test_branch(2, 3, true, 28).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 4, true, 24).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 5, true, 20).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 8, 7, 1, 0).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1010, 0, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 24, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_b(3).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b00, 7, 7, 1, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_ror_w16_reg_in_place_as_duplicate_rorv_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
            dst: x(1),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 1).to_le_bytes());
    expected.extend_from_slice(&enc_logical_shifted(0, 0b01, 0, false, 1, 1, 1, 16).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1011, 1, 2, 1).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 1, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_ror_w16_reg_self_count_in_place() {
    assert_shift_reg_count_alias_lowering(
        "ror_w16_dst_aliases_src_and_count",
        ShiftOp::Ror,
        1,
        0x8001,
        1,
        0x8001,
        OpWidth::W16,
        1,
    );
}
#[test]
fn lowers_ror_w8_reg_as_repeated_byte_rorv_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ror {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 24, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 16, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 8, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1011, 0, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_bidir_shift_immediate_encoding() {
    let words = code_words(&lower_single_op(OpKind::BidirShift {
        dst: x(0),
        src: SrcOperand::Reg(x(1)),
        amount: SrcOperand::Imm(3),
        kind: 2,
        width: OpWidth::W64,
    }));
    assert_eq!(
        words,
        vec![enc_bitfield_regs(1, 0b10, 61, 60, 1, 0), 0xd65f_03c0]
    );
}
#[test]
fn lowers_bidir_shift_runtime() {
    assert_bidir_shift_runtime(
        "w64_positive_logical_left",
        SrcOperand::Reg(x(1)),
        SrcOperand::Reg(x(2)),
        0x0123_4567_89ab_cdef,
        0xffff_0000_0000_0004,
        2,
        OpWidth::W64,
    );
    assert_bidir_shift_runtime(
        "w64_negative_arithmetic_right",
        SrcOperand::Reg(x(1)),
        SrcOperand::Reg(x(2)),
        0x8000_0000_0000_0000,
        0x5a5a_5a5a_5a5a_5a7d,
        0,
        OpWidth::W64,
    );
    assert_bidir_shift_runtime(
        "w64_negative_full_arithmetic_right",
        SrcOperand::Reg(x(1)),
        SrcOperand::Reg(x(2)),
        0x8000_0000_0000_0001,
        0x40,
        0,
        OpWidth::W64,
    );
    assert_bidir_shift_runtime(
        "w64_negative_full_logical_right",
        SrcOperand::Reg(x(1)),
        SrcOperand::Reg(x(2)),
        0xffff_ffff_ffff_ffff,
        0x40,
        2,
        OpWidth::W64,
    );
    assert_bidir_shift_runtime(
        "w32_positive_count_above_width",
        SrcOperand::Reg(x(1)),
        SrcOperand::Reg(x(2)),
        0x8000_0001,
        36,
        3,
        OpWidth::W32,
    );
    assert_bidir_shift_runtime(
        "w32_immediate_source",
        SrcOperand::Imm(0x1234),
        SrcOperand::Reg(x(2)),
        0x1234,
        4,
        2,
        OpWidth::W32,
    );
}
#[test]
fn rejects_bidir_shift_unsupported_width() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::BidirShift {
            dst: x(0),
            src: SrcOperand::Reg(x(1)),
            amount: SrcOperand::Reg(x(2)),
            kind: 0,
            width: OpWidth::W16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();
    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn fuses_signed_load_w_shifted_reg_offset_sequence() {
    let ext = VReg::virt(0);
    let shifted = VReg::virt(1);
    let addr = VReg::virt(2);
    let load_tmp = VReg::virt(3);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::ZeroExtend {
            dst: ext,
            src: x(2),
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    );
    builder.push_op(
        0,
        OpKind::Shl {
            dst: shifted,
            src: ext,
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: addr,
            src1: x(1),
            src2: SrcOperand::Reg(shifted),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Load {
            dst: load_tmp,
            addr: Address::Direct(addr),
            width: MemWidth::B2,
            sign: SignExtend::Sign,
        },
    );
    builder.push_op(
        0,
        OpKind::ZeroExtend {
            dst: x(0),
            src: load_tmp,
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_reg(1, 0b11, 2, 0b010, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_lifted_extract_sequence_with_masked_shift_counts() {
    let lo = VReg::virt(0);
    let hi = VReg::virt(1);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: lo,
            src: x(2),
            amount: SrcOperand::Imm64(77),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Shl {
            dst: hi,
            src: x(1),
            amount: SrcOperand::Imm(115),
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
fn fuses_lifted_ror_w_alias_sequence() {
    let lo = VReg::virt(0);
    let hi = VReg::virt(1);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: lo,
            src: x(1),
            amount: SrcOperand::Imm(7),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Shl {
            dst: hi,
            src: x(1),
            amount: SrcOperand::Imm(25),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Or {
            dst: x(0),
            src1: lo,
            src2: SrcOperand::Reg(hi),
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
    expected.extend_from_slice(&enc_extract(0, 1, 1, 7).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_x_imm_as_ror() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(13),
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
    expected.extend_from_slice(&enc_extract(1, 1, 1, 51).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_w_imm_as_ror_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(7),
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
    expected.extend_from_slice(&enc_extract(0, 1, 1, 25).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shl_w_reg_with_x86_count_guard() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shl {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1000, 1, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 5, false, 8).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(0, 0, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shr_w_reg_with_x86_count_guard() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shr {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1001, 1, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 5, false, 8).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(0, 0, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_and_w_shifted_byte_mask_imm() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::And {
            dst: x(0),
            src1: x(1),
            src2: SrcOperand::Imm(0xff00),
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
    expected.extend_from_slice(&enc_logical_imm(0, 0b00, 0, 24, 7, 0, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_and_all_ones_left_imm_shifted_as_orr_or_adds() {
    let cases = [
        (
            OpKind::And {
                dst: x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsl,
                    amount: 4,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            enc_logical_shift_regs(1, 0b01, 0, 0, 4, 0, 31, 2),
        ),
        (
            OpKind::And {
                dst: x(0),
                src1: VReg::Imm(0x1_ffff_ffff),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 13,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            enc_logical_shift_regs(0, 0b01, 0, 3, 13, 0, 31, 2),
        ),
        (
            OpKind::And {
                dst: x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsr,
                    amount: 8,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            enc_addsub_shift_regs(1, 0, 1, 1, 8, 0, 31, 2),
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
fn lowers_andnot_all_ones_left_imm_shifted_as_mvn_or_flags() {
    let cases = [
        (
            OpKind::AndNot {
                dst: x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsl,
                    amount: 4,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            vec![
                enc_logical_shift_regs(1, 0b01, 1, 0, 4, 0, 31, 2),
                0xd65f_03c0u32,
            ],
        ),
        (
            OpKind::AndNot {
                dst: x(0),
                src1: VReg::Imm(0x1_ffff_ffff),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 13,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            vec![
                enc_logical_shift_regs(0, 0b01, 1, 3, 13, 0, 31, 2),
                0xd65f_03c0u32,
            ],
        ),
        (
            OpKind::AndNot {
                dst: x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsr,
                    amount: 8,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            vec![
                enc_logical_shift_regs(1, 0b01, 1, 1, 8, 0, 31, 2),
                enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                0xd65f_03c0u32,
            ],
        ),
        (
            OpKind::AndNot {
                dst: x(0),
                src1: VReg::Imm(0x1_ffff_ffff),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Asr,
                    amount: 31,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            vec![
                enc_logical_shift_regs(0, 0b01, 1, 2, 31, 0, 31, 2),
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
fn lowers_orr_all_ones_left_imm_shifted_as_movn_or_flags() {
    let cases = [
        (
            OpKind::Or {
                dst: x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsl,
                    amount: 4,
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
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 13,
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
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsr,
                    amount: 8,
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
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Asr,
                    amount: 31,
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
fn lowers_xor_all_ones_left_imm_shifted_as_eon_or_flags() {
    let cases = [
        (
            OpKind::Xor {
                dst: x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsl,
                    amount: 4,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            vec![
                enc_logical_shift_regs(1, 0b10, 1, 0, 4, 0, 31, 2),
                0xd65f_03c0u32,
            ],
        ),
        (
            OpKind::Xor {
                dst: x(0),
                src1: VReg::Imm(0x1_ffff_ffff),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Ror,
                    amount: 13,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            vec![
                enc_logical_shift_regs(0, 0b10, 1, 3, 13, 0, 31, 2),
                0xd65f_03c0u32,
            ],
        ),
        (
            OpKind::Xor {
                dst: x(0),
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsr,
                    amount: 8,
                },
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            vec![
                enc_logical_shift_regs(1, 0b10, 1, 1, 8, 0, 31, 2),
                enc_logical_reg_n(1, 0b11, 0, 31, 0, 0),
                0xd65f_03c0u32,
            ],
        ),
        (
            OpKind::Xor {
                dst: x(0),
                src1: VReg::Imm(0x1_ffff_ffff),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Asr,
                    amount: 31,
                },
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            vec![
                enc_logical_shift_regs(0, 0b10, 1, 2, 31, 0, 31, 2),
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
fn lowers_test_all_ones_left_imm_shifted_as_adds_zero_base() {
    let cases = [
        (
            OpKind::Test {
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsl,
                    amount: 4,
                },
                width: OpWidth::W64,
            },
            enc_addsub_shift_regs(1, 0, 1, 0, 4, 31, 31, 2),
        ),
        (
            OpKind::Test {
                src1: VReg::Imm(-1),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Lsr,
                    amount: 8,
                },
                width: OpWidth::W64,
            },
            enc_addsub_shift_regs(1, 0, 1, 1, 8, 31, 31, 2),
        ),
        (
            OpKind::Test {
                src1: VReg::Imm(0x1_ffff_ffff),
                src2: SrcOperand::Shifted {
                    reg: x(2),
                    shift: ShiftOp::Asr,
                    amount: 31,
                },
                width: OpWidth::W32,
            },
            enc_addsub_shift_regs(0, 0, 1, 2, 31, 31, 31, 2),
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
fn lowers_ands_w8_reg_with_flags_as_and_uxtb_shifted_ands() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::And {
            dst: x(0),
            src1: x(1),
            src2: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_logical_reg_n(0, 0b00, 0, 0, 1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 8, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 24, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_orrs_w8_reg_with_flags_as_orr_uxtb_shifted_ands() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Or {
            dst: x(0),
            src1: x(1),
            src2: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_logical_reg_n(0, 0b01, 0, 0, 1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 8, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 24, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_eors_w16_imm_with_flags_as_eor_uxth_shifted_ands() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Xor {
            dst: x(0),
            src1: x(1),
            src2: SrcOperand::Imm(0xff),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 16, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_reg_n(0, 0b11, 0, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 16, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_axflag_with_masked_shift_counts() {
    let code = lower_ops_with_flagm_features(axflag_ops(), true, true);

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_flagm(0b010).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_xaflag_with_masked_shift_counts() {
    let code = lower_ops_with_flagm_features(xaflag_ops(), true, true);

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_flagm(0b001).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_x_imm_as_extract() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(13),
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
    expected.extend_from_slice(&enc_extract(1, 1, 0, 13).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_x_all_ones_imm_src_as_shift_orr() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: VReg::Imm(-1),
            amount: SrcOperand::Imm(13),
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
    expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 13, 63, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 13, 12, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_x_imm_src_zero_insert_as_lsr() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: VReg::Imm(0x1200),
            amount: SrcOperand::Imm(8),
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
    expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 8, 63, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_x_encodable_imm_src_as_shift_orr() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: VReg::Imm(0x1f),
            amount: SrcOperand::Imm(13),
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
    expected.extend_from_slice(&enc_bitfield_regs(1, 0b10, 13, 63, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_imm(1, 0b01, 1, 13, 4, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_x_imm_as_extract() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(13),
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
    expected.extend_from_slice(&enc_extract(1, 0, 1, 51).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w_all_ones_imm_src_as_shift_orr() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: VReg::Imm(-1),
            amount: SrcOperand::Imm(7),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 25, 24, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 0, 6, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w_imm_src_zero_insert_as_lsl() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: VReg::Imm(0x00ff),
            amount: SrcOperand::Imm(7),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 25, 24, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w_encodable_imm_src_as_shift_orr() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: VReg::Imm(0xe000_0000),
            amount: SrcOperand::Imm(7),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 25, 24, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 28, 2, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_w_imm_as_extract_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(7),
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
    expected.extend_from_slice(&enc_extract(0, 1, 0, 7).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w_imm_as_extract_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(7),
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
    expected.extend_from_slice(&enc_extract(0, 0, 1, 25).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_w_masked_zero_count_as_self_mov() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(32),
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
fn fuses_lifted_ubfiz_sequence_with_masked_shift_count() {
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
            amount: SrcOperand::Imm64(68),
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
fn lowers_flag_setting_shift_runtime() {
    assert_shift_flags_lowering(
        "shl_x_imm1_flags",
        ShiftOp::Lsl,
        1,
        0x8000_0000_0000_0001,
        None,
        1,
        OpWidth::W64,
        0,
        0b1010,
    );
    assert_shift_flags_lowering(
        "shr_w_imm1_flags",
        ShiftOp::Lsr,
        1,
        0x8000_0000,
        None,
        1,
        OpWidth::W32,
        0,
        0b0110,
    );
    assert_shift_flags_lowering(
        "sar_w8_imm63_sign_carry",
        ShiftOp::Asr,
        1,
        0x80,
        None,
        63,
        OpWidth::W8,
        0,
        0b0101,
    );
    assert_shift_flags_lowering(
        "shl_w_count32_aliases_count",
        ShiftOp::Lsl,
        1,
        0x8000_0001,
        Some(2),
        32,
        OpWidth::W32,
        2,
        0b1001,
    );
    assert_shift_flags_lowering(
        "shr_w_count0_preserves_flags",
        ShiftOp::Lsr,
        1,
        0x1234_5678,
        Some(2),
        0,
        OpWidth::W32,
        0,
        0b1011,
    );
    assert_shift_flags_lowering(
        "shr_w8_reg9_zero_carry",
        ShiftOp::Lsr,
        1,
        0xff,
        Some(2),
        9,
        OpWidth::W8,
        0,
        0b0011,
    );
    assert_shift_flags_lowering(
        "sar_w16_reg20_sign_carry",
        ShiftOp::Asr,
        1,
        0x8001,
        Some(2),
        20,
        OpWidth::W16,
        0,
        0b0101,
    );
}
#[test]
fn lowers_sar_w_reg_with_sign_guard() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1010, 1, 2, 0).to_le_bytes());
    expected.extend_from_slice(&enc_test_branch(2, 5, false, 8).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b00, 31, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_w32_shift_reg_when_dst_is_count() {
    assert_shift_reg_count_alias_lowering(
        "shl_w_dst_aliases_count",
        ShiftOp::Lsl,
        1,
        0x0000_0003,
        2,
        4,
        OpWidth::W32,
        2,
    );
    assert_shift_reg_count_alias_lowering(
        "shl_w_dst_aliases_src_and_count",
        ShiftOp::Lsl,
        1,
        3,
        1,
        3,
        OpWidth::W32,
        1,
    );
    assert_shift_reg_count_alias_lowering(
        "shr_w_dst_aliases_count_oob_zero",
        ShiftOp::Lsr,
        1,
        0x8000_0000,
        2,
        32,
        OpWidth::W32,
        2,
    );
    assert_shift_reg_count_alias_lowering(
        "sar_w_dst_aliases_count",
        ShiftOp::Asr,
        1,
        0xffff_fff0,
        2,
        4,
        OpWidth::W32,
        2,
    );
    assert_shift_reg_count_alias_lowering(
        "sar_w_dst_aliases_count_oob_sign",
        ShiftOp::Asr,
        1,
        0x8000_0000,
        2,
        32,
        OpWidth::W32,
        2,
    );
}
#[test]
fn lowers_flag_setting_rotate_runtime() {
    assert_rotate_flags_lowering(
        "rol_x_imm1_flags",
        false,
        1,
        0x8000_0000_0000_0001,
        None,
        1,
        OpWidth::W64,
        0,
        0b1100,
    );
    assert_rotate_flags_lowering(
        "ror_w_imm1_flags",
        true,
        1,
        0x3,
        None,
        1,
        OpWidth::W32,
        0,
        0b0100,
    );
    assert_rotate_flags_lowering(
        "rol_w16_imm16_updates_carry_and_clears_overflow",
        false,
        1,
        0x8001,
        None,
        16,
        OpWidth::W16,
        0,
        0b1001,
    );
    assert_rotate_flags_lowering(
        "ror_w8_reg32_preserves_flags",
        true,
        1,
        0x81,
        Some(2),
        32,
        OpWidth::W8,
        0,
        0b1011,
    );
    assert_rotate_flags_lowering(
        "rol_w8_reg9_aliases_count",
        false,
        1,
        0x81,
        Some(2),
        9,
        OpWidth::W8,
        2,
        0b0100,
    );
    assert_rotate_flags_lowering(
        "ror_x_reg4_flags",
        true,
        1,
        0x10,
        Some(2),
        4,
        OpWidth::W64,
        0,
        0b1001,
    );
}
#[test]
fn lowers_rol_x_reg_as_neg_count_rorv() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 0, 31, 2).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(1, 0b1011, 1, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_w_reg_as_neg_count_rorv_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_addsub_shift_regs(0, 1, 0, 0, 0, 0, 31, 2).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(0, 0b1011, 1, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_x_reg_when_dst_is_count() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rol {
            dst: x(2),
            src: x(1),
            amount: SrcOperand::Reg(x(2)),
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
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 1, 0, 0, 0, 2, 31, 2).to_le_bytes());
    expected.extend_from_slice(&enc_dp2_regs(1, 0b1011, 1, 2, 2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rol_reg_when_dst_aliases_src() {
    assert_rol_reg_lowering(
        "rol_x_dst_aliases_src",
        0,
        0x8000_0000_0000_0001,
        2,
        1,
        OpWidth::W64,
        0,
    );
    assert_rol_reg_lowering(
        "rol_w_dst_aliases_src",
        0,
        0x8000_0001,
        2,
        4,
        OpWidth::W32,
        0,
    );
    assert_rol_reg_lowering(
        "rol_x_dst_aliases_src_and_count",
        1,
        3,
        1,
        3,
        OpWidth::W64,
        1,
    );
}
#[test]
fn lowers_shld_w16_imm_as_shift_bfxil_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 27, 26, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 11, 15, 1, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w16_imm_src_zero_insert_as_lsl_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: VReg::Imm(0x07ff),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 27, 26, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w16_encodable_imm_src_as_shift_orr_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: VReg::Imm(0xf800),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 27, 26, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 0, 4, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w16_masked_zero_count_as_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(32),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w8_imm_as_shift_bfxil_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 29, 28, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b01, 5, 7, 1, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w8_masked_zero_count_as_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(32),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_w8_masked_zero_count_as_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(32),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_w8_imm_src_zero_insert_as_lsr_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: VReg::Imm(0x18),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 3, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_w8_encodable_imm_src_as_shift_orr_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: VReg::Imm(0x07),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 3, 31, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_logical_imm(0, 0b01, 0, 27, 2, 0, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_w16_masked_zero_count_as_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(32),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w16_full_count_alias_as_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(16),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w16_full_count_imm_src_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: VReg::Imm(0x1_0000_1234),
            amount: SrcOperand::Imm(16),
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
fn lowers_shrd_w16_full_count_alias_as_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(16),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shld_w8_full_count_alias_as_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shld {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(8),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_shrd_w8_full_count_alias_as_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Shrd {
            dst: x(0),
            src: x(0),
            amount: SrcOperand::Imm(8),
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
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
