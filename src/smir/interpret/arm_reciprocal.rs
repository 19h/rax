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
    pub(crate) fn arm_eval_recip_step(mut first: u32, mut second: u32, sqrt: bool) -> (u32, u32) {
        // A32 FPRecipStep/FPRSqrtStep use StandardFPSCRValue: nearest-even,
        // default NaN and input/output flushing, regardless of live FPSCR RM.
        let control = (1 << 24) | (1 << 25);
        let mut status = 0;
        for raw in [&mut first, &mut second] {
            if *raw & 0x7f800000 == 0 && *raw & 0x007fffff != 0 {
                *raw &= 0x80000000;
                status |= 0x80;
            }
        }
        let a = first & 0x7fffffff;
        let b = second & 0x7fffffff;
        if a > 0x7f800000 || b > 0x7f800000 {
            let signaling = (a > 0x7f800000 && first & 0x00400000 == 0)
                || (b > 0x7f800000 && second & 0x00400000 == 0);
            return (0x7fc00000, status | u32::from(signaling));
        }
        let product = if (a == 0x7f800000 && b == 0) || (a == 0 && b == 0x7f800000) {
            0
        } else {
            let (value, raised) = Self::arm_eval_arithmetic(
                ArmFpOperation::Multiply,
                u64::from(first),
                u64::from(second),
                false,
                control,
            );
            status |= raised;
            value
        };
        let (result, raised) = if sqrt {
            // FPHalvedSub(3, product) rounds (3-product)/2 once, *after*
            // the already-rounded product. FPMulAdd(1.5, -product, 0.5)
            // has exactly that value and the same special cases here.
            Self::arm_eval_fused(0x3fc00000, product ^ 0x80000000, 0x3f000000, false, control)
        } else {
            Self::arm_eval_arithmetic(
                ArmFpOperation::Subtract,
                0x40000000,
                product,
                false,
                control,
            )
        };
        (result as u32, status | raised)
    }
}
