//! Mixed-precision operation boundaries, NaN processing and status.
use rax::isa::arm::aarch32::vfp::{self, Fpscr};
fn check(acc: u32, a: u16, b: u16, subtract: bool, control: u32, want: u32, status: u32) {
    let initial = control | 0xa8000000;
    let mut f = Fpscr::from_bits(initial);
    let result = if subtract {
        vfp::vfmsl_f32_f16_bits(acc, a, b, &mut f)
    } else {
        vfp::vfmal_f32_f16_bits(acc, a, b, &mut f)
    };
    assert_eq!((result, f.bits()), (want, initial | status));
}
#[test]
fn finite_mixed_fused_operation_is_exact() {
    check(0x3f800000, 0x3c00, 0x4000, false, 0, 0x40400000, 0);
}
#[test]
fn halfway_single_result_rounds_once_and_sets_inexact() {
    check(0x3f800000, 1, 0x3c00, false, 0, 0x3f800000, 0x10);
}
#[test]
fn half_input_flush_does_not_raise_idc() {
    check(0, 1, 0x3c00, false, 1 << 19, 0, 0);
}
#[test]
fn single_addend_flush_raises_idc() {
    check(1, 0, 0x3c00, false, 1 << 24, 0, 0x80);
}
#[test]
fn quiet_addend_does_not_hide_invalid_product() {
    check(0x7fc12345, 0, 0x7c00, false, 0, 0x7fc00000, 1);
}
#[test]
fn half_signaling_payload_widens_after_nan_processing() {
    check(0, 0x7c01, 0x3c00, false, 0, 0x7fc02000, 1);
}
#[test]
fn subtraction_negates_first_half_nan_before_processing() {
    check(0, 0x7c01, 0x3c00, true, 0, 0xffc02000, 1);
}
#[test]
fn live_rounding_and_sticky_flags_are_preserved() {
    check(
        0x3f800000,
        1,
        0x3c00,
        false,
        (1 << 22) | (1 << 26) | 0x81,
        0x3f800001,
        0x10,
    );
}
