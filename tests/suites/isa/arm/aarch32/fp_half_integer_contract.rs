//! Half-to-integer exact boundaries and architectural cumulative status.
use rax::isa::arm::aarch32::vfp::{self, Fpscr, RoundingMode};
#[test]
fn signed_i16_minimum_is_exact_and_valid() {
    let mut f = Fpscr::default();
    assert_eq!(
        vfp::vcvt_s16_f16_bits_round(0xf800, RoundingMode::RoundNearest, &mut f),
        i16::MIN
    );
    assert_eq!(f.bits(), 0);
}
#[test]
fn signed_i16_positive_overflow_saturates_without_inexact() {
    let mut f = Fpscr::default();
    assert_eq!(
        vfp::vcvt_s16_f16_bits_round(0x7800, RoundingMode::RoundNearest, &mut f),
        i16::MAX
    );
    assert_eq!(f.bits(), 1);
}
#[test]
fn unsigned_negative_rounds_to_zero_with_inexact_and_without_invalid() {
    let mut f = Fpscr::default();
    assert_eq!(
        vfp::vcvt_u16_f16_bits_round(0xb400, RoundingMode::RoundNearest, &mut f),
        0
    );
    assert_eq!(f.bits(), 0x10);
}
#[test]
fn unsigned_negative_round_down_saturates_without_inexact() {
    let mut f = Fpscr::default();
    assert_eq!(
        vfp::vcvt_u16_f16_bits_round(0xb400, RoundingMode::RoundMinusInf, &mut f),
        0
    );
    assert_eq!(f.bits(), 1);
}
#[test]
fn flushed_half_input_does_not_raise_idc_or_inexact() {
    let mut f = Fpscr::from_bits(1 << 19);
    assert_eq!(
        vfp::vcvt_s32_f16_bits_round(1, RoundingMode::RoundPlusInf, &mut f),
        0
    );
    assert_eq!(f.bits(), 1 << 19);
}
#[test]
fn fz32_and_ahp_do_not_change_half_integer_data_processing() {
    let initial = (1 << 24) | (1 << 26);
    let mut f = Fpscr::from_bits(initial);
    assert_eq!(
        vfp::vcvt_s32_f16_bits_round(1, RoundingMode::RoundPlusInf, &mut f),
        1
    );
    assert_eq!(f.bits(), initial | 0x10);
}
#[test]
fn quiet_nan_is_invalid_even_with_default_nan_control() {
    let initial = 1 << 25;
    let mut f = Fpscr::from_bits(initial);
    assert_eq!(
        vfp::vcvt_u32_f16_bits_round(0x7e01, RoundingMode::RoundZero, &mut f),
        0
    );
    assert_eq!(f.bits(), initial | 1);
}
#[test]
fn explicit_ties_away_retains_live_controls_and_existing_status() {
    let initial = 0xa8000081 | (3 << 22) | (1 << 26);
    let mut f = Fpscr::from_bits(initial);
    assert_eq!(
        vfp::vcvt_s32_f16_bits_round(0xc100, RoundingMode::RoundTiesAway, &mut f),
        -3
    );
    assert_eq!(f.bits(), initial | 0x10);
}
