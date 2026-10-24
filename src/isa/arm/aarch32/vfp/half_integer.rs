//! Half-to-integer data conversion: exact rounding, FZ16 and cumulative flags.
use super::{Fpscr, RoundingMode};
use crate::smir::interpret::SmirInterpreter;
fn convert(raw: u16, signed: bool, bits: u32, rounding: RoundingMode, fpscr: &mut Fpscr) -> u32 {
    let (result, status) = SmirInterpreter::arm_eval_half_to_integer(
        raw,
        signed,
        bits,
        super::integer::mode(rounding),
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    result
}
/// Convert binary16 to signed I32 with explicit rounding, ignoring AHP.
pub fn vcvt_s32_f16_bits_round(raw: u16, rounding: RoundingMode, fpscr: &mut Fpscr) -> i32 {
    convert(raw, true, 32, rounding, fpscr) as i32
}
/// Convert binary16 to unsigned I32 with explicit rounding, ignoring AHP.
pub fn vcvt_u32_f16_bits_round(raw: u16, rounding: RoundingMode, fpscr: &mut Fpscr) -> u32 {
    convert(raw, false, 32, rounding, fpscr)
}
/// Convert binary16 to signed I16 with architectural saturation and status.
pub fn vcvt_s16_f16_bits_round(raw: u16, rounding: RoundingMode, fpscr: &mut Fpscr) -> i16 {
    convert(raw, true, 16, rounding, fpscr) as i16
}
/// Convert binary16 to unsigned I16 with architectural saturation and status.
pub fn vcvt_u16_f16_bits_round(raw: u16, rounding: RoundingMode, fpscr: &mut Fpscr) -> u16 {
    convert(raw, false, 16, rounding, fpscr) as u16
}
