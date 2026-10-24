//! Scalar number selection and comparisons use architectural NaN/FZ rules.
use super::Fpscr;
use crate::smir::interpret::SmirInterpreter;

/// Execute binary32 VMAXNM.
pub fn vmaxnm_f32(a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_minmax(
        a.to_bits() as u64,
        b.to_bits() as u64,
        false,
        true,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute binary64 VMAXNM.
pub fn vmaxnm_f64(a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_minmax(
        a.to_bits() as u64,
        b.to_bits() as u64,
        true,
        true,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision maxNum.
pub fn vmaxnm_f16_bits(a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    super::half::minmax(a, b, true, fpscr)
}

/// Execute binary32 VMINNM.
pub fn vminnm_f32(a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_minmax(
        a.to_bits() as u64,
        b.to_bits() as u64,
        false,
        false,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute binary64 VMINNM.
pub fn vminnm_f64(a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_minmax(
        a.to_bits() as u64,
        b.to_bits() as u64,
        true,
        false,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision minNum.
pub fn vminnm_f16_bits(a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    super::half::minmax(a, b, false, fpscr)
}

/// Compare binary32, signaling invalid only for signaling NaNs.
pub fn vcmp_f32(a: f32, b: f32, fpscr: &mut Fpscr) {
    vcmp_f32_with_exception(a, b, false, fpscr);
}
/// Compare binary32 with explicit quiet/signaling comparison selection.
pub fn vcmp_f32_with_exception(a: f32, b: f32, signal_all_nans: bool, fpscr: &mut Fpscr) {
    let (nzcv, status) = SmirInterpreter::arm_eval_compare(
        a.to_bits() as u64,
        b.to_bits() as u64,
        false,
        signal_all_nans,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits((fpscr.bits() & !0xf000_0000) | nzcv | status);
}

/// Compare binary64, signaling invalid only for signaling NaNs.
pub fn vcmp_f64(a: f64, b: f64, fpscr: &mut Fpscr) {
    vcmp_f64_with_exception(a, b, false, fpscr);
}
/// Compare binary64 with explicit quiet/signaling comparison selection.
pub fn vcmp_f64_with_exception(a: f64, b: f64, signal_all_nans: bool, fpscr: &mut Fpscr) {
    let (nzcv, status) = SmirInterpreter::arm_eval_compare(
        a.to_bits() as u64,
        b.to_bits() as u64,
        true,
        signal_all_nans,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits((fpscr.bits() & !0xf000_0000) | nzcv | status);
}
