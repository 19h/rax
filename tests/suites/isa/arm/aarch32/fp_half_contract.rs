//! Binary16 data processing uses FZ16 and its own precision/status rules.
use rax::isa::arm::aarch32::vfp::{self, Fpscr, RoundingMode};
fn binary(
    op: fn(u16, u16, &mut Fpscr) -> u16,
    a: u16,
    b: u16,
    control: u32,
    want: u16,
    status: u32,
) {
    let initial = 0xa8000000 | control;
    let mut f = Fpscr::from_bits(initial);
    assert_eq!((op(a, b, &mut f), f.bits()), (want, initial | status));
}
macro_rules! case {
    ($name:ident,$op:ident,$a:expr,$b:expr,$control:expr,$want:expr,$status:expr) => {
        #[test]
        fn $name() {
            binary(vfp::$op, $a, $b, $control, $want, $status);
        }
    };
}
case!(
    exact_subnormal_does_not_raise_underflow,
    vadd_f16_bits,
    1,
    1,
    0,
    2,
    0
);
case!(
    half_input_flush_does_not_raise_input_denormal,
    vadd_f16_bits,
    1,
    1,
    1 << 19,
    0,
    0
);
case!(
    single_flush_control_does_not_flush_half,
    vadd_f16_bits,
    1,
    1,
    1 << 24,
    2,
    0
);
case!(
    arithmetic_ignores_alternative_half_format,
    vadd_f16_bits,
    0x7c00,
    0x3c00,
    1 << 26,
    0x7c00,
    0
);
case!(
    quiet_half_nan_does_not_raise_invalid,
    vadd_f16_bits,
    0x7e01,
    0x3c00,
    0,
    0x7e01,
    0
);
case!(
    signaling_half_nan_quiets_and_raises_invalid,
    vadd_f16_bits,
    0x7c01,
    0x3c00,
    0,
    0x7e01,
    1
);
case!(
    default_half_nan_discards_payload_without_invalid,
    vadd_f16_bits,
    0xfe01,
    0x3c00,
    1 << 25,
    0x7e00,
    0
);
case!(
    nearest_halfway_add_rounds_even_and_sets_inexact,
    vadd_f16_bits,
    0x3c00,
    0x1000,
    0,
    0x3c00,
    0x10
);
case!(
    directed_halfway_add_rounds_up,
    vadd_f16_bits,
    0x3c00,
    0x1000,
    1 << 22,
    0x3c01,
    0x10
);
case!(
    positive_overflow_toward_zero_saturates_finite,
    vadd_f16_bits,
    0x7bff,
    0x7bff,
    3 << 22,
    0x7bff,
    0x14
);
case!(
    negative_overflow_toward_zero_saturates_finite,
    vadd_f16_bits,
    0xfbff,
    0xfbff,
    3 << 22,
    0xfbff,
    0x14
);
case!(
    division_by_flushed_half_reports_zero_divide_without_idc,
    vdiv_f16_bits,
    0x3c00,
    1,
    1 << 19,
    0x7c00,
    2
);
case!(
    exact_product_subnormal_does_not_raise_underflow,
    vmul_f16_bits,
    0x0400,
    0x3800,
    0,
    0x0200,
    0
);
case!(
    product_flush_sets_underflow_without_inexact,
    vmul_f16_bits,
    0x0400,
    0x3800,
    1 << 19,
    0,
    8
);
case!(
    maxnum_preserves_exact_subnormal,
    vmaxnm_f16_bits,
    1,
    0,
    0,
    1,
    0
);
case!(
    maxnum_flushes_half_input_without_idc,
    vmaxnm_f16_bits,
    1,
    0,
    1 << 19,
    0,
    0
);
#[test]
fn fused_half_retains_exact_subnormal_residual() {
    let mut f = Fpscr::default();
    assert_eq!(vfp::vfma_f16_bits(0xbc00, 0x3c01, 0x3bfe, &mut f), 0x8010);
    assert_eq!(f.bits(), 0);
}
#[test]
fn fused_half_output_flush_preserves_negative_zero() {
    let mut f = Fpscr::from_bits(1 << 19);
    assert_eq!(vfp::vfma_f16_bits(0xbc00, 0x3c01, 0x3bfe, &mut f), 0x8000);
    assert_eq!(f.bits(), (1 << 19) | 8);
}
#[test]
fn nonfused_half_rounds_product_before_adding() {
    let mut f = Fpscr::default();
    assert_eq!(vfp::vmla_f16_bits(0xbc00, 0x3c01, 0x3bfe, &mut f), 0);
    assert_eq!(f.bits(), 0x10);
}
#[test]
fn compare_half_input_flush_changes_relation_without_idc() {
    let mut f = Fpscr::from_bits((1 << 19) | 0x08000004);
    vfp::vcmp_f16_bits(1, 0, &mut f);
    assert_eq!(f.bits(), 0x68080004);
}
#[test]
fn compare_quiet_half_nan_keeps_invalid_clear() {
    let mut f = Fpscr::default();
    vfp::vcmp_f16_bits(0x7e01, 0, &mut f);
    assert_eq!(f.bits(), 0x30000000);
}
#[test]
fn integral_rounding_flushes_half_input_before_rounding() {
    let mut f = Fpscr::from_bits(1 << 19);
    assert_eq!(
        vfp::vrint_f16_bits(0x8001, RoundingMode::RoundMinusInf, true, &mut f),
        0x8000
    );
    assert_eq!(f.bits(), 1 << 19);
}
#[test]
fn integral_rounding_quiet_half_nan_does_not_raise_invalid() {
    let mut f = Fpscr::default();
    assert_eq!(
        vfp::vrint_f16_bits(0x7e01, RoundingMode::RoundNearest, true, &mut f),
        0x7e01
    );
    assert_eq!(f.bits(), 0);
}
