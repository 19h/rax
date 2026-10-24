//! Arm reciprocal estimates and A32 nonfused refinement steps for binary32.
use super::{ArmFpOperation, SmirInterpreter};
impl SmirInterpreter {
    pub(crate) fn arm_recip_estimate(mut value: u32) -> u32 {
        debug_assert!((256..512).contains(&value));
        value = value * 2 + 1;
        (((1u32 << 19) / value) + 1) >> 1
    }
    pub(crate) fn arm_recip_sqrt_estimate(mut value: u32) -> u32 {
        debug_assert!((128..512).contains(&value));
        value = if value < 256 {
            value * 2 + 1
        } else {
            ((value & !1) + 1) * 2
        };
        let value = u64::from(value);
        let mut result = 512;
        while value * (result + 1) * (result + 1) < (1u64 << 28) {
            result += 1;
        }
        ((result + 1) >> 1) as u32
    }
    pub(crate) fn arm_eval_recip_estimate(mut raw: u32, sqrt: bool, control: u32) -> (u32, u32) {
        let sign = raw & 0x80000000;
        let mut status = 0;
        if raw & 0x7f800000 == 0 && raw & 0x007fffff != 0 && control & (1 << 24) != 0 {
            raw = sign;
            status |= 0x80;
        }
        let absolute = raw & 0x7fffffff;
        if absolute > 0x7f800000 {
            status |= u32::from(raw & 0x00400000 == 0);
            return (
                if control & (1 << 25) != 0 {
                    0x7fc00000
                } else {
                    raw | 0x00400000
                },
                status,
            );
        }
        if absolute == 0 {
            return (sign | 0x7f800000, status | 2);
        }
        if sqrt && sign != 0 {
            return (0x7fc00000, status | 1);
        }
        if absolute == 0x7f800000 {
            return (sign, status);
        }
        if !sqrt && absolute < 0x00200000 {
            let infinity = match (control >> 22) & 3 {
                0 => true,
                1 => sign == 0,
                2 => sign != 0,
                _ => false,
            };
            return (
                sign | if infinity { 0x7f800000 } else { 0x7f7fffff },
                status | 0x14,
            );
        }
        if !sqrt && control & (1 << 24) != 0 && absolute >= 0x7e800000 {
            return (sign, status | 8);
        }
        let mut exponent = ((raw >> 23) & 0xff) as i32;
        let mut fraction = u64::from(raw & 0x007fffff) << 29;
        if sqrt {
            if exponent == 0 {
                while fraction & (1 << 51) == 0 {
                    fraction <<= 1;
                    exponent -= 1;
                }
                fraction = (fraction << 1) & 0x000fffffffffffff;
            }
            let scaled = if exponent & 1 == 0 {
                0x100 | ((fraction >> 44) & 0xff) as u32
            } else {
                0x80 | ((fraction >> 45) & 0x7f) as u32
            };
            let result_exp = (380 - exponent) / 2;
            let estimate = Self::arm_recip_sqrt_estimate(scaled);
            ((result_exp as u32) << 23 | (estimate & 0xff) << 15, status)
        } else {
            if exponent == 0 {
                if fraction & (1 << 51) == 0 {
                    exponent = -1;
                    fraction = (fraction << 2) & 0x000fffffffffffff;
                } else {
                    fraction = (fraction << 1) & 0x000fffffffffffff;
                }
            }
            let estimate = Self::arm_recip_estimate(0x100 | ((fraction >> 44) & 0xff) as u32);
            let mut result_exp = 253 - exponent;
            let mut fraction = u64::from(estimate & 0xff) << 44;
            if result_exp == 0 {
                fraction = (1 << 51) | fraction >> 1;
            } else if result_exp == -1 {
                fraction = (1 << 50) | fraction >> 2;
                result_exp = 0;
            }
            (
                sign | ((result_exp as u32) << 23) | ((fraction >> 29) as u32 & 0x007fffff),
                status,
            )
        }
    }
    pub(crate) fn arm_eval_recip_step(first: u32, second: u32, sqrt: bool) -> (u32, u32) {
        let (bits, status) = Self::arm_eval_recip_step_format(
            u64::from(first),
            u64::from(second),
            sqrt,
            super::X86_SIMD_F32,
            3 << 24,
        );
        (bits as u32, status)
    }
    pub(crate) fn arm_eval_half_recip_step(
        first: u16,
        second: u16,
        sqrt: bool,
        control: u32,
    ) -> (u16, u32) {
        let standard = (control & (1 << 19)) | (3 << 24);
        let (bits, status) = Self::arm_eval_recip_step_format(
            u64::from(first),
            u64::from(second),
            sqrt,
            super::X86_SIMD_F16,
            super::arm_format::half_control(standard),
        );
        (bits as u16, status)
    }
    fn arm_eval_recip_step_format(
        mut first: u64,
        mut second: u64,
        sqrt: bool,
        format: super::X86SimdFpFormat,
        control: u32,
    ) -> (u64, u32) {
        let (sign, exponent, fraction, quiet) = Self::x86_simd_fp_masks(format);
        let half = format.total_bits == 16;
        let mut status = 0;
        if control & (1 << 24) != 0 {
            for raw in [&mut first, &mut second] {
                if *raw & exponent == 0 && *raw & fraction != 0 {
                    *raw &= sign;
                    if !half {
                        status |= 0x80;
                    }
                }
            }
        }
        if Self::x86_simd_fp_is_nan(first, format) || Self::x86_simd_fp_is_nan(second, format) {
            let signaling = Self::x86_simd_fp_is_snan(first, format)
                || Self::x86_simd_fp_is_snan(second, format);
            return (exponent | quiet, status | u32::from(signaling));
        }
        let a = first & !sign;
        let b = second & !sign;
        let product = if (a == exponent && b == 0) || (a == 0 && b == exponent) {
            0
        } else {
            let (bits, raised) = Self::arm_eval_arithmetic_format(
                ArmFpOperation::Multiply,
                first,
                second,
                format,
                control,
            );
            status |= raised;
            bits
        };
        let (result, raised) = if sqrt {
            // The product is rounded before the single halved subtraction.
            // FPMulAdd(1.5,-product,0.5) is exactly FPHalvedSub(3,product).
            let (one_point_five, one_half) = if half {
                (0x3e00, 0x3800)
            } else {
                (0x3fc00000, 0x3f000000)
            };
            Self::arm_eval_fused_format(one_point_five, product ^ sign, one_half, format, control)
        } else {
            let two = if half { 0x4000 } else { 0x40000000 };
            Self::arm_eval_arithmetic_format(
                ArmFpOperation::Subtract,
                two,
                product,
                format,
                control,
            )
        };
        (result, status | raised)
    }
}
