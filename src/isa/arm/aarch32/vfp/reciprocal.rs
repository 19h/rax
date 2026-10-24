//! Binary32 estimate operations and A32 nonfused refinement steps.
use super::Fpscr;
use crate::smir::interpret::SmirInterpreter;
fn estimate(value: f32, sqrt: bool, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) =
        SmirInterpreter::arm_eval_recip_estimate(value.to_bits(), sqrt, fpscr.bits());
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits)
}
/// Shared FPRecipEstimate operation under explicit FPSCR controls.
pub fn vrecpe_f32(value: f32, fpscr: &mut Fpscr) -> f32 {
    estimate(value, false, fpscr)
}
/// Shared FPRSqrtEstimate operation under explicit FPSCR controls.
pub fn vrsqrte_f32(value: f32, fpscr: &mut Fpscr) -> f32 {
    estimate(value, true, fpscr)
}
fn step(first: f32, second: f32, sqrt: bool, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) =
        SmirInterpreter::arm_eval_recip_step(first.to_bits(), second.to_bits(), sqrt);
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits)
}
/// A32 FPRecipStep, with a rounded binary32 product and standard SIMD controls.
pub fn vrecps_f32(first: f32, second: f32, fpscr: &mut Fpscr) -> f32 {
    step(first, second, false, fpscr)
}
/// A32 FPRSqrtStep, with a rounded product followed by one halved subtraction.
pub fn vrsqrts_f32(first: f32, second: f32, fpscr: &mut Fpscr) -> f32 {
    step(first, second, true, fpscr)
}

/// A32 half reciprocal refinement with a rounded half product and standard controls.
pub fn vrecps_f16_bits(first: u16, second: u16, fpscr: &mut Fpscr) -> u16 {
    half_step(first, second, false, fpscr)
}
/// A32 half reciprocal-square-root refinement with one final halved subtraction.
pub fn vrsqrts_f16_bits(first: u16, second: u16, fpscr: &mut Fpscr) -> u16 {
    half_step(first, second, true, fpscr)
}
fn half_step(first: u16, second: u16, sqrt: bool, fpscr: &mut Fpscr) -> u16 {
    let (bits, status) =
        SmirInterpreter::arm_eval_half_recip_step(first, second, sqrt, fpscr.bits());
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    bits
}
