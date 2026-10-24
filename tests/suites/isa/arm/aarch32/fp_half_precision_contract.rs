//! Precision conversion observes AHP and ignores FZ16.
use rax::isa::arm::aarch32::vfp::{self, Fpscr};
fn check(raw: u32, control: u32, want: u16, status: u32) {
    let initial = control | 0xa8000000;
    let mut f = Fpscr::from_bits(initial);
    assert_eq!(
        (
            vfp::vcvt_f16_bits_f32(f32::from_bits(raw), &mut f),
            f.bits()
        ),
        (want, initial | status)
    );
}
macro_rules! conversion {
    ($name:ident,$raw:expr,$control:expr,$want:expr,$status:expr) => {
        #[test]
        fn $name() {
            check($raw, $control, $want, $status);
        }
    };
}
conversion!(
    exact_half_subnormal_does_not_raise_underflow,
    0x33800000,
    0,
    1,
    0
);
conversion!(
    half_subnormal_output_ignores_fz16,
    0x33800000,
    1 << 19,
    1,
    0
);
conversion!(halfway_underflow_reports_both_flags, 0x33000000, 0, 0, 0x18);
conversion!(
    tiny_round_up_reaches_minimum_half_subnormal,
    1,
    1 << 22,
    1,
    0x18
);
conversion!(
    negative_tiny_round_down_preserves_sign,
    0x80000001,
    2 << 22,
    0x8001,
    0x18
);
conversion!(
    single_input_flush_reports_only_input_denormal,
    1,
    1 << 24,
    0,
    0x80
);
conversion!(halfway_normal_sets_inexact, 0x3f801000, 0, 0x3c00, 0x10);
conversion!(
    halfway_normal_round_up_advances,
    0x3f801000,
    1 << 22,
    0x3c01,
    0x10
);
conversion!(
    overflow_round_zero_saturates_finite,
    0x47800000,
    3 << 22,
    0x7bff,
    0x14
);
conversion!(
    negative_overflow_round_up_saturates_finite,
    0xc7800000,
    1 << 22,
    0xfbff,
    0x14
);
conversion!(
    quiet_nan_payload_does_not_raise_invalid,
    0x7fc12345,
    0,
    0x7e09,
    0
);
conversion!(
    signaling_nan_quiets_and_raises_invalid,
    0x7f812345,
    0,
    0x7e09,
    1
);
conversion!(
    default_nan_discards_sign_and_payload,
    0xffc12345,
    1 << 25,
    0x7e00,
    0
);
conversion!(
    alternative_exponent_31_is_finite,
    0x47800000,
    1 << 26,
    0x7c00,
    0
);
conversion!(
    alternative_positive_infinity_saturates_invalid,
    0x7f800000,
    1 << 26,
    0x7fff,
    1
);
conversion!(
    alternative_negative_infinity_saturates_invalid,
    0xff800000,
    1 << 26,
    0xffff,
    1
);
conversion!(
    alternative_quiet_nan_is_signed_zero_invalid,
    0xffc12345,
    1 << 26,
    0x8000,
    1
);
conversion!(
    alternative_nan_sign_survives_default_nan_control,
    0xffc12345,
    (1 << 26) | (1 << 25),
    0x8000,
    1
);
conversion!(
    alternative_finite_overflow_suppresses_inexact,
    0x48000000,
    1 << 26,
    0x7fff,
    1
);
conversion!(
    alternative_halfway_carry_suppresses_inexact,
    0x47fff000,
    1 << 26,
    0x7fff,
    1
);
conversion!(
    alternative_round_zero_without_carry_is_inexact,
    0x47fff000,
    (1 << 26) | (3 << 22),
    0x7fff,
    0x10
);
conversion!(
    negative_zero_survives_all_profiles,
    0x80000000,
    (1 << 26) | (1 << 19) | (1 << 24),
    0x8000,
    0
);
