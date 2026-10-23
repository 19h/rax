//! Arm FPAdd/FPSub/FPMul/FPDiv/FPSqrt and FPRoundBase for binary32/binary64.
//! Integer significands and explicit guest rounding leave host FP state untouched.

use super::{
    FpRoundMode, SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64, X86SimdFpFormat, hr_add_scaled,
};

#[derive(Clone, Copy)]
pub(crate) enum ArmFpOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
    SquareRoot,
}

pub(super) fn rounding(control: u32) -> FpRoundMode {
    match (control >> 22) & 3 {
        0 => FpRoundMode::RoundNearest,
        1 => FpRoundMode::RoundUp,
        2 => FpRoundMode::RoundDown,
        _ => FpRoundMode::RoundTowardZero,
    }
}

impl SmirInterpreter {
    pub(crate) fn arm_eval_arithmetic(
        operation: ArmFpOperation,
        mut first: u64,
        mut second: u64,
        double: bool,
        control: u32,
    ) -> (u64, u32) {
        let format = if double { X86_SIMD_F64 } else { X86_SIMD_F32 };
        let (sign, exponent, fraction, quiet) = Self::x86_simd_fp_masks(format);
        let canonical_nan = exponent | quiet;
        let unary = matches!(operation, ArmFpOperation::SquareRoot);
        let mut status = 0;
        // Arm FPUnpack raises InputDenorm only when an input is flushed.
        if control & (1 << 24) != 0 {
            for input in [&mut first, &mut second]
                .into_iter()
                .take(if unary { 1 } else { 2 })
            {
                if *input & exponent == 0 && *input & fraction != 0 {
                    *input &= sign;
                    status |= 1 << 7;
                }
            }
        }
        let inputs = [first, second];
        let inputs = &inputs[..if unary { 1 } else { 2 }];
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
            status |= u32::from(signaling.is_some());
            return (
                if control & (1 << 25) != 0 {
                    canonical_nan
                } else {
                    nan | quiet
                },
                status,
            );
        }
        let mode = rounding(control);
        if matches!(operation, ArmFpOperation::Subtract) {
            second ^= sign;
        }
        let negative = (first ^ second) & sign != 0;
        let signed = if negative { sign } else { 0 };
        let inf1 = first & !sign == exponent;
        let inf2 = second & !sign == exponent;
        let zero1 = first & !sign == 0;
        let zero2 = second & !sign == 0;
        let invalid = match operation {
            ArmFpOperation::Add | ArmFpOperation::Subtract => inf1 && inf2 && negative,
            ArmFpOperation::Multiply => (inf1 && zero2) || (zero1 && inf2),
            ArmFpOperation::Divide => (inf1 && inf2) || (zero1 && zero2),
            ArmFpOperation::SquareRoot => first & sign != 0 && !zero1,
        };
        if invalid {
            return (canonical_nan, status | 1);
        }
        let result = match operation {
            ArmFpOperation::Add | ArmFpOperation::Subtract => {
                if inf1 || inf2 {
                    return (if inf1 { first } else { second }, status);
                }
                if zero1 && zero2 {
                    let negative = if (first ^ second) & sign == 0 {
                        first & sign != 0
                    } else {
                        mode == FpRoundMode::RoundDown
                    };
                    return (if negative { sign } else { 0 }, status);
                }
                let a = Self::x86_simd_fp_decode(first, format);
                let b = Self::x86_simd_fp_decode(second, format);
                let guard = if double { 72 } else { 100 };
                let (negative, magnitude, exponent, sticky) = hr_add_scaled(
                    a.negative,
                    a.significand,
                    a.exponent,
                    b.negative,
                    b.significand,
                    b.exponent,
                    guard,
                );
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
                Self::arm_round_arithmetic(
                    negative, magnitude, exponent, sticky, format, mode, control,
                )
            }
            ArmFpOperation::Multiply => {
                if inf1 || inf2 {
                    return (signed | exponent, status);
                }
                if zero1 || zero2 {
                    return (signed, status);
                }
                let a = Self::x86_simd_fp_decode(first, format);
                let b = Self::x86_simd_fp_decode(second, format);
                Self::arm_round_arithmetic(
                    negative,
                    a.significand * b.significand,
                    a.exponent + b.exponent,
                    false,
                    format,
                    mode,
                    control,
                )
            }
            ArmFpOperation::Divide => {
                if inf1 {
                    return (signed | exponent, status);
                }
                if inf2 {
                    return (signed, status);
                }
                if zero2 {
                    return (signed | exponent, status | 2);
                }
                if zero1 {
                    return (signed, status);
                }
                let mut a = Self::x86_simd_fp_decode(first, format);
                let mut b = Self::x86_simd_fp_decode(second, format);
                let a_shift =
                    format.fraction_bits as i32 - (127 - a.significand.leading_zeros() as i32);
                let b_shift =
                    format.fraction_bits as i32 - (127 - b.significand.leading_zeros() as i32);
                a.significand <<= a_shift;
                a.exponent -= a_shift;
                b.significand <<= b_shift;
                b.exponent -= b_shift;
                let guard = if double { 74 } else { 100 };
                let numerator = a.significand << guard;
                Self::arm_round_arithmetic(
                    negative,
                    numerator / b.significand,
                    a.exponent - b.exponent - guard,
                    numerator % b.significand != 0,
                    format,
                    mode,
                    control,
                )
            }
            ArmFpOperation::SquareRoot => {
                if inf1 || zero1 {
                    return (first, status);
                }
                let mut a = Self::x86_simd_fp_decode(first, format);
                let shift =
                    format.fraction_bits as i32 - (127 - a.significand.leading_zeros() as i32);
                a.significand <<= shift;
                a.exponent -= shift;
                if a.exponent & 1 != 0 {
                    a.significand <<= 1;
                    a.exponent -= 1;
                }
                let radicand = a.significand << 64;
                let root = Self::x86_x87_integer_sqrt(radicand);
                Self::arm_round_arithmetic(
                    false,
                    root,
                    a.exponent / 2 - 32,
                    root * root != radicand,
                    format,
                    mode,
                    control,
                )
            }
        };
        (result.0, status | result.1)
    }

    pub(super) fn arm_round_arithmetic(
        negative: bool,
        magnitude: u128,
        exponent: i32,
        sticky: bool,
        format: X86SimdFpFormat,
        mode: FpRoundMode,
        control: u32,
    ) -> (u64, u32) {
        let (sign_mask, exponent_mask, fraction_mask, _) = Self::x86_simd_fp_masks(format);
        let sign = if negative { sign_mask } else { 0 };
        if magnitude == 0 {
            debug_assert!(!sticky);
            return (sign, 0);
        }
        let msb = 127 - magnitude.leading_zeros() as i32;
        let unbiased = msb + exponent;
        let minimum = 1 - format.bias;
        let subnormal_exponent = minimum - format.fraction_bits as i32;
        let tiny = unbiased < minimum;
        // FPRoundBase flushes before rounding, sets UFC and does not add IXC.
        if tiny && control & (1 << 24) != 0 {
            return (sign, 1 << 3);
        }
        let target_exponent = if tiny {
            subnormal_exponent
        } else {
            unbiased - format.fraction_bits as i32
        };
        let (rounded, rounded_exponent, inexact) = Self::x86_simd_fp_round_shift(
            magnitude,
            exponent,
            sticky,
            target_exponent - exponent,
            mode,
            negative,
        );
        let mut status = if inexact { 1 << 4 } else { 0 };
        if tiny && (inexact || control & (1 << 11) != 0) {
            status |= 1 << 3;
        }
        if rounded == 0 {
            return (sign, status);
        }
        let rounded_msb = 127 - rounded.leading_zeros() as i32;
        let rounded_unbiased = rounded_msb + rounded_exponent;
        if rounded_unbiased > format.bias {
            let to_infinity = match mode {
                FpRoundMode::RoundNearest => true,
                FpRoundMode::RoundUp => !negative,
                FpRoundMode::RoundDown => negative,
                FpRoundMode::RoundTowardZero => false,
                _ => unreachable!(),
            };
            let bits = if to_infinity {
                sign | exponent_mask
            } else {
                sign | (exponent_mask - (1u64 << format.fraction_bits)) | fraction_mask
            };
            return (bits, status | (1 << 2) | (1 << 4));
        }
        let bits = if rounded_unbiased < minimum {
            let fraction = if rounded_exponent >= subnormal_exponent {
                rounded << (rounded_exponent - subnormal_exponent)
            } else {
                rounded >> (subnormal_exponent - rounded_exponent)
            };
            sign | (fraction as u64 & fraction_mask)
        } else {
            let shift = rounded_msb - format.fraction_bits as i32;
            let significand = if shift >= 0 {
                rounded >> shift
            } else {
                rounded << -shift
            };
            sign | (((rounded_unbiased + format.bias) as u64) << format.fraction_bits)
                | (significand as u64 & fraction_mask)
        };
        (bits, status)
    }
}
