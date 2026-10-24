//! Arm integer/fixed conversion and shared FPToFixed rounding intermediates.
//! Scale integer significands exactly; guest FPSCR never changes host FP state.
use super::{FpRoundMode, SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64};

impl SmirInterpreter {
    pub(crate) fn arm_eval_fp_to_integer(
        raw: u64,
        double: bool,
        signed: bool,
        fractional_bits: u32,
        mode: FpRoundMode,
        control: u32,
    ) -> (u32, u32) {
        let (result, status) =
            Self::arm_eval_fp_to_fixed(raw, double, signed, 32, fractional_bits, mode, control);
        (result as u32, status)
    }

    pub(super) fn arm_eval_fp_to_fixed(
        raw: u64,
        double: bool,
        signed: bool,
        integer_bits: u32,
        fractional_bits: u32,
        mode: FpRoundMode,
        control: u32,
    ) -> (u64, u32) {
        Self::arm_eval_fp_to_fixed_format(
            raw,
            if double { X86_SIMD_F64 } else { X86_SIMD_F32 },
            signed,
            integer_bits,
            fractional_bits,
            mode,
            control,
        )
    }
    pub(super) fn arm_eval_fp_to_fixed_format(
        raw: u64,
        format: super::X86SimdFpFormat,
        signed: bool,
        integer_bits: u32,
        fractional_bits: u32,
        mut mode: FpRoundMode,
        control: u32,
    ) -> (u64, u32) {
        debug_assert!(matches!(integer_bits, 32 | 64));
        let source = format;
        let (sign, exponent, fraction, _) = Self::x86_simd_fp_masks(source);
        let negative = raw & sign != 0;
        let saturated = if Self::x86_simd_fp_is_nan(raw, source) {
            0
        } else {
            match (signed, negative) {
                (true, true) => 1u64 << (integer_bits - 1),
                (true, false) => (1u64 << (integer_bits - 1)) - 1,
                (false, true) => 0,
                (false, false) => u64::MAX >> (64 - integer_bits),
            }
        };
        if Self::x86_simd_fp_is_nan(raw, source) || Self::x86_simd_fp_is_infinite(raw, source) {
            return (saturated, 1);
        }
        if raw & exponent == 0 && raw & fraction != 0 && control & (1 << 24) != 0 {
            return (0, if format.total_bits == 16 { 0 } else { 1 << 7 });
        }
        // Architectural immediates are <=32. For the legacy public helpers,
        // any larger shift beyond 2048 is equivalent: every nonzero finite
        // binary64 input is then outside even the 64-bit integer range.
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
        let converted = Self::x86_simd_fp_to_int(raw, scaled, integer_bits, signed, mode);
        if converted.status & 1 != 0 {
            (saturated, 1)
        } else {
            (
                converted.bits,
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
