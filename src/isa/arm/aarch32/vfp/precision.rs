//! VFP binary32/binary64 precision changes with guest FPSCR semantics.
use super::Fpscr;
use crate::smir::interpret::SmirInterpreter;

/// Convert single to double using the default FPSCR controls. Call the
/// controlled variant when cumulative exceptions and guest controls matter.
pub fn vcvt_f64_f32(value: f32) -> f64 {
    vcvt_f64_f32_with_fpscr(value, &mut Fpscr::default())
}

/// Convert single to double with input flushing, NaN processing and sticky flags.
pub fn vcvt_f64_f32_with_fpscr(value: f32, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_precision_conversion(
        u64::from(value.to_bits()),
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits)
}

/// Convert double to single with guest rounding, flushing and sticky flags.
pub fn vcvt_f32_f64(value: f64, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) =
        SmirInterpreter::arm_eval_precision_conversion(value.to_bits(), false, fpscr.bits());
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}
