//! 3DNow! instruction interpretation

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

    #[inline]
    pub(crate) fn x86_three_d_now_is_zero(bits: u32) -> bool {
        bits & 0x7F80_0000 == 0
    }


    #[inline]
    pub(crate) fn x86_three_d_now_is_unsupported(bits: u32) -> bool {
        bits & 0x7F80_0000 == 0x7F80_0000
    }


    /// 3DNow! treats every exponent-zero input as signed zero, including IEEE
    /// binary32 subnormals. Exponent-255 encodings are architecturally
    /// unsupported and are left intact for deterministic undefined handling.
    #[inline]
    pub(crate) fn x86_three_d_now_input_bits(bits: u32) -> u32 {
        if Self::x86_three_d_now_is_zero(bits) {
            bits & 0x8000_0000
        } else {
            bits
        }
    }


    /// Map a binary32 result into the 3DNow! numerical domain: subnormal
    /// results flush to signed zero and overflow saturates to signed maximum
    /// normal. A canonical quiet NaN is the deterministic result selected for
    /// architecturally undefined exponent-255 input combinations.
    #[inline]
    pub(crate) fn x86_three_d_now_finish(value: f32) -> u32 {
        let bits = value.to_bits();
        let sign = bits & 0x8000_0000;
        let exponent = bits & 0x7F80_0000;
        let fraction = bits & 0x007F_FFFF;
        if exponent == 0 {
            sign
        } else if exponent == 0x7F80_0000 {
            if fraction == 0 {
                sign | 0x7F7F_FFFF
            } else {
                0x7FC0_0000
            }
        } else {
            bits
        }
    }


    #[inline]
    pub(crate) fn x86_three_d_now_binary(
        first: u32,
        second: u32,
        operation: impl FnOnce(f32, f32) -> f32,
    ) -> u32 {
        let first = Self::x86_three_d_now_input_bits(first);
        let second = Self::x86_three_d_now_input_bits(second);
        if Self::x86_three_d_now_is_unsupported(first)
            || Self::x86_three_d_now_is_unsupported(second)
        {
            return 0x7FC0_0000;
        }
        Self::x86_three_d_now_finish(operation(f32::from_bits(first), f32::from_bits(second)))
    }


    #[inline]
    pub(crate) fn x86_three_d_now_multiply(first: u32, second: u32) -> u32 {
        let first = Self::x86_three_d_now_input_bits(first);
        let second = Self::x86_three_d_now_input_bits(second);
        if Self::x86_three_d_now_is_zero(first) || Self::x86_three_d_now_is_zero(second) {
            return (first ^ second) & 0x8000_0000;
        }
        Self::x86_three_d_now_binary(first, second, |a, b| a * b)
    }


    #[inline]
    pub(crate) fn x86_three_d_now_lane(value: u64, lane: u32) -> u32 {
        (value >> (lane * 32)) as u32
    }


    #[inline]
    pub(crate) fn x86_three_d_now_pack(low: u32, high: u32) -> u64 {
        u64::from(low) | (u64::from(high) << 32)
    }


    pub(crate) fn x86_three_d_now_float_to_int(bits: u32, minimum: i32, maximum: i32) -> i32 {
        let bits = Self::x86_three_d_now_input_bits(bits);
        if Self::x86_three_d_now_is_unsupported(bits) {
            // AMD defines exponent-255 sources as unsupported. Zero is the
            // interpreter's deterministic undefined result.
            return 0;
        }
        let value = f32::from_bits(bits);
        if value >= maximum as f32 + 1.0 {
            maximum
        } else if value <= minimum as f32 {
            minimum
        } else {
            value.trunc() as i32
        }
    }


    pub(crate) fn x86_three_d_now_min_max(first: u32, second: u32, maximum: bool) -> u32 {
        let first = Self::x86_three_d_now_input_bits(first);
        let second = Self::x86_three_d_now_input_bits(second);
        if Self::x86_three_d_now_is_unsupported(first)
            || Self::x86_three_d_now_is_unsupported(second)
        {
            return 0x7FC0_0000;
        }
        let first_value = f32::from_bits(first);
        let second_value = f32::from_bits(second);
        if first_value == 0.0 || second_value == 0.0 {
            let other = if first_value == 0.0 {
                (second, second_value)
            } else {
                (first, first_value)
            };
            if (maximum && other.1 > 0.0) || (!maximum && other.1 < 0.0) {
                other.0
            } else {
                0
            }
        } else if (maximum && first_value > second_value)
            || (!maximum && first_value < second_value)
        {
            first
        } else {
            second
        }
    }


    pub(crate) fn x86_three_d_now_reciprocal(bits: u32, square_root: bool) -> u32 {
        let bits = Self::x86_three_d_now_input_bits(bits);
        let sign = bits & 0x8000_0000;
        if Self::x86_three_d_now_is_zero(bits) {
            return sign | 0x7F7F_FFFF;
        }
        if Self::x86_three_d_now_is_unsupported(bits) {
            return 0x7FC0_0000;
        }
        let magnitude = f32::from_bits(bits & 0x7FFF_FFFF);
        let estimate = if square_root {
            1.0f32 / magnitude.sqrt()
        } else {
            1.0f32 / magnitude
        };
        Self::x86_three_d_now_finish(f32::from_bits(estimate.to_bits() | sign))
    }


    pub(crate) fn x86_three_d_now_iteration(first: u32, second: u32, kind: X86ThreeDNowKind) -> u32 {
        let first = Self::x86_three_d_now_input_bits(first);
        let second = Self::x86_three_d_now_input_bits(second);
        if Self::x86_three_d_now_is_zero(first) || Self::x86_three_d_now_is_zero(second) {
            return (first ^ second) & 0x8000_0000;
        }
        if Self::x86_three_d_now_is_unsupported(first)
            || Self::x86_three_d_now_is_unsupported(second)
        {
            return 0x7FC0_0000;
        }
        let first = f32::from_bits(first);
        let second = f32::from_bits(second);
        let value = match kind {
            X86ThreeDNowKind::PfRcpIt1 => 2.0f32 - first * second,
            X86ThreeDNowKind::PfRcpIt2 => first * second,
            X86ThreeDNowKind::PfRsqIt1 => (3.0f32 - first * second) * 0.5f32,
            _ => unreachable!("non-iteration 3DNow! kind"),
        };
        Self::x86_three_d_now_finish(value)
    }


    pub(crate) fn x86_three_d_now_eval(kind: X86ThreeDNowKind, first: u64, second: u64) -> u64 {
        use X86ThreeDNowKind::*;

        let first_low = Self::x86_three_d_now_lane(first, 0);
        let first_high = Self::x86_three_d_now_lane(first, 1);
        let second_low = Self::x86_three_d_now_lane(second, 0);
        let second_high = Self::x86_three_d_now_lane(second, 1);
        match kind {
            Pf2Iw => {
                let low = Self::x86_three_d_now_float_to_int(
                    second_low,
                    i16::MIN.into(),
                    i16::MAX.into(),
                );
                let high = Self::x86_three_d_now_float_to_int(
                    second_high,
                    i16::MIN.into(),
                    i16::MAX.into(),
                );
                Self::x86_three_d_now_pack(low as u32, high as u32)
            }
            Pf2Id => {
                let low = Self::x86_three_d_now_float_to_int(second_low, i32::MIN, i32::MAX);
                let high = Self::x86_three_d_now_float_to_int(second_high, i32::MIN, i32::MAX);
                Self::x86_three_d_now_pack(low as u32, high as u32)
            }
            Pi2Fw => {
                let low = (second as u16 as i16) as f32;
                let high = ((second >> 32) as u16 as i16) as f32;
                Self::x86_three_d_now_pack(low.to_bits(), high.to_bits())
            }
            Pi2Fd => Self::x86_three_d_now_pack(
                (second_low as i32 as f32).to_bits(),
                (second_high as i32 as f32).to_bits(),
            ),
            PfAcc | PfNAcc | PfPNAcc => {
                let low = Self::x86_three_d_now_binary(first_low, first_high, |a, b| {
                    if kind == PfAcc { a + b } else { a - b }
                });
                let high = Self::x86_three_d_now_binary(second_low, second_high, |a, b| {
                    if kind == PfNAcc { a - b } else { a + b }
                });
                Self::x86_three_d_now_pack(low, high)
            }
            PfAdd | PfSub | PfSubR | PfMul => {
                let evaluate = |a_bits, b_bits| match kind {
                    PfAdd => Self::x86_three_d_now_binary(a_bits, b_bits, |a, b| a + b),
                    PfSub => Self::x86_three_d_now_binary(a_bits, b_bits, |a, b| a - b),
                    PfSubR => Self::x86_three_d_now_binary(a_bits, b_bits, |a, b| b - a),
                    PfMul => Self::x86_three_d_now_multiply(a_bits, b_bits),
                    _ => unreachable!(),
                };
                let low = evaluate(first_low, second_low);
                let high = evaluate(first_high, second_high);
                Self::x86_three_d_now_pack(low, high)
            }
            PfCmpEq | PfCmpGe | PfCmpGt => {
                let compare = |a_bits: u32, b_bits: u32| {
                    let a_bits = Self::x86_three_d_now_input_bits(a_bits);
                    let b_bits = Self::x86_three_d_now_input_bits(b_bits);
                    if Self::x86_three_d_now_is_unsupported(a_bits)
                        || Self::x86_three_d_now_is_unsupported(b_bits)
                    {
                        return 0;
                    }
                    let a = f32::from_bits(a_bits);
                    let b = f32::from_bits(b_bits);
                    if match kind {
                        PfCmpEq => a == b,
                        PfCmpGe => a >= b,
                        PfCmpGt => a > b,
                        _ => unreachable!(),
                    } {
                        u32::MAX
                    } else {
                        0
                    }
                };
                Self::x86_three_d_now_pack(
                    compare(first_low, second_low),
                    compare(first_high, second_high),
                )
            }
            PfMax | PfMin => Self::x86_three_d_now_pack(
                Self::x86_three_d_now_min_max(first_low, second_low, kind == PfMax),
                Self::x86_three_d_now_min_max(first_high, second_high, kind == PfMax),
            ),
            PfRcp | PfRsqrt => {
                let result = Self::x86_three_d_now_reciprocal(second_low, kind == PfRsqrt);
                Self::x86_three_d_now_pack(result, result)
            }
            PfRcpIt1 | PfRcpIt2 | PfRsqIt1 => Self::x86_three_d_now_pack(
                Self::x86_three_d_now_iteration(first_low, second_low, kind),
                Self::x86_three_d_now_iteration(first_high, second_high, kind),
            ),
            PmulHrw => {
                let mut result = 0u64;
                for lane in 0..4 {
                    let shift = lane * 16;
                    let a = ((first >> shift) as u16) as i16 as i32;
                    let b = ((second >> shift) as u16) as i16 as i32;
                    let rounded = (a * b + 0x8000) >> 16;
                    result |= u64::from(rounded as u16) << shift;
                }
                result
            }
        }
    }
}
