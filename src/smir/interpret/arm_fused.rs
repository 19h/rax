//! Arm FPMulAdd: unpack three operands, process NaNs, then round exactly once.
use super::arm_arithmetic::rounding;
use super::{FpRoundMode, SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64, hr_add_scaled};

impl SmirInterpreter {
    pub(crate) fn arm_eval_fused(
        accumulator: u64,
        first: u64,
        second: u64,
        double: bool,
        control: u32,
    ) -> (u64, u32) {
        Self::arm_eval_fused_format(
            accumulator,
            first,
            second,
            if double { X86_SIMD_F64 } else { X86_SIMD_F32 },
            control,
        )
    }
    pub(crate) fn arm_eval_half_fused(
        accumulator: u16,
        first: u16,
        second: u16,
        control: u32,
    ) -> (u16, u32) {
        let (bits, status) = Self::arm_eval_fused_format(
            u64::from(accumulator),
            u64::from(first),
            u64::from(second),
            super::X86_SIMD_F16,
            super::arm_format::half_control(control),
        );
        (bits as u16, status)
    }
    pub(super) fn arm_eval_fused_format(
        mut accumulator: u64,
        mut first: u64,
        mut second: u64,
        format: super::X86SimdFpFormat,
        control: u32,
    ) -> (u64, u32) {
        let double = format.total_bits == 64;
        let (sign, exponent, fraction, quiet) = Self::x86_simd_fp_masks(format);
        let canonical_nan = exponent | quiet;
        let mut status = 0;
        if control & (1 << 24) != 0 {
            for input in [&mut accumulator, &mut first, &mut second] {
                if *input & exponent == 0 && *input & fraction != 0 {
                    *input &= sign;
                    status |= if format.total_bits == 16 { 0 } else { 1 << 7 };
                }
            }
        }
        let inf1 = first & !sign == exponent;
        let inf2 = second & !sign == exponent;
        let zero1 = first & !sign == 0;
        let zero2 = second & !sign == 0;
        let invalid_product = (inf1 && zero2) || (zero1 && inf2);
        let inputs = [accumulator, first, second];
        let signaling = inputs
            .iter()
            .copied()
            .find(|bits| Self::x86_simd_fp_is_snan(*bits, format));
        let nan = signaling.or_else(|| {
            inputs
                .iter()
                .copied()
                .find(|bits| Self::x86_simd_fp_is_nan(*bits, format))
        });
        if let Some(nan) = nan {
            // A quiet addend cannot suppress invalid 0*infinity (FPMulAdd).
            let invalid_override = Self::x86_simd_fp_is_nan(accumulator, format)
                && !Self::x86_simd_fp_is_snan(accumulator, format)
                && invalid_product;
            status |= u32::from(signaling.is_some() || invalid_override);
            return (
                if control & (1 << 25) != 0 || invalid_override {
                    canonical_nan
                } else {
                    nan | quiet
                },
                status,
            );
        }
        let negative_product = (first ^ second) & sign != 0;
        let negative_accumulator = accumulator & sign != 0;
        let infinite_accumulator = accumulator & !sign == exponent;
        if invalid_product
            || (infinite_accumulator && (inf1 || inf2) && negative_accumulator != negative_product)
        {
            return (canonical_nan, status | 1);
        }
        if infinite_accumulator {
            return (accumulator, status);
        }
        if inf1 || inf2 {
            return ((if negative_product { sign } else { 0 }) | exponent, status);
        }
        let mode = rounding(control);
        let a = Self::x86_simd_fp_decode(first, format);
        let b = Self::x86_simd_fp_decode(second, format);
        let c = Self::x86_simd_fp_decode(accumulator, format);
        let product = a.significand * b.significand;
        if product == 0 && c.significand == 0 {
            let negative = if negative_accumulator == negative_product {
                negative_product
            } else {
                mode == FpRoundMode::RoundDown
            };
            return (if negative { sign } else { 0 }, status);
        }
        let product_exponent = a.exponent + b.exponent;
        let (negative, magnitude, exponent, sticky) = if product == 0 {
            (negative_accumulator, c.significand, c.exponent, false)
        } else if c.significand == 0 {
            (negative_product, product, product_exponent, false)
        } else if double {
            Self::x86_f64_fma_add_scaled(
                negative_product,
                product,
                product_exponent,
                negative_accumulator,
                c.significand,
                c.exponent,
            )
        } else {
            // A binary32 product has at most 48 bits; 78 guard bits retain
            // every cancellation-relevant bit within a signed 128-bit sum.
            hr_add_scaled(
                negative_product,
                product,
                product_exponent,
                negative_accumulator,
                c.significand,
                c.exponent,
                78,
            )
        };
        if magnitude == 0 && !sticky {
            return (
                if mode == FpRoundMode::RoundDown {
                    sign
                } else {
                    0
                },
                status,
            );
        }
        let (bits, raised) = Self::arm_round_arithmetic(
            negative, magnitude, exponent, sticky, format, mode, control,
        );
        (bits, status | raised)
    }
}
