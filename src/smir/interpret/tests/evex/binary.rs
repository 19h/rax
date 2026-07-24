//! Exact x86 SIMD binary floating-point execution tests.

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn x86_fp_binary_cores_cover_rounding_special_values_daz_and_minmax_selection() {
    let half_ulp_f32 = u64::from((2.0f32).powi(-24).to_bits());
    for (mode, expected) in [
        (FpRoundMode::RoundNearest, 0x3F80_0000),
        (FpRoundMode::RoundDown, 0x3F80_0000),
        (FpRoundMode::RoundUp, 0x3F80_0001),
        (FpRoundMode::RoundTowardZero, 0x3F80_0000),
    ] {
        let result = SmirInterpreter::x86_simd_fp_add(
            u64::from(1.0f32.to_bits()),
            half_ulp_f32,
            X86_SIMD_F32,
            mode,
            0x1F80,
        );
        assert_eq!(result.bits, expected, "add {mode:?}");
        assert_eq!(result.status, 1 << 5, "add precision {mode:?}");
    }

    for (mode, expected_f32, expected_f64) in [
        (
            FpRoundMode::RoundNearest,
            0x3EAA_AAAB,
            0x3FD5_5555_5555_5555,
        ),
        (FpRoundMode::RoundDown, 0x3EAA_AAAA, 0x3FD5_5555_5555_5555),
        (FpRoundMode::RoundUp, 0x3EAA_AAAB, 0x3FD5_5555_5555_5556),
        (
            FpRoundMode::RoundTowardZero,
            0x3EAA_AAAA,
            0x3FD5_5555_5555_5555,
        ),
    ] {
        let f32_result = SmirInterpreter::x86_simd_fp_div(
            u64::from(1.0f32.to_bits()),
            u64::from(3.0f32.to_bits()),
            X86_SIMD_F32,
            mode,
            0x1F80,
        );
        assert_eq!(f32_result.bits, expected_f32, "binary32 div {mode:?}");
        assert_eq!(f32_result.status, 1 << 5);
        let f64_result = SmirInterpreter::x86_simd_fp_div(
            1.0f64.to_bits(),
            3.0f64.to_bits(),
            X86_SIMD_F64,
            mode,
            0x1F80,
        );
        assert_eq!(f64_result.bits, expected_f64, "binary64 div {mode:?}");
        assert_eq!(f64_result.status, 1 << 5);
    }

    for (first, second, expected, status) in [
        (
            1.0f32.to_bits(),
            0.0f32.to_bits(),
            f32::INFINITY.to_bits(),
            1 << 2,
        ),
        (0.0f32.to_bits(), 0.0f32.to_bits(), 0xFFC0_0000, 1),
        (
            f32::INFINITY.to_bits(),
            f32::INFINITY.to_bits(),
            0xFFC0_0000,
            1,
        ),
        (
            (-1.0f32).to_bits(),
            0.0f32.to_bits(),
            f32::NEG_INFINITY.to_bits(),
            1 << 2,
        ),
        (
            (-0.0f32).to_bits(),
            1.0f32.to_bits(),
            (-0.0f32).to_bits(),
            0,
        ),
    ] {
        let result = SmirInterpreter::x86_simd_fp_div(
            u64::from(first),
            u64::from(second),
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
        );
        assert_eq!(result.bits, u64::from(expected));
        assert_eq!(result.status, status);
    }

    let denormal = SmirInterpreter::x86_simd_fp_div(
        1,
        u64::from(1.0f32.to_bits()),
        X86_SIMD_F32,
        FpRoundMode::RoundNearest,
        0x1F80,
    );
    assert_eq!(denormal.bits, 1);
    assert_eq!(denormal.status, 1 << 1);
    let daz = SmirInterpreter::x86_simd_fp_div(
        1,
        u64::from(1.0f32.to_bits()),
        X86_SIMD_F32,
        FpRoundMode::RoundNearest,
        0x1F80 | (1 << 6),
    );
    assert_eq!(daz.bits, 0);
    assert_eq!(daz.status, 0);

    let qnan = 0x7FC1_2345;
    let sub_nan_denormal =
        SmirInterpreter::x86_simd_fp_sub(qnan, 1, X86_SIMD_F32, FpRoundMode::RoundNearest, 0x1F80);
    assert_eq!(sub_nan_denormal.bits, qnan);
    assert_eq!(sub_nan_denormal.status, 1 << 1);
    let sub_nan_daz = SmirInterpreter::x86_simd_fp_sub(
        qnan,
        1,
        X86_SIMD_F32,
        FpRoundMode::RoundNearest,
        0x1F80 | (1 << 6),
    );
    assert_eq!(sub_nan_daz.bits, qnan);
    assert_eq!(sub_nan_daz.status, 0);

    for (mode, expected) in [
        (FpRoundMode::RoundNearest, 0x7F80_0000),
        (FpRoundMode::RoundDown, 0x7F7F_FFFF),
        (FpRoundMode::RoundUp, 0x7F80_0000),
        (FpRoundMode::RoundTowardZero, 0x7F7F_FFFF),
    ] {
        let overflow = SmirInterpreter::x86_simd_fp_div(
            0x7F7F_FFFF,
            u64::from(0.5f32.to_bits()),
            X86_SIMD_F32,
            mode,
            0x1F80,
        );
        assert_eq!(overflow.bits, expected, "overflow {mode:?}");
        assert_eq!(overflow.status, (1 << 3) | (1 << 5));
    }

    for (mode, expected) in [
        (FpRoundMode::RoundNearest, 0),
        (FpRoundMode::RoundDown, 0),
        (FpRoundMode::RoundUp, 1),
        (FpRoundMode::RoundTowardZero, 0),
    ] {
        let underflow =
            SmirInterpreter::x86_simd_fp_div(0x0080_0000, 0x7F7F_FFFF, X86_SIMD_F32, mode, 0x1F80);
        assert_eq!(underflow.bits, expected, "underflow {mode:?}");
        assert_eq!(underflow.status, (1 << 4) | (1 << 5));
    }
    let ftz = SmirInterpreter::x86_simd_fp_div(
        0x0080_0000,
        0x7F7F_FFFF,
        X86_SIMD_F32,
        FpRoundMode::RoundUp,
        0x1F80 | (1 << 15),
    );
    assert_eq!(ftz.bits, 0);
    assert_eq!(ftz.status, (1 << 4) | (1 << 5));

    for min in [true, false] {
        let signed_zero = SmirInterpreter::x86_simd_fp_min_max(
            u64::from(0.0f32.to_bits()),
            u64::from((-0.0f32).to_bits()),
            X86_SIMD_F32,
            0x1F80,
            min,
        );
        assert_eq!(signed_zero.bits, u64::from((-0.0f32).to_bits()));
        assert_eq!(signed_zero.status, 0);

        let qnan_src2 = SmirInterpreter::x86_simd_fp_min_max(
            u64::from(1.0f32.to_bits()),
            0x7FC1_2345,
            X86_SIMD_F32,
            0x1F80,
            min,
        );
        assert_eq!(qnan_src2.bits, 0x7FC1_2345);
        assert_eq!(qnan_src2.status, 1, "QNaN reports invalid");

        let snan_src2 = SmirInterpreter::x86_simd_fp_min_max(
            u64::from(1.0f32.to_bits()),
            0x7F81_2345,
            X86_SIMD_F32,
            0x1F80,
            min,
        );
        assert_eq!(snan_src2.bits, 0x7F81_2345, "src2 SNaN stays unquieted");
        assert_eq!(snan_src2.status, 1);
    }
}

#[test]
fn vfp16_arithmetic_accrues_exact_status_honors_sae_and_traps_before_commit() {
    fn execute(
        op: Avx10FP16Op,
        first: u16,
        second: u16,
        mxcsr: u32,
        round: FpRoundMode,
        active: bool,
    ) -> (BlockResult, VecValue, u32) {
        let destination = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let source1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        let source2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
        let mask = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::VFP16Arith {
                dst: destination,
                src1: source1,
                src2: source2,
                mask: Some(mask),
                op,
                round,
                width: VecWidth::V128,
                zeroing: false,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();

        let sentinel = [0xA55A_3CC3_F00F_9669; 16];
        let mut context = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
            x86.xmm[0] = sentinel;
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 16, u64::from(first));
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 16, u64::from(second));
            x86.k[1] = u64::from(active);
            x86.mxcsr = mxcsr;
        }
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &function.blocks[0],
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        (result, x86.xmm[0], x86.mxcsr)
    }

    const IE: u32 = 1;
    const DE: u32 = 1 << 1;
    const ZE: u32 = 1 << 2;
    const OE: u32 = 1 << 3;
    const UE: u32 = 1 << 4;
    const PE: u32 = 1 << 5;
    const DAZ: u32 = 1 << 6;
    const PM: u32 = 1 << 12;
    const FTZ: u32 = 1 << 15;

    let lane = |value: &VecValue| SmirInterpreter::get_lane(value, 0, 16) as u16;

    let (exit, result, mxcsr) = execute(
        Avx10FP16Op::Sqrt,
        0x4000,
        0x4000,
        0x1F80,
        FpRoundMode::Dynamic,
        true,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(lane(&result), 0x3DA8, "RN(sqrt(2))");
    assert_eq!(mxcsr, 0x1F80 | PE);

    let (exit, result, mxcsr) = execute(
        Avx10FP16Op::Sqrt,
        0x4000,
        0x4000,
        0x1F80,
        FpRoundMode::RoundUp,
        true,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(lane(&result), 0x3DA9, "RU(sqrt(2))");
    assert_eq!(mxcsr, 0x1F80, "embedded rounding implies SAE");

    for (op, first, second, expected, status) in [
        (Avx10FP16Op::Sqrt, 0xBC00, 0xBC00, 0xFE00, IE),
        (Avx10FP16Op::Div, 0x3C00, 0x0000, 0x7C00, ZE),
        (Avx10FP16Op::Mul, 0x7BFF, 0x4000, 0x7C00, OE | PE),
        (Avx10FP16Op::Mul, 0x0400, 0x0400, 0x0000, UE | PE),
    ] {
        let (exit, result, mxcsr) = execute(op, first, second, 0x1F80, FpRoundMode::Dynamic, true);
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(lane(&result), expected, "{op:?}");
        assert_eq!(mxcsr, 0x1F80 | status, "{op:?}");
    }

    let (exit, result, mxcsr) = execute(
        Avx10FP16Op::Sqrt,
        0x0001,
        0x0001,
        0x1F80 | DAZ | FTZ,
        FpRoundMode::Dynamic,
        true,
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(lane(&result), 0x0C00, "sqrt(2^-24) = 2^-12");
    assert_eq!(mxcsr, 0x1F80 | DAZ | FTZ | DE);

    let sentinel = [0xA55A_3CC3_F00F_9669; 16];
    let (exit, result, mxcsr) = execute(
        Avx10FP16Op::Sqrt,
        0x4000,
        0x4000,
        0x1F80 & !PM,
        FpRoundMode::Dynamic,
        true,
    );
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    assert_eq!(result, sentinel, "unmasked PE precedes destination commit");
    assert_eq!(mxcsr, (0x1F80 & !PM) | PE);

    let (exit, result, mxcsr) = execute(Avx10FP16Op::Div, 0, 0, 0, FpRoundMode::Dynamic, false);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(result[0], sentinel[0], "inactive lane merges destination");
    assert_eq!(mxcsr, 0, "inactive 0/0 reports no status");
}

#[test]
fn lifted_vex_scalar_binary_operations_use_exact_mxcsr_semantics_and_lane_contract() {
    for (opcode, expected) in [
        (0x58, 3.5f32.to_bits()),
        (0x59, 3.0f32.to_bits()),
        (0x5C, (-0.5f32).to_bits()),
        (0x5D, 1.5f32.to_bits()),
        (0x5E, 0.75f32.to_bits()),
        (0x5F, 2.0f32.to_bits()),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[1],
                    lane,
                    32,
                    if lane == 0 {
                        u64::from(1.5f32.to_bits())
                    } else {
                        u64::from((10.0 + f32::from(lane)).to_bits())
                    },
                );
            }
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, u64::from(2.0f32.to_bits()));
            x86.xmm[0] = [u64::MAX; 16];
            x86.mxcsr = 0x1F80;
        }
        let exit = execute_lifted_x86(
            &[0xC5, 0xF2, opcode, 0xC2],
            &mut ctx,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[0], 0, 32),
                u64::from(expected)
            );
            for lane in 1..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[0], lane, 32),
                    SmirInterpreter::get_lane(&x86.xmm[1], lane, 32),
                    "opcode {opcode:02X}, upper XMM lane {lane}"
                );
            }
            for lane in 4..16u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[0], lane, 32), 0);
            }
            assert_eq!(x86.mxcsr, 0x1F80, "all table inputs are exact");
        }
    }
}

#[test]
fn x86_fp_div_rounding_matches_independent_rational_reference_grid() {
    fn finite_parts(bits: u64, format: X86SimdFpFormat) -> (u128, i32) {
        let exponent_field =
            ((bits >> format.fraction_bits) & ((1u64 << format.exponent_bits) - 1)) as i32;
        let fraction = u128::from(bits & ((1u64 << format.fraction_bits) - 1));
        if exponent_field == 0 {
            (fraction, 1 - format.bias - format.fraction_bits as i32)
        } else {
            (
                fraction | (1u128 << format.fraction_bits),
                exponent_field - format.bias - format.fraction_bits as i32,
            )
        }
    }

    fn normalize(mut magnitude: u128, mut exponent: i32) -> (u128, i32) {
        let zeros = magnitude.trailing_zeros();
        magnitude >>= zeros;
        exponent += zeros as i32;
        (magnitude, exponent)
    }

    fn compare_candidate(
        candidate: u64,
        numerator: u64,
        denominator: u64,
        format: X86SimdFpFormat,
    ) -> std::cmp::Ordering {
        let (candidate_significand, candidate_exponent) = finite_parts(candidate, format);
        let (numerator, numerator_exponent) = finite_parts(numerator, format);
        let (denominator, denominator_exponent) = finite_parts(denominator, format);
        let (left, left_exponent) = normalize(numerator, numerator_exponent);
        let (right, right_exponent) = normalize(
            denominator * candidate_significand,
            denominator_exponent + candidate_exponent,
        );
        let left_top = 127 - left.leading_zeros() as i32 + left_exponent;
        let right_top = 127 - right.leading_zeros() as i32 + right_exponent;
        match left_top.cmp(&right_top) {
            std::cmp::Ordering::Equal => {
                let common_exponent = left_exponent.min(right_exponent);
                let left_shift = (left_exponent - common_exponent) as u32;
                let right_shift = (right_exponent - common_exponent) as u32;
                debug_assert!(left_shift < 128 && right_shift < 128);
                // Compare exact numerator with denominator*candidate. Reverse
                // the ordering so the result is candidate relative to exact.
                (right << right_shift).cmp(&(left << left_shift))
            }
            ordering => ordering.reverse(),
        }
    }

    fn verify(numerator: u64, denominator: u64, format: X86SimdFpFormat) {
        let exponent_mask = ((1u64 << format.exponent_bits) - 1) << format.fraction_bits;
        if numerator & !((1u64 << (format.total_bits - 1)) - 1) != 0
            || denominator & !((1u64 << (format.total_bits - 1)) - 1) != 0
            || numerator & exponent_mask == exponent_mask
            || denominator & exponent_mask == exponent_mask
            || numerator & (exponent_mask | ((1u64 << format.fraction_bits) - 1)) == 0
            || denominator & (exponent_mask | ((1u64 << format.fraction_bits) - 1)) == 0
        {
            return;
        }
        let nearest = if format.total_bits == 32 {
            u64::from(
                (f32::from_bits(numerator as u32) / f32::from_bits(denominator as u32)).to_bits(),
            )
        } else {
            (f64::from_bits(numerator) / f64::from_bits(denominator)).to_bits()
        };
        if nearest == 0 || nearest & exponent_mask == exponent_mask {
            return;
        }
        let ordering = compare_candidate(nearest, numerator, denominator, format);
        let (down, up) = match ordering {
            std::cmp::Ordering::Less => (nearest, nearest + 1),
            std::cmp::Ordering::Equal => (nearest, nearest),
            std::cmp::Ordering::Greater => (nearest - 1, nearest),
        };
        let inexact = ordering != std::cmp::Ordering::Equal;
        let fraction_mask = (1u64 << format.fraction_bits) - 1;
        let denormal_input = |bits: u64| bits & exponent_mask == 0 && bits & fraction_mask != 0;
        let expected_status = (u32::from(denormal_input(numerator) || denormal_input(denominator))
            << 1)
            | (u32::from(inexact) << 5);
        for (mode, expected) in [
            (FpRoundMode::RoundNearest, nearest),
            (FpRoundMode::RoundDown, down),
            (FpRoundMode::RoundUp, up),
            (FpRoundMode::RoundTowardZero, down),
        ] {
            let actual =
                SmirInterpreter::x86_simd_fp_div(numerator, denominator, format, mode, 0x1F80);
            assert_eq!(
                actual.bits, expected,
                "format={} numerator={numerator:016X} denominator={denominator:016X} mode={mode:?}",
                format.total_bits
            );
            assert_eq!(
                actual.status & ((1 << 1) | (1 << 5)),
                expected_status,
                "status format={} numerator={numerator:016X} denominator={denominator:016X}",
                format.total_bits
            );
        }
    }

    for numerator in [
        1u32,
        2,
        0x007F_FFFF,
        0x0080_0000,
        0x3F7F_FFFF,
        0x3F80_0000,
        0x4000_0000,
        0x7F7F_FFFF,
    ] {
        for denominator in [1u32, 3, 0x007F_FFFF, 0x0080_0000, 0x3F80_0000, 0x4040_0000] {
            verify(u64::from(numerator), u64::from(denominator), X86_SIMD_F32);
        }
    }
    for numerator in [
        1u64,
        2,
        0x000F_FFFF_FFFF_FFFF,
        0x0010_0000_0000_0000,
        0x3FEF_FFFF_FFFF_FFFF,
        0x3FF0_0000_0000_0000,
        0x4000_0000_0000_0000,
        0x7FEF_FFFF_FFFF_FFFF,
    ] {
        for denominator in [
            1u64,
            3,
            0x000F_FFFF_FFFF_FFFF,
            0x0010_0000_0000_0000,
            0x3FF0_0000_0000_0000,
            0x4008_0000_0000_0000,
        ] {
            verify(numerator, denominator, X86_SIMD_F64);
        }
    }

    let mut state = 0xD1B5_4A32_D192_ED03u64;
    for _ in 0..2_048 {
        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xA076_1D64_78BD_642F);
        let numerator32 = (state as u32 & 0x7F7F_FFFF).max(1);
        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xE703_7ED1_A0B4_28DB);
        let denominator32 = (state as u32 & 0x7F7F_FFFF).max(1);
        verify(
            u64::from(numerator32),
            u64::from(denominator32),
            X86_SIMD_F32,
        );

        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x8EBC_6AF0_9C88_C6E3);
        let numerator64 = (state & 0x7FEF_FFFF_FFFF_FFFF).max(1);
        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x5899_65CC_7537_4CC3);
        let denominator64 = (state & 0x7FEF_FFFF_FFFF_FFFF).max(1);
        verify(numerator64, denominator64, X86_SIMD_F64);
    }
}

#[test]
fn lifted_evex_scalar_binary_er_rounds_exactly_merges_upper_bits_and_is_state_silent() {
    let mut observed = Vec::new();
    for (p2, expected) in [
        (0x18, 0x3F80_0000u64),
        (0x38, 0x3F80_0000),
        (0x58, 0x3F80_0001),
        (0x78, 0x3F80_0000),
    ] {
        let mut ctx = SmirContext::new_x86_64();
        let before = (0x1F80 & !(1 << 12)) | (2 << 13);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            for lane in 0..4u8 {
                SmirInterpreter::set_lane(
                    &mut x86.xmm[0],
                    lane,
                    32,
                    if lane == 0 {
                        u64::from(1.0f32.to_bits())
                    } else {
                        u64::from((10.0 + f32::from(lane)).to_bits())
                    },
                );
            }
            SmirInterpreter::set_lane(
                &mut x86.xmm[1],
                0,
                32,
                u64::from((2.0f32).powi(-24).to_bits()),
            );
            x86.xmm[2] = [0xA5A5_A5A5_A5A5_A5A5; 16];
            x86.mxcsr = before;
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF1, 0x7E, p2, 0x58, 0xD1],
            &mut ctx,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[2], 0, 32), expected);
            for lane in 1..4u8 {
                assert_eq!(
                    SmirInterpreter::get_lane(&x86.xmm[2], lane, 32),
                    SmirInterpreter::get_lane(&x86.xmm[0], lane, 32),
                    "upper XMM lane {lane}"
                );
            }
            for lane in 4..16u8 {
                assert_eq!(SmirInterpreter::get_lane(&x86.xmm[2], lane, 32), 0);
            }
            assert_eq!(x86.mxcsr, before, "ER implies SAE");
            observed.push(x86.xmm[2]);
        }
    }
    assert_ne!(observed[0], observed[2]);
}

#[test]
fn lifted_evex_scalar_binary_masks_faults_and_exceptions_atomically() {
    let sentinel = [0xCAFE_BABE_DEAD_BEEF; 16];
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 32, u64::from(1.0f32.to_bits()));
        SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, 0);
        x86.xmm[2] = sentinel;
        x86.k[1] = 1;
        x86.mxcsr = 0x1F80 & !(1 << 9);
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0x7E, 0x09, 0x5E, 0xD1],
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], sentinel, "#XM precedes all destination writes");
        assert_eq!(x86.mxcsr & (1 << 2), 1 << 2);
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    ctx.write_vreg(rax, 0x100);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        x86.k[1] = 0;
        x86.mxcsr = 0;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0x7E, 0x09, 0x5E, 0x10],
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            SmirInterpreter::get_lane(&x86.xmm[2], 0, 32),
            sentinel[0] & 0xFFFF_FFFF,
            "merge mask keeps low lane"
        );
        assert_eq!(x86.mxcsr, 0, "inactive lane reports no FP status");
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = sentinel;
        x86.k[1] = 0;
        x86.mxcsr = 0;
    }
    let exit = execute_lifted_x86(
        &[0x62, 0xF1, 0x7E, 0x89, 0x5E, 0x10],
        &mut ctx,
        &mut FlatMemory::new(0x100),
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[2], [0; 16], "zero mask clears low and upper state");
        assert_eq!(x86.mxcsr, 0);
    }
}

#[test]
fn lifted_evex_scalar_minmax_sae_preserves_raw_src2_nan_without_mxcsr_status() {
    for opcode in [0x5Du8, 0x5F] {
        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 32, u64::from(1.0f32.to_bits()));
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, 0x7F81_2345);
            x86.mxcsr = 0;
        }
        let exit = execute_lifted_x86(
            &[0x62, 0xF1, 0x7E, 0x18, opcode, 0xD1],
            &mut ctx,
            &mut FlatMemory::new(0x100),
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(SmirInterpreter::get_lane(&x86.xmm[2], 0, 32), 0x7F81_2345);
            assert_eq!(x86.mxcsr, 0, "SAE suppresses invalid status");
        }
    }
}

#[test]
fn optimized_evex_scalar_binary_er_matches_o0_o1_o2() {
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::optimize::{OptLevel, optimize_function};

    let mut observed = Vec::new();
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut lifter = X86_64Lifter::strict();
        let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
        let lifted = lifter
            .lift_insn(0x1000, &[0x62, 0xF1, 0x7E, 0x58, 0x5E, 0xD1], &mut lift_ctx)
            .unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        function.blocks[0].ops = lifted.ops;
        optimize_function(&mut function, level);

        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[0], 0, 32, u64::from(1.0f32.to_bits()));
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, u64::from(3.0f32.to_bits()));
            x86.mxcsr = 0;
        }
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x100),
            &function.blocks[0],
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            observed.push((x86.xmm[2], x86.mxcsr));
        }
    }
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[0], observed[2]);
    assert_eq!(
        SmirInterpreter::get_lane(&observed[0].0, 0, 32),
        0x3EAA_AAAB
    );
    assert_eq!(observed[0].1, 0, "ER suppresses precision status");
}

#[test]
fn optimizer_retains_dead_dynamic_x86_fp_binary_status_at_o0_o1_o2() {
    use crate::smir::optimize::{OptLevel, optimize_function};

    let src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let src2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86FpBinary {
                dst: VReg::Virtual(VirtualId(77)),
                src1,
                src2,
                mask: None,
                elem: VecElementType::F32,
                lanes: 1,
                op: X86FpBinaryOp::Div,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        optimize_function(&mut function, level);
        assert!(
            function.blocks[0]
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86FpBinary { .. }))
        );

        let mut ctx = SmirContext::new_x86_64();
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            SmirInterpreter::set_lane(&mut x86.xmm[1], 0, 32, u64::from(1.0f32.to_bits()));
            SmirInterpreter::set_lane(&mut x86.xmm[2], 0, 32, 0);
            x86.mxcsr = 0x1F80;
        }
        let exit = SmirInterpreter::new().execute_block(
            &mut ctx,
            &mut FlatMemory::new(0x100),
            &function.blocks[0],
        );
        assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.mxcsr & (1 << 2), 1 << 2, "{level:?}");
        }
    }
}
