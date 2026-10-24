//! Exact binary16 data processing, distinct from AHP precision conversions.
use super::{Fpscr, RoundingMode};
use crate::smir::interpret::{ArmFpOperation, SmirInterpreter};
fn accumulate(bits: u16, status: u32, fpscr: &mut Fpscr) -> u16 {
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    bits
}
pub(super) fn arithmetic(operation: ArmFpOperation, a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    let (bits, status) = SmirInterpreter::arm_eval_half_arithmetic(operation, a, b, fpscr.bits());
    accumulate(bits, status, fpscr)
}
pub(super) fn fused(acc: u16, a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    let (bits, status) = SmirInterpreter::arm_eval_half_fused(acc, a, b, fpscr.bits());
    accumulate(bits, status, fpscr)
}
pub(super) fn minmax(a: u16, b: u16, maximum: bool, fpscr: &mut Fpscr) -> u16 {
    let (bits, status) = SmirInterpreter::arm_eval_half_minmax(a, b, maximum, true, fpscr.bits());
    accumulate(bits, status, fpscr)
}
pub(super) fn compare(a: u16, b: u16, signal_all_nans: bool, fpscr: &mut Fpscr) {
    let (nzcv, status) =
        SmirInterpreter::arm_eval_half_compare(a, b, signal_all_nans, fpscr.bits());
    *fpscr = Fpscr::from_bits((fpscr.bits() & !0xf0000000) | nzcv | status);
}
pub(super) fn round_integral(
    value: u16,
    mode: RoundingMode,
    exact: bool,
    fpscr: &mut Fpscr,
) -> u16 {
    let (bits, status) = SmirInterpreter::arm_eval_half_round_integral(
        value,
        super::integer::mode(mode),
        exact,
        fpscr.bits(),
    );
    accumulate(bits, status, fpscr)
}
