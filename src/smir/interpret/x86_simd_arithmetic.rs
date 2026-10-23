//! Shared, host-independent evaluation for legacy x86 SIMD arithmetic.

use super::{FpRoundMode, SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64};

#[derive(Clone, Copy, Debug)]
pub(crate) enum X86SimdArithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
    SquareRoot,
    Minimum,
    Maximum,
}

impl SmirInterpreter {
    /// Evaluate one binary32/binary64 lane using the same integer arithmetic as
    /// SMIR. The result contains raw IEEE-754 bits and newly raised MXCSR flags;
    /// instruction-level exception priority and commit remain the caller's job.
    pub(crate) fn x86_eval_simd_arithmetic(
        operation: X86SimdArithmetic,
        first: u64,
        second: u64,
        double: bool,
        mxcsr: u32,
    ) -> (u64, u32) {
        let format = if double { X86_SIMD_F64 } else { X86_SIMD_F32 };
        let rounding = match (mxcsr >> 13) & 3 {
            0 => FpRoundMode::RoundNearest,
            1 => FpRoundMode::RoundDown,
            2 => FpRoundMode::RoundUp,
            _ => FpRoundMode::RoundTowardZero,
        };
        let result = match operation {
            X86SimdArithmetic::Add => Self::x86_simd_fp_add(first, second, format, rounding, mxcsr),
            X86SimdArithmetic::Subtract => {
                Self::x86_simd_fp_sub(first, second, format, rounding, mxcsr)
            }
            X86SimdArithmetic::Multiply => {
                Self::x86_simd_fp_mul(first, second, format, rounding, mxcsr)
            }
            X86SimdArithmetic::Divide => {
                Self::x86_simd_fp_div(first, second, format, rounding, mxcsr)
            }
            X86SimdArithmetic::SquareRoot => {
                Self::x86_simd_fp_sqrt(second, format, rounding, mxcsr)
            }
            X86SimdArithmetic::Minimum => {
                Self::x86_simd_fp_min_max(first, second, format, mxcsr, true)
            }
            X86SimdArithmetic::Maximum => {
                Self::x86_simd_fp_min_max(first, second, format, mxcsr, false)
            }
        };
        (result.bits, result.status)
    }
}
