//! Arm FPToFixed/FixedToFP for binary32/binary64 and 32-bit integer operands.
//! Scale integer significands exactly; guest FPSCR never changes host FP state.
use super::{FpRoundMode, SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64};

impl SmirInterpreter {
    pub(crate) fn arm_eval_fp_to_integer(
        raw: u64,
        double: bool,
        signed: bool,
        fractional_bits: u32,
        mut mode: FpRoundMode,
        control: u32,
    ) -> (u32, u32) {
        let source = if double { X86_SIMD_F64 } else { X86_SIMD_F32 };
        let (sign, exponent, fraction, _) = Self::x86_simd_fp_masks(source);
        let negative = raw & sign != 0;
        let saturated = if Self::x86_simd_fp_is_nan(raw, source) {
            0
        } else {
            match (signed, negative) {
                (true, true) => i32::MIN as u32,
                (true, false) => i32::MAX as u32,
                (false, true) => 0,
                (false, false) => u32::MAX,
            }
        };
        if Self::x86_simd_fp_is_nan(raw, source) || Self::x86_simd_fp_is_infinite(raw, source) {
            return (saturated, 1);
        }
        if raw & exponent == 0 && raw & fraction != 0 && control & (1 << 24) != 0 {
            return (0, 1 << 7);
        }
        // Architectural immediates are <=32. For the legacy public helpers,
        // any larger shift beyond 2048 is equivalent: every nonzero finite
        // binary64 input is then outside the 32-bit integer range.
        let scaled = super::X86SimdFpFormat {
            bias: source.bias - fractional_bits.min(2048) as i32,
            ..source
        };
        if mode == FpRoundMode::RoundNearestTiesAway {
            let value = Self::x86_simd_fp_decode(raw, scaled);
            let shift = value.exponent.unsigned_abs();
            let halfway = if value.exponent < 0 && shift <= 128 {
                let mask = if shift == 128 {
                    u128::MAX
                } else {
                    (1u128 << shift) - 1
                };
                value.significand & mask == 1u128 << (shift - 1)
            } else {
                false
            };
            mode = if halfway {
                if negative {
                    FpRoundMode::RoundDown
                } else {
                    FpRoundMode::RoundUp
                }
            } else {
                FpRoundMode::RoundNearest
            };
        }
        let converted = Self::x86_simd_fp_to_int(raw, scaled, 32, signed, mode);
        if converted.status & 1 != 0 {
            (saturated, 1)
        } else {
            (
                converted.bits as u32,
                if converted.status & (1 << 5) != 0 {
                    1 << 4
                } else {
                    0
                },
            )
        }
    }

    pub(crate) fn arm_eval_integer_to_fp(
        raw: u32,
        double: bool,
        signed: bool,
        fractional_bits: u32,
        control: u32,
    ) -> (u64, u32) {
        let negative = signed && (raw as i32) < 0;
        let magnitude = if negative {
            (raw as i32).unsigned_abs()
        } else {
            raw
        };
        // Beyond 2048 fractional bits every 32-bit magnitude is below the
        // least binary64 subnormal. Directed rounding and FZ still apply.
        Self::arm_round_arithmetic(
            negative,
            u128::from(magnitude),
            -(fractional_bits.min(2048) as i32),
            false,
            if double { X86_SIMD_F64 } else { X86_SIMD_F32 },
            super::arm_arithmetic::rounding(control),
            control,
        )
    }
}
