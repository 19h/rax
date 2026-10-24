//! Arm scalar FPCompare and FPMaxNum/FPMinNum with bit-preserving classification.
use super::{SmirInterpreter, X86_SIMD_F32, X86_SIMD_F64, X86SimdFpFormat};
use std::cmp::Ordering;

fn ordered(first: u64, second: u64, sign: u64) -> Ordering {
    if first == second || (first & !sign == 0 && second & !sign == 0) {
        Ordering::Equal
    } else if (first ^ second) & sign != 0 {
        if first & sign != 0 {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else if first & sign == 0 {
        first.cmp(&second)
    } else {
        second.cmp(&first)
    }
}

impl SmirInterpreter {
    fn arm_unpack_pair(
        first: &mut u64,
        second: &mut u64,
        format: X86SimdFpFormat,
        control: u32,
    ) -> u32 {
        let mut status = 0;
        let (sign, exponent, fraction, _) = Self::x86_simd_fp_masks(format);
        if control & (1 << 24) != 0 {
            for input in [first, second] {
                if *input & exponent == 0 && *input & fraction != 0 {
                    *input &= sign;
                    status |= if format.total_bits == 16 { 0 } else { 1 << 7 };
                }
            }
        }
        status
    }

    pub(crate) fn arm_eval_minmax(
        first: u64,
        second: u64,
        double: bool,
        maximum: bool,
        number: bool,
        control: u32,
    ) -> (u64, u32) {
        Self::arm_eval_minmax_format(
            first,
            second,
            if double { X86_SIMD_F64 } else { X86_SIMD_F32 },
            maximum,
            number,
            control,
        )
    }
    pub(crate) fn arm_eval_half_minmax(
        first: u16,
        second: u16,
        maximum: bool,
        number: bool,
        control: u32,
    ) -> (u16, u32) {
        let (bits, status) = Self::arm_eval_minmax_format(
            u64::from(first),
            u64::from(second),
            super::X86_SIMD_F16,
            maximum,
            number,
            super::arm_format::half_control(control),
        );
        (bits as u16, status)
    }
    pub(super) fn arm_eval_minmax_format(
        mut first: u64,
        mut second: u64,
        format: super::X86SimdFpFormat,
        maximum: bool,
        number: bool,
        control: u32,
    ) -> (u64, u32) {
        let mut status = Self::arm_unpack_pair(&mut first, &mut second, format, control);
        let (sign, exponent, fraction, quiet) = Self::x86_simd_fp_masks(format);
        let nan1 = Self::x86_simd_fp_is_nan(first, format);
        let nan2 = Self::x86_simd_fp_is_nan(second, format);
        let snan1 = Self::x86_simd_fp_is_snan(first, format);
        let snan2 = Self::x86_simd_fp_is_snan(second, format);
        // A single quiet NaN is replaced by the non-selecting infinity;
        // signaling NaNs still propagate through FPProcessNaNs and raise IOC.
        let quiet1 = nan1 && !snan1;
        let quiet2 = nan2 && !snan2;
        let identity = exponent | if maximum { sign } else { 0 };
        if number && quiet1 && !quiet2 {
            first = identity;
        }
        if number && quiet2 && !quiet1 {
            second = identity;
        }
        let inputs = [first, second];
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
                    exponent | quiet
                } else {
                    nan | quiet
                },
                status,
            );
        }
        if first & !sign == 0 && second & !sign == 0 {
            return (
                if maximum {
                    (first & second) & sign
                } else {
                    (first | second) & sign
                },
                status,
            );
        }
        let order = ordered(first, second, sign);
        let selected =
            if (maximum && order == Ordering::Greater) || (!maximum && order == Ordering::Less) {
                first
            } else {
                second
            };
        if selected & exponent == 0 && selected & fraction != 0 && control & (1 << 11) != 0 {
            status |= 1 << 3;
        }
        (selected, status)
    }

    pub(crate) fn arm_eval_compare(
        first: u64,
        second: u64,
        double: bool,
        signal_all_nans: bool,
        control: u32,
    ) -> (u32, u32) {
        Self::arm_eval_compare_format(
            first,
            second,
            if double { X86_SIMD_F64 } else { X86_SIMD_F32 },
            signal_all_nans,
            control,
        )
    }
    pub(crate) fn arm_eval_half_compare(
        first: u16,
        second: u16,
        signal_all_nans: bool,
        control: u32,
    ) -> (u32, u32) {
        Self::arm_eval_compare_format(
            u64::from(first),
            u64::from(second),
            super::X86_SIMD_F16,
            signal_all_nans,
            super::arm_format::half_control(control),
        )
    }
    pub(super) fn arm_eval_compare_format(
        mut first: u64,
        mut second: u64,
        format: super::X86SimdFpFormat,
        signal_all_nans: bool,
        control: u32,
    ) -> (u32, u32) {
        let mut status = Self::arm_unpack_pair(&mut first, &mut second, format, control);
        if Self::x86_simd_fp_is_nan(first, format) || Self::x86_simd_fp_is_nan(second, format) {
            status |= u32::from(
                signal_all_nans
                    || Self::x86_simd_fp_is_snan(first, format)
                    || Self::x86_simd_fp_is_snan(second, format),
            );
            return (3 << 28, status);
        }
        let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
        let flags = match ordered(first, second, sign) {
            Ordering::Equal => 6,
            Ordering::Less => 8,
            Ordering::Greater => 2,
        };
        (flags << 28, status)
    }
}
