//! Scalar fused arithmetic: operand negation precedes one final rounding.
use super::{Fpscr, vcvt_f16_bits_f32, vcvt_f32_f16_bits};
use crate::smir::interpret::SmirInterpreter;

/// Execute the binary32 VFMA operation.
pub fn vfma_f32(acc: f32, a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits()) as u64,
        (a.to_bits()) as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute the binary64 VFMA operation.
pub fn vfma_f64(acc: f64, a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits()) as u64,
        (a.to_bits()) as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision fused multiply-accumulate.
pub fn vfma_f16_bits(acc: u16, a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    vcvt_f16_bits_f32(
        vfma_f32(
            vcvt_f32_f16_bits(acc),
            vcvt_f32_f16_bits(a),
            vcvt_f32_f16_bits(b),
            fpscr,
        ),
        fpscr,
    )
}

/// Execute the binary32 VFMS operation.
pub fn vfms_f32(acc: f32, a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits()) as u64,
        (a.to_bits() ^ 0x8000_0000) as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute the binary64 VFMS operation.
pub fn vfms_f64(acc: f64, a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits()) as u64,
        (a.to_bits() ^ 0x8000_0000_0000_0000) as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision fused multiply-subtract.
pub fn vfms_f16_bits(acc: u16, a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    vcvt_f16_bits_f32(
        vfms_f32(
            vcvt_f32_f16_bits(acc),
            vcvt_f32_f16_bits(a),
            vcvt_f32_f16_bits(b),
            fpscr,
        ),
        fpscr,
    )
}

/// Execute the binary32 VFNMA operation.
pub fn vfnma_f32(acc: f32, a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits() ^ 0x8000_0000) as u64,
        (a.to_bits() ^ 0x8000_0000) as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute the binary64 VFNMA operation.
pub fn vfnma_f64(acc: f64, a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits() ^ 0x8000_0000_0000_0000) as u64,
        (a.to_bits() ^ 0x8000_0000_0000_0000) as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision fused negated multiply-accumulate.
pub fn vfnma_f16_bits(acc: u16, a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    vcvt_f16_bits_f32(
        vfnma_f32(
            vcvt_f32_f16_bits(acc),
            vcvt_f32_f16_bits(a),
            vcvt_f32_f16_bits(b),
            fpscr,
        ),
        fpscr,
    )
}

/// Execute the binary32 VFNMS operation.
pub fn vfnms_f32(acc: f32, a: f32, b: f32, fpscr: &mut Fpscr) -> f32 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits() ^ 0x8000_0000) as u64,
        (a.to_bits()) as u64,
        b.to_bits() as u64,
        false,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f32::from_bits(bits as u32)
}

/// Execute the binary64 VFNMS operation.
pub fn vfnms_f64(acc: f64, a: f64, b: f64, fpscr: &mut Fpscr) -> f64 {
    let (bits, status) = SmirInterpreter::arm_eval_fused(
        (acc.to_bits() ^ 0x8000_0000_0000_0000) as u64,
        (a.to_bits()) as u64,
        b.to_bits() as u64,
        true,
        fpscr.bits(),
    );
    *fpscr = Fpscr::from_bits(fpscr.bits() | status);
    f64::from_bits(bits as u64)
}

/// Execute half-precision fused negated multiply-subtract.
pub fn vfnms_f16_bits(acc: u16, a: u16, b: u16, fpscr: &mut Fpscr) -> u16 {
    vcvt_f16_bits_f32(
        vfnms_f32(
            vcvt_f32_f16_bits(acc),
            vcvt_f32_f16_bits(a),
            vcvt_f32_f16_bits(b),
            fpscr,
        ),
        fpscr,
    )
}
