//! Half precision conversion has a distinct AHP/FZ16 policy from data processing.
use super::Fpscr;
use crate::smir::interpret::SmirInterpreter;
/// Widen an IEEE binary16 encoding without NaN processing or FPSCR effects.
/// Use the controlled variant for architectural FPConvert behavior.
pub fn vcvt_f32_f16_bits(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 31);
    let fraction = u32::from(bits & 1023);
    let value = if exponent == 0 {
        if fraction == 0 {
            sign
        } else {
            let leading = 31 - fraction.leading_zeros();
            sign | ((leading + 103) << 23) | ((fraction ^ (1 << leading)) << (23 - leading))
        }
    } else if exponent == 31 {
        sign | 0x7f800000 | fraction << 13
    } else {
        sign | ((exponent + 112) << 23) | fraction << 13
    };
    f32::from_bits(value)
}
/// FPConvert from half to single, observing AHP and ignoring FZ16.
pub fn vcvt_f32_f16_bits_with_fpscr(bits: u16, fpscr: &mut Fpscr) -> f32 {
    let (result, status) =
        SmirInterpreter::arm_eval_half_precision_conversion(u32::from(bits), true, fpscr.bits());
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(result)
}
/// FPConvert from single to half with guest rounding, AHP, NaNs and sticky flags.
pub fn vcvt_f16_bits_f32(value: f32, fpscr: &mut Fpscr) -> u16 {
    let (result, status) =
        SmirInterpreter::arm_eval_half_precision_conversion(value.to_bits(), false, fpscr.bits());
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    result as u16
}
/// Data-processing signed integer conversion uses live RM and ignores AHP.
pub fn vcvt_f16_bits_s32(value: i32, fpscr: &mut Fpscr) -> u16 {
    integer(value as u32, true, fpscr)
}
/// Data-processing unsigned integer conversion uses live RM and ignores AHP.
pub fn vcvt_f16_bits_u32(value: u32, fpscr: &mut Fpscr) -> u16 {
    integer(value, false, fpscr)
}
fn integer(value: u32, signed: bool, fpscr: &mut Fpscr) -> u16 {
    let (result, status) =
        SmirInterpreter::arm_eval_half_integer_to_fp(value, signed, fpscr.bits());
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    result
}
