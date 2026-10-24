//! Integral floating-point rounding without modifying host FP control or flags.
use super::integer::mode;
use super::{Fpscr, RoundingMode};
use crate::smir::interpret::SmirInterpreter;
/// Round binary32 using the explicit guest rounding mode and exception policy.
pub fn vrint_f32(value: f32, rounding: RoundingMode, exact: bool, fpscr: &mut Fpscr) -> f32 {
    let (result, status) = SmirInterpreter::arm_eval_round_integral(
        u64::from(value.to_bits()),
        false,
        mode(rounding),
        exact,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(result as u32)
}
/// Round binary64 using the explicit guest rounding mode and exception policy.
pub fn vrint_f64(value: f64, rounding: RoundingMode, exact: bool, fpscr: &mut Fpscr) -> f64 {
    let (result, status) = SmirInterpreter::arm_eval_round_integral(
        value.to_bits(),
        true,
        mode(rounding),
        exact,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(result)
}
