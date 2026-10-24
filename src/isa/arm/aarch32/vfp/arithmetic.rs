//! Scalar VFP arithmetic and exact guest-controlled binary16/binary32/binary64 results.
use super::Fpscr;
use crate::smir::interpret::{ArmFpOperation, SmirInterpreter};

/// Execute single-precision addition.
pub fn vadd_f32(a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Add,
        a.to_bits() as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute double-precision addition.
pub fn vadd_f64(a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Add,
        a.to_bits() as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision addition.
pub fn vadd_f16_bits(a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    super::half::arithmetic(ArmFpOperation::Add, a, b, fpscr)
}

/// Execute single-precision subtraction.
pub fn vsub_f32(a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Subtract,
        a.to_bits() as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute double-precision subtraction.
pub fn vsub_f64(a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Subtract,
        a.to_bits() as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision subtraction.
pub fn vsub_f16_bits(a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    super::half::arithmetic(ArmFpOperation::Subtract, a, b, fpscr)
}

/// Execute single-precision multiplication.
pub fn vmul_f32(a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Multiply,
        a.to_bits() as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute double-precision multiplication.
pub fn vmul_f64(a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Multiply,
        a.to_bits() as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision multiplication.
pub fn vmul_f16_bits(a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    super::half::arithmetic(ArmFpOperation::Multiply, a, b, fpscr)
}

/// Execute single-precision division.
pub fn vdiv_f32(a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Divide,
        a.to_bits() as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute double-precision division.
pub fn vdiv_f64(a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::Divide,
        a.to_bits() as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision division.
pub fn vdiv_f16_bits(a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    super::half::arithmetic(ArmFpOperation::Divide, a, b, fpscr)
}

/// Execute single-precision negation.
pub fn vneg_f32(a: f32) -> f32 {
    f32::from_bits(a.to_bits() ^ (1 << 31))
}

/// Execute double-precision negation.
pub fn vneg_f64(a: f64) -> f64 {
    f64::from_bits(a.to_bits() ^ (1 << 63))
}

/// Execute half-precision negation.
pub fn vneg_f16_bits(a: u16) -> u16 {
    a ^ 0x8000
}

/// Execute single-precision absolute value.
pub fn vabs_f32(a: f32) -> f32 {
    a.abs()
}

/// Execute double-precision absolute value.
pub fn vabs_f64(a: f64) -> f64 {
    a.abs()
}

/// Execute half-precision absolute value.
pub fn vabs_f16_bits(a: u16) -> u16 {
    a & 0x7FFF
}

/// Execute single-precision square root.
pub fn vsqrt_f32(a: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::SquareRoot,
        a.to_bits() as u64,
        0,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute double-precision square root.
pub fn vsqrt_f64(a: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_arithmetic(
        ArmFpOperation::SquareRoot,
        a.to_bits() as u64,
        0,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision square root.
pub fn vsqrt_f16_bits(a: u16, fpscr: &mut Fpscr) -> u16 {
    super::half::arithmetic(ArmFpOperation::SquareRoot, a, 0, fpscr)
}
