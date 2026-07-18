//! Scalar x86 operation interpretation

use crate::smir::interpret::*;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86OpHint, X86ThreeDNowKind, X86X87ArithmeticDestination,
    X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind,
    X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};

impl SmirInterpreter {

    pub(crate) fn x86_rcl(val: u64, count: u64, carry_in: bool, width: OpWidth) -> (u64, bool, u64) {
        let bits = width.bits() as u64;
        let cmask = if bits == 64 { 0x3F } else { 0x1F };
        let effective = (count & cmask) % (bits + 1);
        let mut result = val & width.mask();
        let mut carry = carry_in;

        for _ in 0..effective {
            let msb = ((result >> (bits - 1)) & 1) != 0;
            result = ((result << 1) | u64::from(carry)) & width.mask();
            carry = msb;
        }

        (result, carry, effective)
    }


    pub(crate) fn x86_rcr(val: u64, count: u64, carry_in: bool, width: OpWidth) -> (u64, bool, u64) {
        let bits = width.bits() as u64;
        let cmask = if bits == 64 { 0x3F } else { 0x1F };
        let effective = (count & cmask) % (bits + 1);
        let mut result = val & width.mask();
        let mut carry = carry_in;

        for _ in 0..effective {
            let lsb = (result & 1) != 0;
            result = (result >> 1) | (u64::from(carry) << (bits - 1));
            carry = lsb;
        }

        (result & width.mask(), carry, effective)
    }


    pub(crate) fn x86_hardware_random(width: OpWidth, _seed: bool) -> (u64, bool) {
        let bytes = (width.bits() / 8) as usize;
        let mut value = 0u64;
        #[cfg(unix)]
        {
            use std::io::Read;

            // `/dev/urandom` is available across the supported Unix hosts and
            // avoids linking libc's `getentropy`, which is absent from the old
            // glibc sysroots used by several cross targets.
            let mut value_bytes = [0u8; 8];
            if std::fs::File::open("/dev/urandom")
                .and_then(|mut source| source.read_exact(&mut value_bytes[..bytes]))
                .is_ok()
            {
                value = u64::from_le_bytes(value_bytes);
                return (value & width.mask(), true);
            }
        }
        // Architecturally permitted source-not-ready outcome.
        (0, false)
    }


    pub(crate) fn x86_approx28_emulate(argument: u64, coefficients: &[(u64, u64, u64)]) -> (u64, i32) {
        let index = (argument >> 22) as usize;
        let fraction = argument & 0x3F_FFFF;
        let square = fraction * fraction >> 24;
        let (a_raw, b_raw, c_raw) = coefficients[index];
        let a = (a_raw << 20) as i64;
        let b = ((b_raw as i64) - 0x40_0000) << 3;
        let c = (c_raw << 3) as i64;
        let polynomial = a + b * fraction as i64 + c * square as i64;
        debug_assert!(polynomial > 0);
        let polynomial = polynomial as u64;
        let shift = polynomial.leading_zeros();
        debug_assert!(shift <= 11);
        (polynomial << shift, 1033 - shift as i32)
    }


    pub(crate) fn x86_approx28_finish(
        sign: u64,
        mut significand: u64,
        polynomial_exponent: i32,
        scale: i32,
        format: X86SimdFpFormat,
    ) -> u64 {
        match format.total_bits {
            32 => {
                let mut final_scale = scale + polynomial_exponent + 127 - 1023;
                final_scale = final_scale.max(-25);
                while final_scale <= 0 {
                    final_scale += 1;
                    significand = (significand >> 1) | (significand & 1);
                }

                let increment = 1u64 << 39;
                let mut round_state = (significand >> 39) & 3;
                if significand & (increment - 1) != 0 {
                    round_state |= 2;
                }
                if round_state == 3 {
                    significand = significand.wrapping_add(increment);
                }
                if significand < increment {
                    final_scale += 1;
                    significand = (significand >> 1) | (1u64 << 63);
                }

                let result = significand >> 40;
                if result < (1 << 23) {
                    sign
                } else if final_scale > 254 {
                    sign | 0x7F80_0000
                } else {
                    sign | ((final_scale as u64) << 23) | (result & 0x7F_FFFF)
                }
            }
            64 => {
                let mut final_scale = scale + polynomial_exponent;
                final_scale = final_scale.max(-54);
                while final_scale <= 0 {
                    final_scale += 1;
                    significand = (significand >> 1) | (significand & 1);
                }
                let result = significand >> 11;
                if result < (1u64 << 52) {
                    sign
                } else if final_scale > 2046 {
                    sign | 0x7FF0_0000_0000_0000
                } else {
                    sign | ((final_scale as u64) << 52) | (result & 0xF_FFFF_FFFF_FFFF)
                }
            }
            _ => unreachable!("VRCP28 supports FP32 and FP64"),
        }
    }


    pub(crate) fn x86_exp2_emulate_fraction(argument: u64) -> (u64, i32) {
        let index = (argument >> 22) as usize;
        let fraction = argument & 0x3F_FFFF;
        let square = fraction * fraction >> 24;
        let (a, b, c) = X86_EXP2_23_COEFFICIENTS[index];
        let polynomial = (a << 24) + (b << 9) * fraction + (c << 9) * square;
        let shift = polynomial.leading_zeros();
        debug_assert!(shift <= 11);
        (polynomial << shift, 1033 - shift as i32)
    }


    pub(crate) fn x86_int_to_fp_bits(
        &self,
        ctx: &SmirContext,
        value: i128,
        elem: VecElementType,
        mode: FpRoundMode,
    ) -> u64 {
        let mode = if mode == FpRoundMode::Dynamic {
            self.dynamic_fp_round_mode(ctx)
        } else {
            mode
        };
        let negative = value < 0;
        let magnitude = if negative {
            (-value) as u128
        } else {
            value as u128
        };
        let (frac_bits, exp_bits, bias) = match elem {
            VecElementType::F16 => (10, 5, 15),
            VecElementType::F32 => (23, 8, 127),
            VecElementType::F64 => (52, 11, 1023),
            _ => return 0,
        };
        Self::x86_int_magnitude_to_fp_bits(negative, magnitude, frac_bits, exp_bits, bias, mode)
    }


    pub(crate) fn x86_int_magnitude_to_fp_bits(
        negative: bool,
        magnitude: u128,
        frac_bits: u32,
        exp_bits: u32,
        bias: i32,
        mode: FpRoundMode,
    ) -> u64 {
        let sign_shift = exp_bits + frac_bits;
        let sign = if negative { 1u64 << sign_shift } else { 0 };
        if magnitude == 0 {
            return sign;
        }

        let precision = frac_bits + 1;
        let mut exponent = (u128::BITS - 1 - magnitude.leading_zeros()) as i32;
        let mut significand = if exponent as u32 <= frac_bits {
            magnitude << (frac_bits - exponent as u32)
        } else {
            let shift = exponent as u32 - frac_bits;
            let mut significand = magnitude >> shift;
            let remainder = magnitude & ((1u128 << shift) - 1);
            let increment = if remainder == 0 {
                false
            } else {
                match mode {
                    FpRoundMode::RoundNearest => {
                        let half = 1u128 << (shift - 1);
                        remainder > half || (remainder == half && significand & 1 != 0)
                    }
                    FpRoundMode::RoundNearestTiesAway => remainder >= 1u128 << (shift - 1),
                    FpRoundMode::RoundDown => negative,
                    FpRoundMode::RoundUp => !negative,
                    FpRoundMode::RoundTowardZero => false,
                    FpRoundMode::Dynamic => unreachable!(),
                }
            };
            if increment {
                significand += 1;
            }
            significand
        };

        if significand == 1u128 << precision {
            significand >>= 1;
            exponent += 1;
        }

        let max_exp = (1u64 << exp_bits) - 1;
        let biased = exponent + bias;
        if biased >= max_exp as i32 {
            let round_to_infinity = match mode {
                FpRoundMode::RoundNearest | FpRoundMode::RoundNearestTiesAway => true,
                FpRoundMode::RoundDown => negative,
                FpRoundMode::RoundUp => !negative,
                FpRoundMode::RoundTowardZero => false,
                FpRoundMode::Dynamic => unreachable!(),
            };
            if round_to_infinity {
                return sign | (max_exp << frac_bits);
            }
            return sign | ((max_exp - 1) << frac_bits) | ((1u64 << frac_bits) - 1);
        }

        let fraction = (significand & ((1u128 << frac_bits) - 1)) as u64;
        sign | ((biased as u64) << frac_bits) | fraction
    }
}
