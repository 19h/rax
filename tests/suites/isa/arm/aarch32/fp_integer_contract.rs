//! FPToFixed status and representability boundaries for implemented 32-bit results.
use rax::isa::arm::aarch32::vfp::{self, Fpscr, RoundingMode};

fn check(
    input: u64,
    double: bool,
    signed: bool,
    fractional_bits: u32,
    mode: RoundingMode,
    control: u32,
    expected: u32,
    status: u32,
) {
    let initial = control | 0xa000_0004;
    let mut fpscr = Fpscr::from_bits(initial);
    let result = match (double, signed, fractional_bits) {
        (true, true, 0) => vfp::vcvt_s32_f64_round(f64::from_bits(input), mode, &mut fpscr) as u32,
        (true, false, 0) => vfp::vcvt_u32_f64_round(f64::from_bits(input), mode, &mut fpscr),
        (false, true, 0) => {
            vfp::vcvt_s32_f32_round(f32::from_bits(input as u32), mode, &mut fpscr) as u32
        }
        (false, false, 0) => {
            vfp::vcvt_u32_f32_round(f32::from_bits(input as u32), mode, &mut fpscr)
        }
        (true, true, bits) => {
            vfp::vcvt_s32_f64_fixed(f64::from_bits(input), bits, &mut fpscr) as u32
        }
        (true, false, bits) => vfp::vcvt_u32_f64_fixed(f64::from_bits(input), bits, &mut fpscr),
        (false, true, bits) => {
            vfp::vcvt_s32_f32_fixed(f32::from_bits(input as u32), bits, &mut fpscr) as u32
        }
        (false, false, bits) => {
            vfp::vcvt_u32_f32_fixed(f32::from_bits(input as u32), bits, &mut fpscr)
        }
    };
    assert_eq!(result, expected, "input={input:016x}");
    assert_eq!(fpscr.bits(), initial | status, "input={input:016x}");
}

macro_rules! conversion_case {
    ($name:ident, $raw:expr, $double:expr, $signed:expr, $fbits:expr, $mode:ident, $control:expr, $expected:expr, $status:expr) => {
        #[test]
        fn $name() {
            check(
                $raw,
                $double,
                $signed,
                $fbits,
                RoundingMode::$mode,
                $control,
                $expected,
                $status,
            );
        }
    };
}

conversion_case!(
    exact_signed_minimum_f64_is_valid,
    (-2147483648.0_f64).to_bits(),
    true,
    true,
    0,
    RoundZero,
    0,
    0x80000000,
    0
);
conversion_case!(
    exact_signed_maximum_f64_is_valid,
    2147483647.0_f64.to_bits(),
    true,
    true,
    0,
    RoundZero,
    0,
    0x7fffffff,
    0
);
conversion_case!(
    exact_unsigned_maximum_f64_is_valid,
    4294967295.0_f64.to_bits(),
    true,
    false,
    0,
    RoundZero,
    0,
    0xffffffff,
    0
);
conversion_case!(
    exact_signed_minimum_f32_is_valid,
    u64::from((-2147483648.0_f32).to_bits()),
    false,
    true,
    0,
    RoundZero,
    0,
    0x80000000,
    0
);
conversion_case!(
    positive_fraction_sets_inexact,
    1.25_f64.to_bits(),
    true,
    true,
    0,
    RoundZero,
    0,
    1,
    0x10
);
conversion_case!(
    negative_fraction_to_unsigned_zero_sets_inexact,
    (-0.25_f64).to_bits(),
    true,
    false,
    0,
    RoundZero,
    0,
    0,
    0x10
);
conversion_case!(
    signed_upper_fraction_truncates_validly,
    2147483647.75_f64.to_bits(),
    true,
    true,
    0,
    RoundZero,
    0,
    0x7fffffff,
    0x10
);
conversion_case!(
    signed_upper_fraction_nearest_saturates_without_inexact,
    2147483647.75_f64.to_bits(),
    true,
    true,
    0,
    RoundNearest,
    0,
    0x7fffffff,
    1
);
conversion_case!(
    ties_even_positive_half,
    2.5_f64.to_bits(),
    true,
    true,
    0,
    RoundNearest,
    0,
    2,
    0x10
);
conversion_case!(
    ties_away_positive_half,
    2.5_f64.to_bits(),
    true,
    true,
    0,
    RoundTiesAway,
    0,
    3,
    0x10
);
conversion_case!(
    ties_away_negative_half,
    (-2.5_f64).to_bits(),
    true,
    true,
    0,
    RoundTiesAway,
    0,
    (-3_i32) as u32,
    0x10
);
conversion_case!(
    unsigned_negative_round_down_is_invalid,
    (-0.25_f64).to_bits(),
    true,
    false,
    0,
    RoundMinusInf,
    0,
    0,
    1
);
conversion_case!(
    positive_subnormal_without_flush_is_inexact,
    1,
    false,
    true,
    0,
    RoundZero,
    0,
    0,
    0x10
);
conversion_case!(
    positive_subnormal_round_up_produces_one,
    1,
    false,
    true,
    0,
    RoundPlusInf,
    0,
    1,
    0x10
);
conversion_case!(
    negative_subnormal_round_down_produces_minus_one,
    0x80000001,
    false,
    true,
    0,
    RoundMinusInf,
    0,
    0xffffffff,
    0x10
);
conversion_case!(
    positive_subnormal_flush_sets_input_denormal_only,
    1,
    false,
    true,
    0,
    RoundPlusInf,
    1 << 24,
    0,
    0x80
);
conversion_case!(
    negative_subnormal_flush_sets_input_denormal_only,
    0x8000000000000001,
    true,
    false,
    0,
    RoundMinusInf,
    1 << 24,
    0,
    0x80
);
conversion_case!(
    quiet_nan_is_invalid_zero,
    0x7ff8000000001234,
    true,
    true,
    0,
    RoundZero,
    0,
    0,
    1
);
conversion_case!(
    signaling_nan_is_invalid_zero,
    0x7ff0000000001234,
    true,
    false,
    0,
    RoundZero,
    0,
    0,
    1
);
conversion_case!(
    positive_infinity_saturates_signed,
    0x7ff0000000000000,
    true,
    true,
    0,
    RoundZero,
    0,
    0x7fffffff,
    1
);
conversion_case!(
    negative_infinity_saturates_signed,
    0xfff0000000000000,
    true,
    true,
    0,
    RoundZero,
    0,
    0x80000000,
    1
);
conversion_case!(
    exact_fixed_scale_has_no_exception,
    0.5_f64.to_bits(),
    true,
    true,
    1,
    RoundZero,
    0,
    1,
    0
);
conversion_case!(
    fixed_scale_inexact_sets_ixc,
    0.375_f64.to_bits(),
    true,
    true,
    1,
    RoundZero,
    0,
    0,
    0x10
);
conversion_case!(
    fixed_scale_overflow_sets_ioc_without_ixc,
    1.0_f64.to_bits(),
    true,
    false,
    32,
    RoundZero,
    0,
    0xffffffff,
    1
);
conversion_case!(
    fixed_scale_flush_precedes_scaling,
    1,
    false,
    true,
    32,
    RoundZero,
    1 << 24,
    0,
    0x80
);
conversion_case!(
    explicit_rounding_ignores_fpscr_rounding_mode,
    (-1.25_f64).to_bits(),
    true,
    true,
    0,
    RoundZero,
    2 << 22,
    0xffffffff,
    0x10
);
