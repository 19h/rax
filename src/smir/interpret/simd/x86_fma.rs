//! Exact x86 fused multiply-add arithmetic boundaries.

use super::*;
use std::cmp::Ordering;

impl SmirInterpreter {
    /// Align and add an exact binary64 product and accumulator without
    /// overflowing the fixed-width intermediate. Cancellation-capable inputs
    /// are first aligned exactly. When their exponent separation makes an
    /// exact `u128` representation impossible, the larger-scale operand is
    /// retained with enough low guard bits for binary64 rounding and every
    /// discarded bit is represented by `sticky`.
    fn x86_f64_fma_add_scaled(
        product_negative: bool,
        product_magnitude: u128,
        product_exponent: i32,
        accumulator_negative: bool,
        accumulator_magnitude: u128,
        accumulator_exponent: i32,
    ) -> (bool, u128, i32, bool) {
        debug_assert_ne!(product_magnitude, 0);
        debug_assert_ne!(accumulator_magnitude, 0);

        let exact_exponent = product_exponent.min(accumulator_exponent);
        let product_shift = (product_exponent - exact_exponent) as u32;
        let accumulator_shift = (accumulator_exponent - exact_exponent) as u32;
        let shift_exact = |magnitude: u128, shift: u32| {
            let bits = u128::BITS - magnitude.leading_zeros();
            bits.checked_add(shift)
                .is_some_and(|width| width <= u128::BITS)
                .then(|| magnitude << shift)
        };

        if let (Some(product), Some(accumulator)) = (
            shift_exact(product_magnitude, product_shift),
            shift_exact(accumulator_magnitude, accumulator_shift),
        ) {
            if product_negative == accumulator_negative {
                if let Some(magnitude) = product.checked_add(accumulator) {
                    return (product_negative, magnitude, exact_exponent, false);
                }
            } else {
                return match product.cmp(&accumulator) {
                    Ordering::Greater => (
                        product_negative,
                        product - accumulator,
                        exact_exponent,
                        false,
                    ),
                    Ordering::Less => (
                        accumulator_negative,
                        accumulator - product,
                        exact_exponent,
                        false,
                    ),
                    Ordering::Equal => (false, 0, exact_exponent, false),
                };
            }
        }

        // A binary64 product has at most 106 significant bits, so shifting it
        // by 21 leaves 127 bits in signed `i128`. An accumulator has at most
        // 53 significant bits; shifting it by 73 leaves 126 bits plus one bit
        // of same-sign addition headroom. Reaching this fallback proves the
        // other term is sufficiently scale-separated that it cannot cancel
        // any discarded leading precision.
        let guard = if product_exponent >= accumulator_exponent {
            21
        } else {
            73
        };
        hr_add_scaled(
            product_negative,
            product_magnitude,
            product_exponent,
            accumulator_negative,
            accumulator_magnitude,
            accumulator_exponent,
            guard,
        )
    }

    /// Exact non-NaN fused multiply-add. The caller owns x86 FMA NaN
    /// source priority and denormal-operand status; this core handles invalid
    /// zero/infinity combinations and performs one final format rounding.
    pub(crate) fn x86_simd_fp_fma_non_nan(
        first: u64,
        second: u64,
        accumulator: u64,
        format: X86SimdFpFormat,
        mode: FpRoundMode,
        mxcsr: u32,
    ) -> X86SimdFpResult {
        debug_assert!(!Self::x86_simd_fp_is_nan(first, format));
        debug_assert!(!Self::x86_simd_fp_is_nan(second, format));
        debug_assert!(!Self::x86_simd_fp_is_nan(accumulator, format));
        let first_inf = Self::x86_simd_fp_is_infinite(first, format);
        let second_inf = Self::x86_simd_fp_is_infinite(second, format);
        let accumulator_inf = Self::x86_simd_fp_is_infinite(accumulator, format);
        let first_zero = Self::x86_simd_fp_is_zero(first, format);
        let second_zero = Self::x86_simd_fp_is_zero(second, format);
        let accumulator_zero = Self::x86_simd_fp_is_zero(accumulator, format);
        let (sign_mask, exponent_mask, _, _) = Self::x86_simd_fp_masks(format);
        let product_negative = (first ^ second) & sign_mask != 0;
        let accumulator_negative = accumulator & sign_mask != 0;

        if (first_inf && second_zero) || (second_inf && first_zero) {
            return X86SimdFpResult {
                bits: Self::x86_simd_fp_indefinite(format),
                status: 1,
            };
        }
        if first_inf || second_inf {
            if accumulator_inf && product_negative != accumulator_negative {
                return X86SimdFpResult {
                    bits: Self::x86_simd_fp_indefinite(format),
                    status: 1,
                };
            }
            return X86SimdFpResult {
                bits: (if product_negative { sign_mask } else { 0 }) | exponent_mask,
                status: 0,
            };
        }
        if accumulator_inf {
            return X86SimdFpResult {
                bits: accumulator & (sign_mask | exponent_mask),
                status: 0,
            };
        }
        if first_zero || second_zero {
            if accumulator_zero {
                let negative = if product_negative == accumulator_negative {
                    product_negative
                } else {
                    mode == FpRoundMode::RoundDown
                };
                return X86SimdFpResult {
                    bits: if negative { sign_mask } else { 0 },
                    status: 0,
                };
            }
            return X86SimdFpResult {
                bits: accumulator,
                status: 0,
            };
        }

        let a = Self::x86_simd_fp_decode(first, format);
        let b = Self::x86_simd_fp_decode(second, format);
        let product_magnitude = a.significand * b.significand;
        let product_exponent = a.exponent + b.exponent;
        let (negative, magnitude, exponent, sticky) = if accumulator_zero {
            (product_negative, product_magnitude, product_exponent, false)
        } else {
            let c = Self::x86_simd_fp_decode(accumulator, format);
            if format.total_bits == 64 {
                Self::x86_f64_fma_add_scaled(
                    product_negative,
                    product_magnitude,
                    product_exponent,
                    accumulator_negative,
                    c.significand,
                    c.exponent,
                )
            } else {
                // Binary16's complete product/accumulator exponent range is
                // below 64 bits; binary32 uses the proven 78-bit guard from
                // the existing exact soft-float FMA core.
                hr_add_scaled(
                    product_negative,
                    product_magnitude,
                    product_exponent,
                    accumulator_negative,
                    c.significand,
                    c.exponent,
                    if format.total_bits == 32 {
                        HR_SF_GUARD
                    } else {
                        64
                    },
                )
            }
        };
        if magnitude == 0 && !sticky {
            return X86SimdFpResult {
                bits: if mode == FpRoundMode::RoundDown {
                    sign_mask
                } else {
                    0
                },
                status: 0,
            };
        }
        // AVX512-FP16 processes gradual underflow regardless of MXCSR.FTZ;
        // binary32/binary64 users of this exact core retain architectural FTZ.
        let round_mxcsr = if format.total_bits == 16 {
            mxcsr & !(1 << 15)
        } else {
            mxcsr
        };
        Self::x86_simd_fp_round_exact(
            negative,
            magnitude,
            exponent,
            sticky,
            format,
            mode,
            round_mxcsr,
        )
    }

    /// One binary32/binary64 x86 FMA boundary. `first`, `second`, and
    /// `accumulator` are already in mnemonic arithmetic order, which is also
    /// Intel's NaN propagation priority. DAZ is applied before classification,
    /// FTZ is applied by the final rounding core, and arithmetic sign changes
    /// do not alter a propagated NaN payload.
    pub(crate) fn x86_fma_boundary(
        first: u64,
        second: u64,
        accumulator: u64,
        format: X86SimdFpFormat,
        negate_product: bool,
        negate_accumulator: bool,
        mode: FpRoundMode,
        mxcsr: u32,
    ) -> X86SimdFpResult {
        debug_assert!(matches!(format.total_bits, 32 | 64));
        let first = Self::x86_simd_fp_apply_daz(first, format, mxcsr);
        let second = Self::x86_simd_fp_apply_daz(second, format, mxcsr);
        let accumulator = Self::x86_simd_fp_apply_daz(accumulator, format, mxcsr);
        let sources = [first.bits, second.bits, accumulator.bits];
        let mut status = first.status | second.status | accumulator.status;
        if sources
            .iter()
            .any(|bits| Self::x86_simd_fp_is_snan(*bits, format))
        {
            status |= 1;
        }
        if let Some(nan) = sources
            .iter()
            .copied()
            .find(|bits| Self::x86_simd_fp_is_nan(*bits, format))
        {
            return X86SimdFpResult {
                bits: Self::x86_simd_fp_quiet_nan(nan, format),
                status,
            };
        }
        let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
        let first = if negate_product {
            first.bits ^ sign
        } else {
            first.bits
        };
        let accumulator = if negate_accumulator {
            accumulator.bits ^ sign
        } else {
            accumulator.bits
        };
        let computed =
            Self::x86_simd_fp_fma_non_nan(first, second.bits, accumulator, format, mode, mxcsr);
        X86SimdFpResult {
            bits: computed.bits,
            status: status | computed.status,
        }
    }

    /// Binary32 compatibility wrapper for AVX512_4FMAPS, whose accumulator is
    /// never negated by the instruction family.
    pub(crate) fn x86_f32_fma_boundary(
        first: u64,
        second: u64,
        accumulator: u64,
        negate_product: bool,
        mode: FpRoundMode,
        mxcsr: u32,
    ) -> X86SimdFpResult {
        Self::x86_fma_boundary(
            first,
            second,
            accumulator,
            X86_SIMD_F32,
            negate_product,
            false,
            mode,
            mxcsr,
        )
    }

    /// One architectural FP16 FMA boundary for the complex arithmetic
    /// instructions. NaN priority follows the written operand order, the
    /// optional arithmetic negation does not alter a propagated NaN payload,
    /// and AVX512-FP16 ignores both DAZ and FTZ.
    pub(crate) fn x86_fp16_fma_boundary(
        first: u64,
        second: u64,
        accumulator: u64,
        negate_first: bool,
        mode: FpRoundMode,
        mxcsr: u32,
    ) -> X86SimdFpResult {
        let sources = [first, second, accumulator];
        let mut status = if sources
            .iter()
            .any(|bits| Self::x86_simd_fp_is_denormal(*bits, X86_SIMD_F16))
        {
            1 << 1
        } else {
            0
        };
        if sources
            .iter()
            .any(|bits| Self::x86_simd_fp_is_snan(*bits, X86_SIMD_F16))
        {
            status |= 1;
        }
        if let Some(nan) = sources
            .iter()
            .copied()
            .find(|bits| Self::x86_simd_fp_is_nan(*bits, X86_SIMD_F16))
        {
            return X86SimdFpResult {
                bits: Self::x86_simd_fp_quiet_nan(nan, X86_SIMD_F16),
                status,
            };
        }
        let first = if negate_first { first ^ 0x8000 } else { first };
        let computed =
            Self::x86_simd_fp_fma_non_nan(first, second, accumulator, X86_SIMD_F16, mode, mxcsr);
        X86SimdFpResult {
            bits: computed.bits,
            status: status | computed.status,
        }
    }
}
