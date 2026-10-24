//! VFP integer/fixed conversions with guest controls and cumulative status.
use super::{Fpscr, RoundingMode};
use crate::smir::interpret::SmirInterpreter;
use crate::smir::ir::types::FpRoundMode;

pub(super) fn mode(mode: RoundingMode) -> FpRoundMode {
    match mode {
        RoundingMode::RoundNearest => FpRoundMode::RoundNearest,
        RoundingMode::RoundPlusInf => FpRoundMode::RoundUp,
        RoundingMode::RoundMinusInf => FpRoundMode::RoundDown,
        RoundingMode::RoundZero => FpRoundMode::RoundTowardZero,
        RoundingMode::RoundTiesAway => FpRoundMode::RoundNearestTiesAway,
    }
}

fn to_integer(
    raw: u64,
    double: bool,
    signed: bool,
    fbits: u32,
    rounding: RoundingMode,
    fpscr: &mut Fpscr,
) -> u32 {
    let (result, status) = SmirInterpreter::arm_eval_fp_to_integer(
        raw,
        double,
        signed,
        fbits,
        mode(rounding),
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    result
}

fn from_integer(
    raw: u32,
    double: bool,
    signed: bool,
    fbits: u32,
    fpscr: &mut Fpscr,
    nearest: bool,
) -> u64 {
    let control = if nearest {
        fpscr.bits() & !(3 << 22)
    } else {
        fpscr.bits()
    };
    let (result, status) =
        SmirInterpreter::arm_eval_integer_to_fp(raw, double, signed, fbits, control);
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    result
}

/// Convert f32 to i32, preserving sticky FPSCR status.
pub fn vcvt_s32_f32(value: f32, fpscr: &mut Fpscr) -> i32 {
    to_integer(
        u64::from(value.to_bits()),
        false,
        true,
        0,
        RoundingMode::RoundZero,
        fpscr,
    ) as i32
}

/// Convert f32 to i32, preserving sticky FPSCR status.
pub fn vcvt_s32_f32_fixed(value: f32, fbits: u32, fpscr: &mut Fpscr) -> i32 {
    to_integer(
        u64::from(value.to_bits()),
        false,
        true,
        fbits,
        RoundingMode::RoundZero,
        fpscr,
    ) as i32
}

/// Convert f32 to i32, preserving sticky FPSCR status.
pub fn vcvt_s32_f32_round(value: f32, rounding: RoundingMode, fpscr: &mut Fpscr) -> i32 {
    to_integer(u64::from(value.to_bits()), false, true, 0, rounding, fpscr) as i32
}

/// Convert f32 to i32 using the FPSCR rounding mode.
pub fn vcvtr_s32_f32(value: f32, fpscr: &mut Fpscr) -> i32 {
    to_integer(
        u64::from(value.to_bits()),
        false,
        true,
        0,
        fpscr.rmode(),
        fpscr,
    ) as i32
}

/// Convert i32 to f32 with default FPSCR controls.
pub fn vcvt_f32_s32(value: i32) -> f32 {
    vcvt_f32_s32_with_fpscr(value, &mut Fpscr::default())
}

/// Convert i32 to f32 with guest rounding and cumulative exceptions.
pub fn vcvt_f32_s32_with_fpscr(value: i32, fpscr: &mut Fpscr) -> f32 {
    f32::from_bits(from_integer(value as u32, false, true, 0, fpscr, false) as u32)
}

/// Convert i32 to f32 with default FPSCR controls.
pub fn vcvt_f32_s32_fixed(value: i32, fbits: u32) -> f32 {
    vcvt_f32_s32_fixed_with_fpscr(value, fbits, &mut Fpscr::default())
}

/// Convert i32 to f32 using nearest rounding and cumulative exceptions.
pub fn vcvt_f32_s32_fixed_with_fpscr(value: i32, fbits: u32, fpscr: &mut Fpscr) -> f32 {
    f32::from_bits(from_integer(value as u32, false, true, fbits, fpscr, true) as u32)
}

/// Convert f32 to u32, preserving sticky FPSCR status.
pub fn vcvt_u32_f32(value: f32, fpscr: &mut Fpscr) -> u32 {
    to_integer(
        u64::from(value.to_bits()),
        false,
        false,
        0,
        RoundingMode::RoundZero,
        fpscr,
    )
}

/// Convert f32 to u32, preserving sticky FPSCR status.
pub fn vcvt_u32_f32_fixed(value: f32, fbits: u32, fpscr: &mut Fpscr) -> u32 {
    to_integer(
        u64::from(value.to_bits()),
        false,
        false,
        fbits,
        RoundingMode::RoundZero,
        fpscr,
    )
}

/// Convert f32 to u32, preserving sticky FPSCR status.
pub fn vcvt_u32_f32_round(value: f32, rounding: RoundingMode, fpscr: &mut Fpscr) -> u32 {
    to_integer(u64::from(value.to_bits()), false, false, 0, rounding, fpscr)
}

/// Convert f32 to u32 using the FPSCR rounding mode.
pub fn vcvtr_u32_f32(value: f32, fpscr: &mut Fpscr) -> u32 {
    to_integer(
        u64::from(value.to_bits()),
        false,
        false,
        0,
        fpscr.rmode(),
        fpscr,
    )
}

/// Convert u32 to f32 with default FPSCR controls.
pub fn vcvt_f32_u32(value: u32) -> f32 {
    vcvt_f32_u32_with_fpscr(value, &mut Fpscr::default())
}

/// Convert u32 to f32 with guest rounding and cumulative exceptions.
pub fn vcvt_f32_u32_with_fpscr(value: u32, fpscr: &mut Fpscr) -> f32 {
    f32::from_bits(from_integer(value as u32, false, false, 0, fpscr, false) as u32)
}

/// Convert u32 to f32 with default FPSCR controls.
pub fn vcvt_f32_u32_fixed(value: u32, fbits: u32) -> f32 {
    vcvt_f32_u32_fixed_with_fpscr(value, fbits, &mut Fpscr::default())
}

/// Convert u32 to f32 using nearest rounding and cumulative exceptions.
pub fn vcvt_f32_u32_fixed_with_fpscr(value: u32, fbits: u32, fpscr: &mut Fpscr) -> f32 {
    f32::from_bits(from_integer(value as u32, false, false, fbits, fpscr, true) as u32)
}

/// Convert f64 to i32, preserving sticky FPSCR status.
pub fn vcvt_s32_f64(value: f64, fpscr: &mut Fpscr) -> i32 {
    to_integer(
        value.to_bits(),
        true,
        true,
        0,
        RoundingMode::RoundZero,
        fpscr,
    ) as i32
}

/// Convert f64 to i32, preserving sticky FPSCR status.
pub fn vcvt_s32_f64_fixed(value: f64, fbits: u32, fpscr: &mut Fpscr) -> i32 {
    to_integer(
        value.to_bits(),
        true,
        true,
        fbits,
        RoundingMode::RoundZero,
        fpscr,
    ) as i32
}

/// Convert f64 to i32, preserving sticky FPSCR status.
pub fn vcvt_s32_f64_round(value: f64, rounding: RoundingMode, fpscr: &mut Fpscr) -> i32 {
    to_integer(value.to_bits(), true, true, 0, rounding, fpscr) as i32
}

/// Convert f64 to i32 using the FPSCR rounding mode.
pub fn vcvtr_s32_f64(value: f64, fpscr: &mut Fpscr) -> i32 {
    to_integer(value.to_bits(), true, true, 0, fpscr.rmode(), fpscr) as i32
}

/// Convert i32 to f64 with default FPSCR controls.
pub fn vcvt_f64_s32(value: i32) -> f64 {
    vcvt_f64_s32_with_fpscr(value, &mut Fpscr::default())
}

/// Convert i32 to f64 with guest rounding and cumulative exceptions.
pub fn vcvt_f64_s32_with_fpscr(value: i32, fpscr: &mut Fpscr) -> f64 {
    f64::from_bits(from_integer(value as u32, true, true, 0, fpscr, false))
}

/// Convert i32 to f64 with default FPSCR controls.
pub fn vcvt_f64_s32_fixed(value: i32, fbits: u32) -> f64 {
    vcvt_f64_s32_fixed_with_fpscr(value, fbits, &mut Fpscr::default())
}

/// Convert i32 to f64 using nearest rounding and cumulative exceptions.
pub fn vcvt_f64_s32_fixed_with_fpscr(value: i32, fbits: u32, fpscr: &mut Fpscr) -> f64 {
    f64::from_bits(from_integer(value as u32, true, true, fbits, fpscr, true))
}

/// Convert f64 to u32, preserving sticky FPSCR status.
pub fn vcvt_u32_f64(value: f64, fpscr: &mut Fpscr) -> u32 {
    to_integer(
        value.to_bits(),
        true,
        false,
        0,
        RoundingMode::RoundZero,
        fpscr,
    )
}

/// Convert f64 to u32, preserving sticky FPSCR status.
pub fn vcvt_u32_f64_fixed(value: f64, fbits: u32, fpscr: &mut Fpscr) -> u32 {
    to_integer(
        value.to_bits(),
        true,
        false,
        fbits,
        RoundingMode::RoundZero,
        fpscr,
    )
}

/// Convert f64 to u32, preserving sticky FPSCR status.
pub fn vcvt_u32_f64_round(value: f64, rounding: RoundingMode, fpscr: &mut Fpscr) -> u32 {
    to_integer(value.to_bits(), true, false, 0, rounding, fpscr)
}

/// Convert f64 to u32 using the FPSCR rounding mode.
pub fn vcvtr_u32_f64(value: f64, fpscr: &mut Fpscr) -> u32 {
    to_integer(value.to_bits(), true, false, 0, fpscr.rmode(), fpscr)
}

/// Convert u32 to f64 with default FPSCR controls.
pub fn vcvt_f64_u32(value: u32) -> f64 {
    vcvt_f64_u32_with_fpscr(value, &mut Fpscr::default())
}

/// Convert u32 to f64 with guest rounding and cumulative exceptions.
pub fn vcvt_f64_u32_with_fpscr(value: u32, fpscr: &mut Fpscr) -> f64 {
    f64::from_bits(from_integer(value as u32, true, false, 0, fpscr, false))
}

/// Convert u32 to f64 with default FPSCR controls.
pub fn vcvt_f64_u32_fixed(value: u32, fbits: u32) -> f64 {
    vcvt_f64_u32_fixed_with_fpscr(value, fbits, &mut Fpscr::default())
}

/// Convert u32 to f64 using nearest rounding and cumulative exceptions.
pub fn vcvt_f64_u32_fixed_with_fpscr(value: u32, fbits: u32, fpscr: &mut Fpscr) -> f64 {
    f64::from_bits(from_integer(value as u32, true, false, fbits, fpscr, true))
}
