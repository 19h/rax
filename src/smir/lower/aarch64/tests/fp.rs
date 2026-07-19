//! tests::fp tests

use super::*;
use crate::smir::lower::aarch64::*;

#[test]
fn lowers_scalar_fp_binary_encodings() {
    let words = code_words(&lower_single_op(OpKind::FAdd {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        precision: FpPrecision::F32,
    }));
    assert_eq!(words, vec![0x1e22_2820, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FDiv {
        dst: v(3),
        src1: v(4),
        src2: v(5),
        precision: FpPrecision::F64,
    }));
    assert_eq!(words, vec![0x1e65_1883, 0xd65f_03c0]);
}
#[test]
fn lowers_scalar_fp_binary_f32_runtime() {
    assert_fp_binary_f32(
        "fadd_s",
        OpKind::FAdd {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        1.5,
        2.25,
        3.75,
    );
    assert_fp_binary_f32(
        "fsub_s",
        OpKind::FSub {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        5.5,
        1.25,
        4.25,
    );
    assert_fp_binary_f32(
        "fmul_s",
        OpKind::FMul {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        -3.0,
        2.0,
        -6.0,
    );
    assert_fp_binary_f32(
        "fdiv_s",
        OpKind::FDiv {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        7.5,
        2.5,
        3.0,
    );
    assert_fp_binary_f32(
        "fmin_s",
        OpKind::FMin {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        -4.0,
        1.0,
        -4.0,
    );
    assert_fp_binary_f32(
        "fmax_s",
        OpKind::FMax {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        -4.0,
        1.0,
        1.0,
    );
}
#[test]
fn lowers_scalar_fp_binary_f64_runtime() {
    assert_fp_binary_f64(
        "fadd_d",
        OpKind::FAdd {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        1.5,
        2.25,
        3.75,
    );
    assert_fp_binary_f64(
        "fsub_d",
        OpKind::FSub {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        5.5,
        1.25,
        4.25,
    );
    assert_fp_binary_f64(
        "fmul_d",
        OpKind::FMul {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        -3.0,
        2.0,
        -6.0,
    );
    assert_fp_binary_f64(
        "fdiv_d",
        OpKind::FDiv {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        7.5,
        2.5,
        3.0,
    );
    assert_fp_binary_f64(
        "fmin_d",
        OpKind::FMin {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        -4.0,
        1.0,
        -4.0,
    );
    assert_fp_binary_f64(
        "fmax_d",
        OpKind::FMax {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        -4.0,
        1.0,
        1.0,
    );
}
#[test]
fn lowers_fp_minmax_nan_semantics() {
    // The scalar path lifts both FMAX/FMAXNM -> OpKind::FMax (and FMIN/
    // FMINNM -> FMin), so scalar FMax/FMin lower to the numeric FMAXNM/
    // FMINNM opcodes to match the interpreter's `a.max(b)`/`a.min(b)`.
    //
    // The vector path is DISTINCT: the lifter maps architectural FMAX/FMIN
    // -> VMax/VMin (NaN-PROPAGATING) and FMAXNM/FMINNM -> VFMinMaxNm
    // (numeric). So vector VMax/VMin must keep the propagating opcode
    // (0b11110) and a lone NaN lane WINS, while VFMinMaxNm stays numeric.
    // (#159 — the earlier #56 change wrongly made VMax/VMin numeric too.)
    let n32 = f32::NAN;
    let n64 = f64::NAN;

    // Scalar: a lone NaN must lose to the finite operand, both orderings.
    assert_fp_binary_f32(
        "fmax_s_nan_lhs",
        OpKind::FMax {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        n32,
        2.5,
        n32.max(2.5),
    );
    assert_fp_binary_f32(
        "fmax_s_nan_rhs",
        OpKind::FMax {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        2.5,
        n32,
        2.5_f32.max(n32),
    );
    assert_fp_binary_f32(
        "fmin_s_nan_lhs",
        OpKind::FMin {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F32,
        },
        n32,
        2.5,
        n32.min(2.5),
    );
    assert_fp_binary_f64(
        "fmax_d_nan_lhs",
        OpKind::FMax {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        n64,
        -3.0,
        n64.max(-3.0),
    );
    assert_fp_binary_f64(
        "fmin_d_nan_rhs",
        OpKind::FMin {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F64,
        },
        -3.0,
        n64,
        (-3.0_f64).min(n64),
    );

    // Vector: VMax/VMin PROPAGATE a NaN lane; VFMinMaxNm is numeric.
    fn apply_f32<F: Fn(f32, f32) -> f32>(a: [f32; 4], b: [f32; 4], op: F) -> (u64, u64) {
        simd_pair_from_f32([
            op(a[0], b[0]),
            op(a[1], b[1]),
            op(a[2], b[2]),
            op(a[3], b[3]),
        ])
    }
    // NaN-propagating max/min (architectural FMAX/FMIN): a lone quiet NaN
    // wins. Matches hardware FMAX/FMIN for quiet-NaN inputs.
    let fmax_prop = |a: f32, b: f32| {
        if a.is_nan() {
            a
        } else if b.is_nan() {
            b
        } else {
            a.max(b)
        }
    };
    let fmin_prop = |a: f32, b: f32| {
        if a.is_nan() {
            a
        } else if b.is_nan() {
            b
        } else {
            a.min(b)
        }
    };
    let a32 = [f32::NAN, -2.25, 8.0, -0.5];
    let b32 = [2.25, f32::NAN, -1.5, 4.0];
    let code = lower_ops(vec![
        OpKind::VMax {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VMin {
            dst: v(6),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
            signed: false,
        },
        // VFMinMaxNm (architectural FMAXNM): numeric — a lone NaN lane loses.
        OpKind::VFMinMaxNm {
            dst: v(7),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
            min: false,
        },
    ]);
    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (1, simd_pair_from_f32(a32).0, simd_pair_from_f32(a32).1),
            (2, simd_pair_from_f32(b32).0, simd_pair_from_f32(b32).1),
        ],
    );
    // VMax/VMin propagate the NaN lanes (lanes 0,1 are NaN in the inputs).
    assert_eq!(
        simd[5],
        apply_f32(a32, b32, fmax_prop),
        "vmax f32 propagates NaN"
    );
    assert_eq!(
        simd[6],
        apply_f32(a32, b32, fmin_prop),
        "vmin f32 propagates NaN"
    );
    // VFMinMaxNm stays numeric: the NaN lane loses to the finite lane.
    assert_eq!(
        simd[7],
        apply_f32(a32, b32, f32::max),
        "vfminmaxnm f32 numeric NaN"
    );
}
#[test]
fn rejects_scalar_fp_binary_unsupported_precision() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::FAdd {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F80,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_scalar_fp_compare_encodings() {
    let words = code_words(&lower_single_op(OpKind::FCmp {
        src1: v(1),
        src2: v(2),
        precision: FpPrecision::F32,
    }));
    assert_eq!(words, vec![0x1e22_2020, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FCmp {
        src1: v(3),
        src2: v(4),
        precision: FpPrecision::F64,
    }));
    assert_eq!(words, vec![0x1e64_2060, 0xd65f_03c0]);
}
#[test]
fn lowers_scalar_fp_compare_f32_runtime() {
    assert_fp_compare_f32("fcmp_s_less", 1.0, 2.0, 0b1000);
    assert_fp_compare_f32("fcmp_s_greater", 2.0, 1.0, 0b0010);
    assert_fp_compare_f32("fcmp_s_equal", 2.0, 2.0, 0b0110);
}
#[test]
fn lowers_scalar_fp_compare_f64_runtime() {
    assert_fp_compare_f64("fcmp_d_less", 1.0, 2.0, 0b1000);
    assert_fp_compare_f64("fcmp_d_greater", 2.0, 1.0, 0b0010);
    assert_fp_compare_f64("fcmp_d_equal", 2.0, 2.0, 0b0110);
}
#[test]
fn rejects_scalar_fp_compare_unsupported_precision() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::FCmp {
            src1: v(1),
            src2: v(2),
            precision: FpPrecision::F80,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_scalar_fp_convert_encodings() {
    let words = code_words(&lower_single_op(OpKind::FConvert {
        dst: v(0),
        src: v(1),
        from: FpPrecision::F32,
        to: FpPrecision::F64,
    }));
    assert_eq!(words, vec![0x1e22_c020, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FConvert {
        dst: v(2),
        src: v(3),
        from: FpPrecision::F64,
        to: FpPrecision::F32,
    }));
    assert_eq!(words, vec![0x1e62_4062, 0xd65f_03c0]);
}
#[test]
fn lowers_scalar_fp_convert_runtime() {
    assert_fp_convert_f32_to_f64("fcvt_d_s_positive", 3.5);
    assert_fp_convert_f32_to_f64("fcvt_d_s_negative", -0.25);
    assert_fp_convert_f64_to_f32("fcvt_s_d_positive", 1.25);
    assert_fp_convert_f64_to_f32("fcvt_s_d_rounded", 1.0 + f64::EPSILON);
    assert_fp_convert_same_f32("fcvt_s_s_copy", -2.75);
}
#[test]
fn rejects_scalar_fp_convert_unsupported_precision() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::FConvert {
            dst: v(0),
            src: v(1),
            from: FpPrecision::F32,
            to: FpPrecision::F80,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_scalar_int_to_fp_encodings() {
    let words = code_words(&lower_single_op(OpKind::IntToFp {
        dst: v(0),
        src: x(1),
        int_width: OpWidth::W32,
        fp_precision: FpPrecision::F32,
        signed: true,
    }));
    assert_eq!(words, vec![0x1e22_0020, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::IntToFp {
        dst: v(2),
        src: x(3),
        int_width: OpWidth::W64,
        fp_precision: FpPrecision::F64,
        signed: false,
    }));
    assert_eq!(words, vec![0x9e63_0062, 0xd65f_03c0]);
}
#[test]
fn lowers_scalar_int_to_fp_runtime() {
    assert_int_to_fp_f32(
        "scvtf_s_w",
        OpKind::IntToFp {
            dst: v(0),
            src: x(1),
            int_width: OpWidth::W32,
            fp_precision: FpPrecision::F32,
            signed: true,
        },
        u64::from((-7_i32) as u32),
        -7.0,
    );
    assert_int_to_fp_f64(
        "ucvtf_d_x",
        OpKind::IntToFp {
            dst: v(0),
            src: x(1),
            int_width: OpWidth::W64,
            fp_precision: FpPrecision::F64,
            signed: false,
        },
        1_234_567_890_123,
        1_234_567_890_123.0,
    );
    assert_int_to_fp_f32(
        "scvtf_s_b",
        OpKind::IntToFp {
            dst: v(0),
            src: x(1),
            int_width: OpWidth::W8,
            fp_precision: FpPrecision::F32,
            signed: true,
        },
        0x80,
        -128.0,
    );
    assert_int_to_fp_f64(
        "ucvtf_d_h",
        OpKind::IntToFp {
            dst: v(0),
            src: x(1),
            int_width: OpWidth::W16,
            fp_precision: FpPrecision::F64,
            signed: false,
        },
        0xffff,
        65_535.0,
    );
}
#[test]
fn rejects_scalar_int_to_fp_unsupported_precision() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::IntToFp {
            dst: v(0),
            src: x(1),
            int_width: OpWidth::W32,
            fp_precision: FpPrecision::F80,
            signed: true,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_scalar_fp_to_int_encodings() {
    let words = code_words(&lower_single_op(OpKind::FpToInt {
        dst: x(0),
        src: v(1),
        fp_precision: FpPrecision::F32,
        int_width: OpWidth::W32,
        signed: true,
        round: FpRoundMode::RoundDown,
    }));
    assert_eq!(words, vec![0x1e30_0020, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FpToInt {
        dst: x(2),
        src: v(3),
        fp_precision: FpPrecision::F64,
        int_width: OpWidth::W64,
        signed: false,
        round: FpRoundMode::RoundUp,
    }));
    assert_eq!(words, vec![0x9e69_0062, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FpToInt {
        dst: x(0),
        src: v(1),
        fp_precision: FpPrecision::F32,
        int_width: OpWidth::W32,
        signed: true,
        round: FpRoundMode::RoundNearestTiesAway,
    }));
    assert_eq!(words, vec![0x1e24_0020, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FpToInt {
        dst: x(2),
        src: v(3),
        fp_precision: FpPrecision::F64,
        int_width: OpWidth::W64,
        signed: false,
        round: FpRoundMode::RoundNearestTiesAway,
    }));
    assert_eq!(words, vec![0x9e65_0062, 0xd65f_03c0]);
}
#[test]
fn lowers_scalar_fp_to_int_runtime() {
    assert_fp_to_int_f32(
        "fcvtms_w_s_round_down",
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F32,
            int_width: OpWidth::W32,
            signed: true,
            round: FpRoundMode::RoundDown,
        },
        -7.9,
        u64::from((-8_i32) as u32),
    );
    assert_fp_to_int_f64(
        "fcvtnu_x_d_round_nearest",
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F64,
            int_width: OpWidth::W64,
            signed: false,
            round: FpRoundMode::RoundNearest,
        },
        1_234_567_890_123.75,
        1_234_567_890_124,
    );
    assert_fp_to_int_f64(
        "fcvtzs_b_d_large_low_bits",
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F64,
            int_width: OpWidth::W8,
            signed: true,
            round: FpRoundMode::RoundTowardZero,
        },
        4_294_967_297.0,
        1,
    );
    assert_fp_to_int_f64(
        "fcvtzu_h_d_large_low_bits",
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F64,
            int_width: OpWidth::W16,
            signed: false,
            round: FpRoundMode::RoundTowardZero,
        },
        4_294_967_297.0,
        1,
    );
    assert_fp_to_int_f32(
        "fcvtas_w_s_ties_away_positive",
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F32,
            int_width: OpWidth::W32,
            signed: true,
            round: FpRoundMode::RoundNearestTiesAway,
        },
        2.5,
        3,
    );
    assert_fp_to_int_f32(
        "fcvtas_w_s_ties_away_negative",
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F32,
            int_width: OpWidth::W32,
            signed: true,
            round: FpRoundMode::RoundNearestTiesAway,
        },
        -2.5,
        u64::from((-3_i32) as u32),
    );
}
#[test]
fn lowers_scalar_fp_to_int_dynamic_runtime_uses_fpcr_rounding() {
    let code = lower_ops(vec![
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F64,
            int_width: OpWidth::W64,
            signed: true,
            round: FpRoundMode::Dynamic,
        },
        OpKind::TestCondition {
            dst: x(2),
            cond: Condition::Eq,
        },
    ]);
    let scratch16 = 0x1616_1616_1616_1616;
    let scratch31 = (0x3131_3131_3131_3131, 0x1313_1313_1313_1313);
    let cases = [
        (0b00, 3.5_f64, 4_u64),
        (0b01, 2.1_f64, 3_u64),
        (0b10, -2.1_f64, (-3_i64) as u64),
        (0b11, -2.9_f64, (-2_i64) as u64),
    ];

    for (rmode, input, expected) in cases {
        let fpcr_in = rmode << 22;
        let (regs, simd, sp, fpcr) = run_aarch64_code_with_regs_simd_and_fpcr(
            &code,
            &[(0, 0x1234_5678_9abc_def0), (16, scratch16)],
            &[(1, input.to_bits(), 0), (31, scratch31.0, scratch31.1)],
            fpcr_in,
        );

        assert_eq!(regs[0], expected, "FPCR.RMode={rmode:#04b}");
        assert_eq!(regs[2], 0, "FPCR.RMode={rmode:#04b}");
        assert_eq!(regs[16], scratch16, "FPCR.RMode={rmode:#04b}");
        assert_eq!(simd[1], (input.to_bits(), 0), "FPCR.RMode={rmode:#04b}");
        assert_eq!(simd[31], scratch31, "FPCR.RMode={rmode:#04b}");
        assert_eq!(sp, 0x8000, "FPCR.RMode={rmode:#04b}");
        assert_eq!(fpcr, fpcr_in, "FPCR.RMode={rmode:#04b}");
    }
}
#[test]
fn lowers_scalar_fp_int_conversion_apx_egpr_operands_runtime() {
    let code = lower_ops(vec![
        OpKind::IntToFp {
            dst: v(2),
            src: x86(X86Reg::R17),
            int_width: OpWidth::W64,
            fp_precision: FpPrecision::F64,
            signed: false,
        },
        OpKind::IntToFp {
            dst: v(3),
            src: x86(X86Reg::R18),
            int_width: OpWidth::W8,
            fp_precision: FpPrecision::F32,
            signed: true,
        },
        OpKind::FpToInt {
            dst: x86(X86Reg::R19),
            src: v(4),
            fp_precision: FpPrecision::F32,
            int_width: OpWidth::W32,
            signed: true,
            round: FpRoundMode::RoundTowardZero,
        },
        OpKind::FpToInt {
            dst: x86(X86Reg::R20),
            src: v(5),
            fp_precision: FpPrecision::F64,
            int_width: OpWidth::W16,
            signed: false,
            round: FpRoundMode::RoundNearest,
        },
    ]);
    let regs = [
        (16, 0x1616_1616_1616_1616),
        (17, 1_234_567_890_123),
        (18, 0xff),
        (21, 0x2121_2121_2121_2121),
    ];
    let simd_regs = [
        (2, 0xffff_ffff_ffff_ffff, 0x2222_2222_2222_2222),
        (3, 0xffff_ffff_ffff_ffff, 0x3333_3333_3333_3333),
        (4, u64::from((-7.9_f32).to_bits()), 0x4444_4444_4444_4444),
        (5, 65_537.0_f64.to_bits(), 0x5555_5555_5555_5555),
    ];
    let (regs, simd, sp) = run_aarch64_code_with_regs_and_simd(&code, &regs, &simd_regs);

    assert_eq!(simd[2], (1_234_567_890_123.0_f64.to_bits(), 0));
    assert_eq!(simd[3], (u64::from((-1.0_f32).to_bits()), 0));
    assert_eq!(regs[19], u64::from((-7_i32) as u32));
    assert_eq!(regs[20], 1);
    assert_eq!(regs[16], 0x1616_1616_1616_1616);
    assert_eq!(regs[17], 1_234_567_890_123);
    assert_eq!(regs[18], 0xff);
    assert_eq!(regs[21], 0x2121_2121_2121_2121);
    assert_eq!(
        simd[4],
        (u64::from((-7.9_f32).to_bits()), 0x4444_4444_4444_4444)
    );
    assert_eq!(simd[5], (65_537.0_f64.to_bits(), 0x5555_5555_5555_5555));
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_scalar_fp_int_conversion_apx_r31_identity_mapping() {
    for kind in [
        OpKind::IntToFp {
            dst: v(0),
            src: x86(X86Reg::R31),
            int_width: OpWidth::W64,
            fp_precision: FpPrecision::F64,
            signed: false,
        },
        OpKind::IntToFp {
            dst: v(0),
            src: x86(X86Reg::R31),
            int_width: OpWidth::W8,
            fp_precision: FpPrecision::F32,
            signed: true,
        },
        OpKind::FpToInt {
            dst: x86(X86Reg::R31),
            src: v(0),
            fp_precision: FpPrecision::F64,
            int_width: OpWidth::W64,
            signed: false,
            round: FpRoundMode::RoundTowardZero,
        },
        OpKind::FpToInt {
            dst: x86(X86Reg::R31),
            src: v(0),
            fp_precision: FpPrecision::F32,
            int_width: OpWidth::W8,
            signed: true,
            round: FpRoundMode::RoundTowardZero,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn rejects_scalar_fp_to_int_unsupported_precision() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::FpToInt {
            dst: x(0),
            src: v(1),
            fp_precision: FpPrecision::F80,
            int_width: OpWidth::W32,
            signed: true,
            round: FpRoundMode::RoundTowardZero,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_scalar_fp_fma_encodings() {
    let words = code_words(&lower_single_op(OpKind::FFma {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        src3: v(3),
        precision: FpPrecision::F32,
    }));
    assert_eq!(words, vec![0x1f02_0c20, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FFma {
        dst: v(4),
        src1: v(5),
        src2: v(6),
        src3: v(7),
        precision: FpPrecision::F64,
    }));
    assert_eq!(words, vec![0x1f46_1ca4, 0xd65f_03c0]);
}
#[test]
fn lowers_scalar_fp_fma_f32_runtime() {
    assert_fp_fma_f32(
        "ffma_s",
        OpKind::FFma {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            src3: v(3),
            precision: FpPrecision::F32,
        },
        1.5,
        2.0,
        0.25,
        3.25,
    );
}
#[test]
fn lowers_scalar_fp_fma_f64_runtime() {
    assert_fp_fma_f64(
        "ffma_d",
        OpKind::FFma {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            src3: v(3),
            precision: FpPrecision::F64,
        },
        -3.0,
        2.0,
        0.5,
        -5.5,
    );
}
#[test]
fn rejects_scalar_fp_fma_unsupported_precision() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::FFma {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            src3: v(3),
            precision: FpPrecision::F80,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_scalar_fp_unary_encodings() {
    let words = code_words(&lower_single_op(OpKind::FAbs {
        dst: v(0),
        src: v(1),
        precision: FpPrecision::F32,
    }));
    assert_eq!(words, vec![0x1e20_c020, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FSqrt {
        dst: v(3),
        src: v(4),
        precision: FpPrecision::F64,
    }));
    assert_eq!(words, vec![0x1e61_c083, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FRound {
        dst: v(0),
        src: v(1),
        precision: FpPrecision::F32,
        mode: FpRoundMode::RoundDown,
    }));
    assert_eq!(words, vec![0x1e25_4020, 0xd65f_03c0]);

    let words = code_words(&lower_single_op(OpKind::FRound {
        dst: v(2),
        src: v(3),
        precision: FpPrecision::F64,
        mode: FpRoundMode::RoundTowardZero,
    }));
    assert_eq!(words, vec![0x1e65_c062, 0xd65f_03c0]);
}
#[test]
fn lowers_scalar_fp_unary_f32_runtime() {
    assert_fp_unary_f32(
        "fabs_s",
        OpKind::FAbs {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F32,
        },
        -7.25,
        7.25,
    );
    assert_fp_unary_f32(
        "fneg_s",
        OpKind::FNeg {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F32,
        },
        3.5,
        -3.5,
    );
    assert_fp_unary_f32(
        "fsqrt_s",
        OpKind::FSqrt {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F32,
        },
        9.0,
        3.0,
    );
    assert_fp_unary_f32(
        "frintm_s_round_down",
        OpKind::FRound {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F32,
            mode: FpRoundMode::RoundDown,
        },
        2.5,
        2.0,
    );
    assert_fp_unary_f32(
        "frintn_s_ties_even",
        OpKind::FRound {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F32,
            mode: FpRoundMode::RoundNearest,
        },
        2.5,
        2.0,
    );
}
#[test]
fn lowers_scalar_fp_unary_f64_runtime() {
    assert_fp_unary_f64(
        "fabs_d",
        OpKind::FAbs {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F64,
        },
        -7.25,
        7.25,
    );
    assert_fp_unary_f64(
        "fneg_d",
        OpKind::FNeg {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F64,
        },
        3.5,
        -3.5,
    );
    assert_fp_unary_f64(
        "fsqrt_d",
        OpKind::FSqrt {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F64,
        },
        9.0,
        3.0,
    );
    assert_fp_unary_f64(
        "frintp_d_round_up",
        OpKind::FRound {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F64,
            mode: FpRoundMode::RoundUp,
        },
        -2.5,
        -2.0,
    );
}
#[test]
fn rejects_scalar_fp_unary_unsupported_precision() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::FAbs {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F80,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::FRound {
            dst: v(0),
            src: v(1),
            precision: FpPrecision::F80,
            mode: FpRoundMode::RoundNearest,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_fused_cond_compare_false_compare_as_inverted_ccmp() {
    let cond = VReg::virt(0);
    let cmp_result = VReg::virt(1);
    let cmp_nzcv = VReg::virt(2);
    let final_nzcv = VReg::virt(3);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
    );
    builder.push_op(
        0,
        OpKind::Sub {
            dst: cmp_result,
            src1: x(1),
            src2: SrcOperand::Reg(x(2)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.push_op(
        0,
        OpKind::Mov {
            dst: cmp_nzcv,
            src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::Nzcv))),
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0,
        OpKind::Select {
            dst: final_nzcv,
            cond,
            src_true: VReg::Imm(0x4000_0000),
            src_false: cmp_nzcv,
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
            src: SrcOperand::Reg(final_nzcv),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_condcmp(1, 1, false, 2, 1, 1, 4).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_fused_cond_compare_false_compare_as_inverted_ccmn_imm() {
    let cond = VReg::virt(0);
    let cmp_result = VReg::virt(1);
    let cmp_nzcv = VReg::virt(2);
    let final_nzcv = VReg::virt(3);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Ugt,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: cmp_result,
            src1: x(1),
            src2: SrcOperand::Imm(5),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
    );
    builder.push_op(
        0,
        OpKind::Mov {
            dst: cmp_nzcv,
            src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::Nzcv))),
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0,
        OpKind::Select {
            dst: final_nzcv,
            cond,
            src_true: VReg::Imm(0x9000_0000),
            src_false: cmp_nzcv,
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
            src: SrcOperand::Reg(final_nzcv),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_condcmp(0, 0, true, 5, 9, 1, 9).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_fused_select_true_increment_as_csinc_inverted_cond() {
    let cond = VReg::virt(0);
    let incremented = VReg::virt(1);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: incremented,
            src1: x(2),
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond,
            src_true: incremented,
            src_false: x(1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(1, 0, 1, 1, 2, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_fused_select_true_w_masked_one_increment_as_csinc_inverted_cond() {
    let cond = VReg::virt(0);
    let incremented = VReg::virt(1);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: incremented,
            src1: x(2),
            src2: SrcOperand::Imm64(0x1_0000_0001),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond,
            src_true: incremented,
            src_false: x(1),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(0, 0, 1, 1, 2, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_fused_select_true_invert_as_csinv_inverted_cond() {
    let cond = VReg::virt(0);
    let inverted = VReg::virt(1);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Ne,
        },
    );
    builder.push_op(
        0,
        OpKind::Not {
            dst: inverted,
            src: x(2),
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond,
            src_true: inverted,
            src_false: x(1),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(0, 1, 0, 1, 2, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_fused_select_true_negate_as_csneg_inverted_cond() {
    let cond = VReg::virt(0);
    let negated = VReg::virt(1);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Ugt,
        },
    );
    builder.push_op(
        0,
        OpKind::Neg {
            dst: negated,
            src: x(2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond,
            src_true: negated,
            src_false: x(1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_csel_regs(1, 1, 1, 1, 2, 9, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fp_trampoline_detection_includes_named_fp_state_regs() {
    let fpcr = VReg::Arch(ArchReg::Arm(ArmReg::Fpcr));
    let fpsr = VReg::Arch(ArchReg::Arm(ArmReg::Fpsr));
    let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));

    let read_fpcr = func_with_ops(vec![OpKind::Mov {
        dst: x(0),
        src: SrcOperand::Reg(fpcr),
        width: OpWidth::W64,
    }]);
    assert!(uses_aarch64_fp_trampoline(&read_fpcr));

    let write_fpsr = func_with_ops(vec![OpKind::Mov {
        dst: fpsr,
        src: SrcOperand::Reg(x(1)),
        width: OpWidth::W64,
    }]);
    assert!(uses_aarch64_fp_trampoline(&write_fpsr));

    let read_nzcv = func_with_ops(vec![OpKind::Mov {
        dst: x(0),
        src: SrcOperand::Reg(nzcv),
        width: OpWidth::W64,
    }]);
    assert!(!uses_aarch64_fp_trampoline(&read_nzcv));
}
#[test]
fn fp_trampoline_detection_includes_raw_fp_state_sysregs() {
    let read_fpcr = func_with_ops(vec![OpKind::ReadSysReg {
        dst: x(0),
        reg: SYSREG_FPCR,
    }]);
    assert!(uses_aarch64_fp_trampoline(&read_fpcr));

    let write_fpsr = func_with_ops(vec![OpKind::WriteSysReg {
        reg: SYSREG_FPSR,
        src: x(1),
    }]);
    assert!(uses_aarch64_fp_trampoline(&write_fpsr));

    let read_nzcv = func_with_ops(vec![OpKind::ReadSysReg {
        dst: x(0),
        reg: SYSREG_NZCV,
    }]);
    assert!(!uses_aarch64_fp_trampoline(&read_nzcv));
}
#[test]
fn lowers_double_shift_x_zero_count_as_noop() {
    let cases = [
        OpKind::Shrd {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm(0),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Shld {
            dst: x(0),
            src: x(1),
            amount: SrcOperand::Imm64(64),
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
fn lowers_double_bidir_and_carry_rotate_apx_egpr_operands_runtime() {
    let ops = vec![
        OpKind::Shld {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R17),
            amount: SrcOperand::Reg(x86(X86Reg::R18)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Shrd {
            dst: x86(X86Reg::R19),
            src: x86(X86Reg::R20),
            amount: SrcOperand::Imm(5),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        OpKind::BidirShift {
            dst: x86(X86Reg::R22),
            src: SrcOperand::Reg(x86(X86Reg::R23)),
            amount: SrcOperand::Reg(x86(X86Reg::R24)),
            kind: 2,
            width: OpWidth::W64,
        },
    ];
    let code = lower_ops(ops);
    let regs = [
        (16, 0x1234_5678_9abc_def0),
        (17, 0xfedc_ba98_7654_3210),
        (18, 12),
        (19, 0x1234),
        (20, 0xabcd),
        (23, 0x0123_4567_89ab_cdef),
        (24, 0xffff_ffff_ffff_fffa),
    ];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(
        out[16],
        ref_double_shift_reg(
            0x1234_5678_9abc_def0,
            0xfedc_ba98_7654_3210,
            12,
            true,
            OpWidth::W64,
        )
    );
    assert_eq!(
        out[19] & width_mask(OpWidth::W16),
        ref_double_shift_imm(0x1234, 0xabcd, 5, false, OpWidth::W16)
    );
    assert_eq!(
        out[22],
        ref_bidir_shift(
            0x0123_4567_89ab_cdef,
            0xffff_ffff_ffff_fffa,
            2,
            OpWidth::W64
        )
    );
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(out[17], 0xfedc_ba98_7654_3210);
    assert_eq!(out[18], 12);
    assert_eq!(out[20], 0xabcd);
    assert_eq!(out[23], 0x0123_4567_89ab_cdef);
    assert_eq!(out[24], 0xffff_ffff_ffff_fffa);
    assert_eq!(sp, 0x8000);

    let rcl_code = lower_single_op(OpKind::Rcl {
        dst: x86(X86Reg::R16),
        src: x86(X86Reg::R17),
        amount: SrcOperand::Reg(x86(X86Reg::R18)),
        width: OpWidth::W8,
        flags: FlagUpdate::All,
    });
    let old_nzcv = 0b0010;
    let (expected, carry, effective) = ref_rotate_carry(0x81, 2, true, OpWidth::W8, false);
    let (out, out_nzcv, sp) = run_aarch64_code(
        &rcl_code,
        &[(17, 0x81), (18, 2), (19, 0x1919_1919_1919_1919)],
        old_nzcv,
    );
    assert_eq!(out[16] & width_mask(OpWidth::W8), expected);
    assert_eq!(
        out_nzcv,
        expected_rotate_carry_nzcv(
            old_nzcv,
            expected,
            carry,
            effective,
            OpWidth::W8,
            FlagUpdate::All,
            false,
        )
    );
    assert_eq!(out[17], 0x81);
    assert_eq!(out[18], 2);
    assert_eq!(out[19], 0x1919_1919_1919_1919);
    assert_eq!(sp, 0x8000);

    let rcr_code = lower_single_op(OpKind::Rcr {
        dst: x86(X86Reg::R20),
        src: x86(X86Reg::R21),
        amount: SrcOperand::Reg(x86(X86Reg::R22)),
        width: OpWidth::W16,
        flags: FlagUpdate::All,
    });
    let old_nzcv = 0b0000;
    let (expected, carry, effective) = ref_rotate_carry(0x8001, 1, false, OpWidth::W16, true);
    let (out, out_nzcv, sp) = run_aarch64_code(&rcr_code, &[(21, 0x8001), (22, 1)], old_nzcv);
    assert_eq!(out[20] & width_mask(OpWidth::W16), expected);
    assert_eq!(
        out_nzcv,
        expected_rotate_carry_nzcv(
            old_nzcv,
            expected,
            carry,
            effective,
            OpWidth::W16,
            FlagUpdate::All,
            true,
        )
    );
    assert_eq!(out[21], 0x8001);
    assert_eq!(out[22], 1);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_double_shift_register_amount() {
    assert_double_shift_reg_lowering(
        "shld_x_reg",
        true,
        0,
        0x1234_5678_9abc_def0,
        1,
        0xfedc_ba98_7654_3210,
        2,
        4,
        OpWidth::W64,
    );
    assert_double_shift_reg_lowering(
        "shld_x_masked_zero",
        true,
        0,
        0x1234_5678_9abc_def0,
        1,
        0xfedc_ba98_7654_3210,
        2,
        64,
        OpWidth::W64,
    );
    assert_double_shift_reg_lowering(
        "shrd_x_dst_aliases_count",
        false,
        2,
        4,
        1,
        0x8000_0000_0000_0001,
        2,
        4,
        OpWidth::W64,
    );
    assert_double_shift_reg_lowering(
        "shld_w_reg_masked_count",
        true,
        0,
        0x8000_0001,
        1,
        0x1234_5678,
        2,
        36,
        OpWidth::W32,
    );
    assert_double_shift_reg_lowering(
        "shrd_w_dst_aliases_src_and_count",
        false,
        1,
        4,
        1,
        4,
        1,
        4,
        OpWidth::W32,
    );
    assert_double_shift_reg_lowering(
        "shld_w16_reg_count_greater_than_width",
        true,
        0,
        0x1234,
        1,
        0xabcd,
        2,
        17,
        OpWidth::W16,
    );
    assert_double_shift_reg_lowering(
        "shrd_w8_reg_count_greater_than_width",
        false,
        0,
        0x12,
        1,
        0xab,
        2,
        9,
        OpWidth::W8,
    );
    assert_double_shift_reg_lowering(
        "shld_w8_reg_nonzero_count",
        true,
        0,
        0x12,
        1,
        0xab,
        2,
        3,
        OpWidth::W8,
    );
    assert_double_shift_reg_lowering(
        "shrd_w16_dst_aliases_count",
        false,
        2,
        5,
        1,
        0xabcd,
        2,
        5,
        OpWidth::W16,
    );
}
#[test]
fn lowers_x86_ndd_double_shift_alias_width_direction_and_count_matrix() {
    let initial = |reg: u8| match reg {
        0 => 0xaaaa_bbbb_cccc_8123,
        1 => 0x1111_2222_3333_0005,
        3 => 0xbbbb_cccc_dddd_5aa5,
        8 => 0x8888_7777_6666_2468,
        _ => unreachable!("unexpected test register x{reg}"),
    };
    let sentinels = [
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
        (15, 0x1515_1515_1515_1515),
        (14, 0x1414_1414_1414_1414),
    ];
    let aliases = [
        ("distinct", 8, 0, 3, false),
        ("dst-fill", 3, 0, 3, false),
        ("dst-base", 0, 0, 3, false),
        ("dst-count", 1, 0, 3, true),
        ("fill-count", 8, 0, 1, true),
        ("base-count", 8, 1, 3, true),
    ];

    for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        for left in [false, true] {
            for (alias, dst, base, fill, register_count) in aliases {
                let amount = if register_count {
                    SrcOperand::Reg(x86(X86Reg::Rcx))
                } else {
                    SrcOperand::Imm(4)
                };
                let code = lower_single_op(OpKind::X86NddDoubleShift {
                    dst: x(dst),
                    base: x(base),
                    fill: x(fill),
                    amount,
                    width,
                    left,
                    flags: FlagUpdate::None,
                });
                let expected_low = if register_count {
                    ref_double_shift_reg(initial(base), initial(fill), initial(1), left, width)
                } else {
                    ref_double_shift_imm(initial(base), initial(fill), 4, left, width)
                };
                let expected = match width {
                    OpWidth::W16 => (initial(dst) & !width_mask(width)) | expected_low,
                    OpWidth::W32 | OpWidth::W64 => expected_low,
                    _ => unreachable!(),
                };
                let mut regs = sentinels.to_vec();
                regs.extend([0, 1, 3, 8].map(|reg| (reg, initial(reg))));
                let old_nzcv = 0b1011;
                let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
                let op = if left { "SHLD" } else { "SHRD" };

                assert_eq!(
                    out[dst as usize], expected,
                    "APX NDD {op} {width:?} {alias} result"
                );
                for reg in [0, 1, 3, 8] {
                    if reg != dst {
                        assert_eq!(
                            out[reg as usize],
                            initial(reg),
                            "APX NDD {op} {width:?} {alias} x{reg} preserved"
                        );
                    }
                }
                for (reg, value) in sentinels {
                    assert_eq!(
                        out[reg as usize], value,
                        "APX NDD {op} {width:?} {alias} x{reg} scratch"
                    );
                }
                assert_eq!(out_nzcv, old_nzcv, "APX NDD {op} {width:?} {alias} NZCV");
                assert_eq!(sp, 0x8000, "APX NDD {op} {width:?} {alias} stack");
            }
        }
    }
}
#[test]
fn lowers_x86_ndd_double_shift_flags_before_destination_commit() {
    let initial = |reg: u8| match reg {
        0 => 0xaaaa_bbbb_cccc_4001,
        1 => 0x1111_2222_3333_0001,
        3 => 0xbbbb_cccc_dddd_8001,
        8 => 0x8888_7777_6666_2468,
        _ => unreachable!("unexpected test register x{reg}"),
    };
    let cases = [
        ("word-fill-alias", OpWidth::W16, true, 3, 0, 3, false),
        ("dword-count-alias", OpWidth::W32, false, 1, 0, 3, true),
        ("qword-register", OpWidth::W64, true, 8, 0, 3, true),
    ];

    for (label, width, left, dst, base, fill, register_count) in cases {
        let amount_value = 1;
        let amount = if register_count {
            SrcOperand::Reg(x86(X86Reg::Rcx))
        } else {
            SrcOperand::Imm(amount_value as i64)
        };
        let code = lower_single_op(OpKind::X86NddDoubleShift {
            dst: x(dst),
            base: x(base),
            fill: x(fill),
            amount,
            width,
            left,
            flags: FlagUpdate::All,
        });
        let expected_low =
            ref_double_shift_flags_value(initial(base), initial(fill), amount_value, left, width);
        let expected = if width == OpWidth::W16 {
            (initial(dst) & !width_mask(width)) | expected_low
        } else {
            expected_low
        };
        let old_nzcv = 0b1101;
        let expected_nzcv = expected_double_shift_nzcv(
            old_nzcv,
            initial(base),
            expected_low,
            amount_value,
            left,
            width,
            FlagUpdate::All,
        );
        let (out, out_nzcv, sp) = run_aarch64_code(
            &code,
            &[
                (0, initial(0)),
                (1, initial(1)),
                (3, initial(3)),
                (8, initial(8)),
                (16, 0x1616_1616_1616_1616),
                (17, 0x1717_1717_1717_1717),
            ],
            old_nzcv,
        );

        assert_eq!(out[dst as usize], expected, "{label} result");
        assert_eq!(out_nzcv, expected_nzcv, "{label} NZCV");
        assert_eq!(out[16], 0x1616_1616_1616_1616, "{label} x16 scratch");
        assert_eq!(out[17], 0x1717_1717_1717_1717, "{label} x17 scratch");
        assert_eq!(sp, 0x8000, "{label} stack");
    }
}
#[test]
fn rejects_unsupported_x86_ndd_double_shift_shapes() {
    for op in [
        OpKind::X86NddDoubleShift {
            dst: x(0),
            base: x(1),
            fill: x(2),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8,
            left: true,
            flags: FlagUpdate::None,
        },
        OpKind::X86NddDoubleShift {
            dst: x(0),
            base: x(1),
            fill: x(2),
            amount: SrcOperand::Reg(x(3)),
            width: OpWidth::W16,
            left: false,
            flags: FlagUpdate::All,
        },
        OpKind::X86NddDoubleShift {
            dst: x(0),
            base: x(1),
            fill: x(2),
            amount: SrcOperand::Reg(x(2)),
            width: OpWidth::W64,
            left: false,
            flags: FlagUpdate::None,
        },
        OpKind::X86NddDoubleShift {
            dst: x(0),
            base: x(1),
            fill: x(2),
            amount: SrcOperand::Imm64(1),
            width: OpWidth::W64,
            left: true,
            flags: FlagUpdate::None,
        },
    ] {
        let error = try_lower_single_op(op).expect_err("unsupported APX NDD double shift");
        assert!(error.to_string().contains("APX NDD double shift"));
    }
}
#[test]
fn lowers_x86_w16_destructive_double_shift_partial_write_alias_matrix() {
    let reg = |index: u8| match index {
        0 => x86(X86Reg::Rax),
        1 => x86(X86Reg::Rcx),
        2 => x86(X86Reg::Rdx),
        3 => x86(X86Reg::Rbx),
        _ => unreachable!("unexpected test register x{index}"),
    };
    let initial = |index: u8| match index {
        0 => 0xaaaa_bbbb_cccc_8123,
        1 => 0x1111_2222_3333_0005,
        2 => 0xdddd_eeee_ffff_abcd,
        3 => 0xbbbb_cccc_dddd_5aa5,
        _ => unreachable!("unexpected test register x{index}"),
    };
    let aliases = [
        ("distinct-imm", 0, 3, false),
        ("distinct-cl", 0, 3, true),
        ("dst-src-imm", 3, 3, false),
        ("dst-src-cl", 3, 3, true),
        ("dst-count", 1, 2, true),
        ("src-count", 0, 1, true),
    ];
    let sentinels = [
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
        (15, 0x1515_1515_1515_1515),
        (14, 0x1414_1414_1414_1414),
    ];

    for left in [false, true] {
        for (alias, dst, src, register_count) in aliases {
            let amount = if register_count {
                SrcOperand::Reg(x86(X86Reg::Rcx))
            } else {
                SrcOperand::Imm(4)
            };
            let kind = if left {
                OpKind::Shld {
                    dst: reg(dst),
                    src: reg(src),
                    amount,
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                }
            } else {
                OpKind::Shrd {
                    dst: reg(dst),
                    src: reg(src),
                    amount,
                    width: OpWidth::W16,
                    flags: FlagUpdate::None,
                }
            };
            let code = lower_single_op(kind);
            let expected_low = if register_count {
                ref_double_shift_reg(initial(dst), initial(src), initial(1), left, OpWidth::W16)
            } else {
                ref_double_shift_imm(initial(dst), initial(src), 4, left, OpWidth::W16)
            };
            let expected = (initial(dst) & !0xffff) | expected_low;
            let mut regs = sentinels.to_vec();
            regs.extend([0, 1, 2, 3].map(|index| (index, initial(index))));
            let old_nzcv = 0b1011;
            let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
            let op = if left { "SHLD" } else { "SHRD" };

            assert_eq!(out[dst as usize], expected, "x86 {op} W16 {alias} result");
            for index in [0, 1, 2, 3] {
                if index != dst {
                    assert_eq!(
                        out[index as usize],
                        initial(index),
                        "x86 {op} W16 {alias} x{index} preserved"
                    );
                }
            }
            for (index, value) in sentinels {
                assert_eq!(
                    out[index as usize], value,
                    "x86 {op} W16 {alias} x{index} scratch"
                );
            }
            assert_eq!(out_nzcv, old_nzcv, "x86 {op} W16 {alias} NZCV");
            assert_eq!(sp, 0x8000, "x86 {op} W16 {alias} stack");
        }
    }
}
#[test]
fn lowers_x86_w16_destructive_double_shift_flags_before_partial_merge() {
    let dst = 0xaaaa_bbbb_cccc_8001;
    let src = 0xbbbb_cccc_dddd_5aa5;
    for left in [false, true] {
        for amount in [1_u64, 16] {
            let kind = if left {
                OpKind::Shld {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rbx),
                    amount: SrcOperand::Imm(amount as i64),
                    width: OpWidth::W16,
                    flags: FlagUpdate::All,
                }
            } else {
                OpKind::Shrd {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rbx),
                    amount: SrcOperand::Imm(amount as i64),
                    width: OpWidth::W16,
                    flags: FlagUpdate::All,
                }
            };
            let code = lower_single_op(kind);
            let expected_low = ref_double_shift_flags_value(dst, src, amount, left, OpWidth::W16);
            let old_nzcv = 0b1101;
            let expected_nzcv = expected_double_shift_nzcv(
                old_nzcv,
                dst,
                expected_low,
                amount,
                left,
                OpWidth::W16,
                FlagUpdate::All,
            );
            let (out, out_nzcv, sp) = run_aarch64_code(
                &code,
                &[
                    (0, dst),
                    (3, src),
                    (16, 0x1616_1616_1616_1616),
                    (17, 0x1717_1717_1717_1717),
                ],
                old_nzcv,
            );
            let op = if left { "SHLD" } else { "SHRD" };

            assert_eq!(
                out[0],
                (dst & !0xffff) | expected_low,
                "x86 {op} W16 count {amount} result"
            );
            assert_eq!(out[3], src, "x86 {op} W16 count {amount} source");
            assert_eq!(out_nzcv, expected_nzcv, "x86 {op} W16 count {amount} NZCV");
            assert_eq!(out[16], 0x1616_1616_1616_1616, "x86 {op} x16");
            assert_eq!(out[17], 0x1717_1717_1717_1717, "x86 {op} x17");
            assert_eq!(sp, 0x8000, "x86 {op} W16 count {amount} stack");
        }
    }
}
#[test]
fn rejects_unrepresentable_x86_w16_destructive_double_shift_flags() {
    for (op, expected) in [
        (
            OpKind::Shld {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rbx),
                amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
            "flag-setting register-count double shift width W16",
        ),
        (
            OpKind::Shrd {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rbx),
                amount: SrcOperand::Imm(17),
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
            "count greater than width",
        ),
    ] {
        let error = try_lower_single_op(op).expect_err("unrepresentable W16 flags");
        assert!(
            error.to_string().contains(expected),
            "unexpected lowerer error: {error}"
        );
    }
}
#[test]
fn lowers_flag_setting_double_shift_runtime() {
    assert_double_shift_flags_lowering(
        "shrd_x_imm1_flags",
        false,
        0,
        0x8000_0000_0000_0001,
        1,
        0,
        None,
        1,
        OpWidth::W64,
        0b1100,
    );
    assert_double_shift_flags_lowering(
        "shld_x_imm1_flags",
        true,
        0,
        0x4000_0000_0000_0000,
        1,
        0,
        None,
        1,
        OpWidth::W64,
        0b0110,
    );
    assert_double_shift_flags_lowering(
        "shrd_w_reg4_flags",
        false,
        0,
        0x8000_0001,
        1,
        0xfedc_ba98,
        Some(2),
        4,
        OpWidth::W32,
        0b0011,
    );
    assert_double_shift_flags_lowering(
        "shld_w_reg32_preserves_flags",
        true,
        0,
        0x1234_5678,
        1,
        0xfedc_ba98,
        Some(2),
        32,
        OpWidth::W32,
        0b0111,
    );
    assert_double_shift_flags_lowering(
        "shld_w_reg33_overflow",
        true,
        0,
        0x4000_0000,
        1,
        0,
        Some(2),
        33,
        OpWidth::W32,
        0b0100,
    );
    assert_double_shift_flags_lowering(
        "shld_w16_imm16_flags",
        true,
        0,
        0x8001,
        1,
        0x1234,
        None,
        16,
        OpWidth::W16,
        0b1001,
    );
    assert_double_shift_flags_lowering(
        "shrd_w8_imm8_flags",
        false,
        0,
        0x80,
        1,
        0x5a,
        None,
        8,
        OpWidth::W8,
        0b0101,
    );
    assert_double_shift_flags_lowering(
        "shrd_x_dst_aliases_count_flags",
        false,
        2,
        4,
        1,
        0x8000_0000_0000_0001,
        Some(2),
        4,
        OpWidth::W64,
        0b1010,
    );
}
#[test]
fn lowers_subword_double_shift_count_greater_than_width_as_base() {
    assert_double_shift_imm_lowering(
        "shld_w16_count_greater_than_width",
        true,
        0,
        0x1234,
        1,
        0xabcd,
        17,
        OpWidth::W16,
    );
    assert_double_shift_imm_lowering(
        "shrd_w16_count_greater_than_width",
        false,
        0,
        0x1234,
        1,
        0xabcd,
        17,
        OpWidth::W16,
    );
    assert_double_shift_imm_lowering(
        "shld_w8_count_greater_than_width",
        true,
        0,
        0x12,
        1,
        0xab,
        9,
        OpWidth::W8,
    );
    assert_double_shift_imm_lowering(
        "shrd_w8_count_greater_than_width",
        false,
        0,
        0x12,
        1,
        0xab,
        9,
        OpWidth::W8,
    );
}
#[test]
fn lowers_subword_double_shift_aliased_nonzero_count() {
    assert_double_shift_imm_lowering(
        "shld_w16_aliased_nonzero_count",
        true,
        0,
        0x1234,
        0,
        0x1234,
        1,
        OpWidth::W16,
    );
    assert_double_shift_imm_lowering(
        "shld_w8_aliased_nonzero_count",
        true,
        0,
        0x81,
        0,
        0x81,
        1,
        OpWidth::W8,
    );
    assert_double_shift_imm_lowering(
        "shrd_w8_aliased_nonzero_count",
        false,
        0,
        0x81,
        0,
        0x81,
        1,
        OpWidth::W8,
    );
    assert_double_shift_imm_lowering(
        "shrd_w16_aliased_nonzero_count",
        false,
        0,
        0x8001,
        0,
        0x8001,
        1,
        OpWidth::W16,
    );
}
#[test]
fn lowers_fused_cond_compare_with_large_immediate_source_runtime() {
    assert_cond_compare_imm_lowering(
        "ccmp_x_large_imm_true",
        true,
        Condition::Eq,
        1,
        0x1234,
        SrcOperand::Imm64(0x40),
        0x40,
        OpWidth::W64,
        0b0100,
        0b0010,
    );
    assert_cond_compare_imm_lowering(
        "ccmp_x_large_imm_false_uses_fallback",
        true,
        Condition::Eq,
        1,
        0x1234,
        SrcOperand::Imm64(0x40),
        0x40,
        OpWidth::W64,
        0b0000,
        0b1010,
    );
    assert_cond_compare_imm_lowering(
        "ccmn_w_large_imm_masks_operand",
        false,
        Condition::Ne,
        1,
        0xffff_fffe,
        SrcOperand::Imm64(0x1_0000_0003),
        0x1_0000_0003,
        OpWidth::W32,
        0b0000,
        0b1111,
    );
    assert_cond_compare_imm_lowering(
        "ccmp_w_negative_imm_avoids_source_scratch",
        true,
        Condition::Always,
        16,
        0,
        SrcOperand::Imm64(-1),
        u64::MAX,
        OpWidth::W32,
        0b1010,
        0b0000,
    );
}
#[test]
fn lowers_fused_cond_compare_apx_egpr_operands_runtime() {
    let cond_vreg = VReg::virt(0);
    let cmp_nzcv = VReg::virt(2);
    let final_nzcv = VReg::virt(3);
    let code = lower_ops(vec![
        OpKind::TestCondition {
            dst: cond_vreg,
            cond: Condition::Eq,
        },
        OpKind::Sub {
            dst: VReg::virt(1),
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R17)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        OpKind::Mov {
            dst: cmp_nzcv,
            src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::Nzcv))),
            width: OpWidth::W32,
        },
        OpKind::Select {
            dst: final_nzcv,
            cond: cond_vreg,
            src_true: cmp_nzcv,
            src_false: VReg::Imm(i64::from(0b1010) << 28),
            width: OpWidth::W32,
        },
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
            src: SrcOperand::Reg(final_nzcv),
            width: OpWidth::W32,
        },
    ]);
    let cond_compare = find_cond_compare_word(&code).expect("expected fused conditional compare");
    assert_eq!(cond_compare >> 31, 1);
    assert_eq!((cond_compare >> 30) & 1, 1);
    assert_eq!((cond_compare >> 16) & 0x1f, 17);
    assert_eq!((cond_compare >> 11) & 1, 0);
    assert_eq!((cond_compare >> 5) & 0x1f, 16);

    let sentinel = 0x1818_1818_1818_1818;
    let (out, out_nzcv, sp) =
        run_aarch64_code(&code, &[(16, 0x20), (17, 0x20), (18, sentinel)], 0b0100);
    assert_eq!(
        out_nzcv,
        expected_addsub_nzcv(0x20, 0x20, true, OpWidth::W64)
    );
    assert_eq!(out[16], 0x20);
    assert_eq!(out[17], 0x20);
    assert_eq!(out[18], sentinel);
    assert_eq!(sp, 0x8000);

    let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(16, 0x20), (17, 0x21), (18, sentinel)], 0);
    assert_eq!(out_nzcv, 0b1010);
    assert_eq!(out[16], 0x20);
    assert_eq!(out[17], 0x21);
    assert_eq!(out[18], sentinel);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_fused_apx_r31_identity_mapping() {
    let lo = VReg::virt(0);
    let hi = VReg::virt(1);
    let err = try_lower_ops(vec![
        OpKind::Shr {
            dst: lo,
            src: x86(X86Reg::R31),
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
    ])
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));

    let cond_vreg = VReg::virt(0);
    let cmp_nzcv = VReg::virt(2);
    let final_nzcv = VReg::virt(3);
    let err = try_lower_ops(vec![
        OpKind::TestCondition {
            dst: cond_vreg,
            cond: Condition::Eq,
        },
        OpKind::Sub {
            dst: VReg::virt(1),
            src1: x86(X86Reg::R16),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        OpKind::Mov {
            dst: cmp_nzcv,
            src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::Nzcv))),
            width: OpWidth::W32,
        },
        OpKind::Select {
            dst: final_nzcv,
            cond: cond_vreg,
            src_true: cmp_nzcv,
            src_false: VReg::Imm(i64::from(0b1010) << 28),
            width: OpWidth::W32,
        },
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
            src: SrcOperand::Reg(final_nzcv),
            width: OpWidth::W32,
        },
    ])
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
