//! Arm FPRoundInt for binary32/binary64 with explicit control and sticky flags.
use super::{FpRoundMode, SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64};
impl SmirInterpreter {
    pub(crate) fn arm_eval_round_integral(
        raw: u64,
        double: bool,
        mode: FpRoundMode,
        exact: bool,
        control: u32,
    ) -> (u64, u32) {
        let format = if double { X86_SIMD_F64 } else { X86_SIMD_F32 };
        let (sign, exponent, fraction, quiet) = Self::x86_simd_fp_masks(format);
        if Self::x86_simd_fp_is_nan(raw, format) {
            let status = u32::from(Self::x86_simd_fp_is_snan(raw, format));
            let result = if control & (1 << 25) != 0 {
                exponent | quiet
            } else {
                raw | quiet
            };
            return (result, status);
        }
        if Self::x86_simd_fp_is_infinite(raw, format) || raw & !sign == 0 {
            return (raw, 0);
        }
        if raw & exponent == 0 && raw & fraction != 0 && control & (1 << 24) != 0 {
            return (raw & sign, 1 << 7);
        }
        let value = Self::x86_simd_fp_decode(raw, format);
        if value.exponent >= 0 {
            return (raw, 0);
        }
        // Fractional binary32/binary64 inputs have magnitude <2^23/<2^52.
        // Their rounded integer, including a carry, therefore fits signed 64 bits.
        let (integer, status) = Self::arm_eval_fp_to_fixed(raw, double, true, 64, 0, mode, control);
        debug_assert_eq!(status & 1, 0);
        let (result, additional) = Self::arm_round_arithmetic(
            raw & sign != 0,
            u128::from((integer as i64).unsigned_abs()),
            0,
            false,
            format,
            FpRoundMode::RoundTowardZero,
            control,
        );
        debug_assert_eq!(additional, 0);
        (result, if exact { status } else { status & !(1 << 4) })
    }
}
