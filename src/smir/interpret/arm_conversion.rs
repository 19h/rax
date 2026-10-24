//! Arm FPConvert for binary32/binary64, with explicit guest control and status.
use super::arm_arithmetic::rounding;
use super::{SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64};

impl SmirInterpreter {
    pub(crate) fn arm_eval_precision_conversion(raw: u64, widen: bool, control: u32) -> (u64, u32) {
        let (source, destination) = if widen {
            (X86_SIMD_F32, X86_SIMD_F64)
        } else {
            (X86_SIMD_F64, X86_SIMD_F32)
        };
        let (source_sign, source_exponent, source_fraction, source_quiet) =
            Self::x86_simd_fp_masks(source);
        let (sign_mask, exponent_mask, _, quiet_mask) = Self::x86_simd_fp_masks(destination);
        let negative = raw & source_sign != 0;
        let sign = if negative { sign_mask } else { 0 };
        if Self::x86_simd_fp_is_nan(raw, source) {
            let status = u32::from(Self::x86_simd_fp_is_snan(raw, source));
            if control & (1 << 25) != 0 {
                return (exponent_mask | quiet_mask, status);
            }
            let payload = raw & source_fraction & !source_quiet;
            let payload = if widen {
                payload << (destination.fraction_bits - source.fraction_bits)
            } else {
                payload >> (source.fraction_bits - destination.fraction_bits)
            };
            return (sign | exponent_mask | quiet_mask | payload, status);
        }
        if raw & !source_sign == source_exponent {
            return (sign | exponent_mask, 0);
        }
        if raw & source_exponent == 0 && raw & source_fraction != 0 && control & (1 << 24) != 0 {
            return (sign, 1 << 7);
        }
        let value = Self::x86_simd_fp_decode(raw, source);
        Self::arm_round_arithmetic(
            negative,
            value.significand,
            value.exponent,
            false,
            destination,
            rounding(control),
            control,
        )
    }
}
