//! Arm precision conversion, including IEEE/alternative half profiles.
use super::arm_arithmetic::rounding;
use super::{SmirInterpreter, X86_SIMD_F16, X86_SIMD_F32, X86_SIMD_F64, X86SimdFpFormat};
impl SmirInterpreter {
    pub(crate) fn arm_eval_precision_conversion(raw: u64, widen: bool, control: u32) -> (u64, u32) {
        let (source, destination) = if widen {
            (X86_SIMD_F32, X86_SIMD_F64)
        } else {
            (X86_SIMD_F64, X86_SIMD_F32)
        };
        Self::arm_eval_precision_conversion_formats(raw, source, destination, control)
    }
    pub(crate) fn arm_eval_half_precision_conversion(
        raw: u32,
        widen: bool,
        control: u32,
    ) -> (u32, u32) {
        let (source, destination) = if widen {
            (X86_SIMD_F16, X86_SIMD_F32)
        } else {
            (X86_SIMD_F32, X86_SIMD_F16)
        };
        let (bits, status) = Self::arm_eval_precision_conversion_formats(
            u64::from(raw),
            source,
            destination,
            control,
        );
        (bits as u32, status)
    }
    pub(crate) fn arm_eval_half_double_precision_conversion(
        raw: u64,
        widen: bool,
        control: u32,
    ) -> (u64, u32) {
        let (source, destination) = if widen {
            (X86_SIMD_F16, X86_SIMD_F64)
        } else {
            (X86_SIMD_F64, X86_SIMD_F16)
        };
        Self::arm_eval_precision_conversion_formats(raw, source, destination, control)
    }
    fn arm_eval_precision_conversion_formats(
        raw: u64,
        source: X86SimdFpFormat,
        destination: X86SimdFpFormat,
        control: u32,
    ) -> (u64, u32) {
        let (source_sign, source_exponent, source_fraction, source_quiet) =
            Self::x86_simd_fp_masks(source);
        let (sign_mask, exponent_mask, fraction_mask, quiet_mask) =
            Self::x86_simd_fp_masks(destination);
        let negative = raw & source_sign != 0;
        let sign = if negative { sign_mask } else { 0 };
        let source_alternative = source.total_bits == 16 && control & (1 << 26) != 0;
        let destination_alternative = destination.total_bits == 16 && control & (1 << 26) != 0;
        if !source_alternative && Self::x86_simd_fp_is_nan(raw, source) {
            let status = u32::from(Self::x86_simd_fp_is_snan(raw, source));
            if destination_alternative {
                return (sign, 1);
            }
            if control & (1 << 25) != 0 {
                return (exponent_mask | quiet_mask, status);
            }
            let payload = raw & source_fraction & !source_quiet;
            let payload = if destination.fraction_bits > source.fraction_bits {
                payload << (destination.fraction_bits - source.fraction_bits)
            } else {
                payload >> (source.fraction_bits - destination.fraction_bits)
            };
            return (sign | exponent_mask | quiet_mask | payload, status);
        }
        if !source_alternative && raw & !source_sign == source_exponent {
            return if destination_alternative {
                (sign | exponent_mask | fraction_mask, 1)
            } else {
                (sign | exponent_mask, 0)
            };
        }
        // FPUnpackCV ignores FZ16. FZ still applies to non-half source formats.
        if source.total_bits != 16
            && raw & source_exponent == 0
            && raw & source_fraction != 0
            && control & (1 << 24) != 0
        {
            return (sign, 1 << 7);
        }
        let value = Self::x86_simd_fp_decode(raw, source);
        // FPRoundCV observes AHP, ignores FZ16; FZ cannot flush a half result.
        let output_control = if destination.total_bits == 16 {
            control & !((1 << 24) | (1 << 19))
        } else {
            control
        };
        Self::arm_round_arithmetic(
            negative,
            value.significand,
            value.exponent,
            false,
            destination,
            rounding(control),
            output_control,
        )
    }
}
