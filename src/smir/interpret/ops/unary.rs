//! Vector unary and x86 square-root operation execution.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;

impl SmirInterpreter {
    /// Correctly round one finite-width x86 SIMD square root without using the
    /// host floating-point environment. The significand is normalized before
    /// promotion, then extended by 32 fractional root bits. For binary64 the
    /// radicand is below 2^118, so every intermediate fits in `u128`.
    pub(crate) fn x86_simd_fp_sqrt(
        bits: u64,
        format: X86SimdFpFormat,
        mode: FpRoundMode,
        mxcsr: u32,
    ) -> X86SimdFpResult {
        let source = Self::x86_simd_fp_apply_daz(bits, format, mxcsr);
        let mut status = source.status;
        let bits = source.bits;
        let (sign, exponent_mask, _, _) = Self::x86_simd_fp_masks(format);

        if Self::x86_simd_fp_is_nan(bits, format) {
            status |= u32::from(Self::x86_simd_fp_is_snan(bits, format));
            return X86SimdFpResult {
                bits: Self::x86_simd_fp_quiet_nan(bits, format),
                status,
            };
        }
        if Self::x86_simd_fp_is_zero(bits, format) {
            return X86SimdFpResult { bits, status };
        }
        if bits & sign != 0 {
            return X86SimdFpResult {
                bits: Self::x86_simd_fp_indefinite(format),
                // An invalid square-root operand is classified as #I rather
                // than also reporting #D for the same negative denormal.
                status: 1,
            };
        }
        if Self::x86_simd_fp_is_infinite(bits, format) {
            return X86SimdFpResult {
                bits: exponent_mask,
                status,
            };
        }

        let mut finite = Self::x86_simd_fp_decode(bits, format);
        debug_assert!(!finite.negative && finite.significand != 0);

        // Subnormal inputs can have only one significant source bit. Normalize
        // first so the fixed-point root retains enough bits for a binary64
        // rounding decision as well as for binary32.
        let source_msb = 127 - finite.significand.leading_zeros() as i32;
        let normalization = format.fraction_bits as i32 - source_msb;
        debug_assert!(normalization >= 0);
        finite.significand <<= normalization as u32;
        finite.exponent -= normalization;

        // A square root halves the binary exponent. Move one factor of two
        // into the significand when necessary so integer division is exact.
        if finite.exponent & 1 != 0 {
            finite.significand <<= 1;
            finite.exponent -= 1;
        }

        const EXTRA_BITS: u32 = 32;
        let radicand = finite.significand << (2 * EXTRA_BITS);
        let root = Self::x86_x87_integer_sqrt(radicand);
        let sticky = root * root != radicand;
        let rounded = Self::x86_simd_fp_round_exact(
            false,
            root,
            finite.exponent / 2 - EXTRA_BITS as i32,
            sticky,
            format,
            mode,
            mxcsr,
        );
        X86SimdFpResult {
            bits: rounded.bits,
            status: status | rounded.status,
        }
    }

    pub(crate) fn execute_op_unary(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            OpKind::X86Sqrt {
                dst,
                src,
                elem,
                lanes,
                round,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let valid_rounding = matches!(
                    (round, suppress_exceptions),
                    (FpRoundMode::Dynamic, false)
                        | (
                            FpRoundMode::RoundNearest
                                | FpRoundMode::RoundDown
                                | FpRoundMode::RoundUp
                                | FpRoundMode::RoundTowardZero,
                            true
                        )
                );
                let max_lanes = (VecWidth::V512.bytes() / elem.bytes()) as u8;
                if !valid_rounding || *lanes == 0 || *lanes > max_lanes {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let source = Self::read_vec(ctx, *src);
                let elem_bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut status = 0u32;
                for lane in 0..*lanes {
                    let computed = Self::x86_simd_fp_sqrt(
                        Self::get_lane(&source, lane, elem_bits),
                        format,
                        mode,
                        mxcsr,
                    );
                    status |= computed.status;
                    Self::set_lane(&mut result, lane, elem_bits, computed.bits);
                }

                if !*suppress_exceptions && status != 0 {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VUnary {
                dst,
                src,
                elem,
                lanes,
                op,
            } => match (op, elem) {
                (VecUnaryOp::FAbs, VecElementType::F32) => {
                    self.vec_unary_op_f32(ctx, *dst, *src, *lanes, |a| a.abs());
                }
                (VecUnaryOp::FAbs, VecElementType::F64) => {
                    self.vec_unary_op_f64(ctx, *dst, *src, *lanes, |a| a.abs());
                }
                (VecUnaryOp::FNeg, VecElementType::F32) => {
                    self.vec_unary_op_f32(ctx, *dst, *src, *lanes, |a| -a);
                }
                (VecUnaryOp::FNeg, VecElementType::F64) => {
                    self.vec_unary_op_f64(ctx, *dst, *src, *lanes, |a| -a);
                }
                (VecUnaryOp::FSqrt, VecElementType::F32) => {
                    self.vec_unary_op_f32(ctx, *dst, *src, *lanes, |a| a.sqrt());
                }
                (VecUnaryOp::FSqrt, VecElementType::F64) => {
                    self.vec_unary_op_f64(ctx, *dst, *src, *lanes, |a| a.sqrt());
                }
                (VecUnaryOp::FRecipEstimate | VecUnaryOp::FRsqrtEstimate, VecElementType::F32) => {
                    let input = Self::read_vec(ctx, *src);
                    let mut result = [0u64; 16];
                    for lane in 0..*lanes {
                        let raw = Self::get_lane(&input, lane, 32) as u32;
                        let sign = raw & 0x8000_0000;
                        let exponent = raw & 0x7F80_0000;
                        let fraction = raw & 0x007F_FFFF;
                        let estimate = if exponent == 0 {
                            // Zero and denormal inputs are both treated as signed zero.
                            sign | 0x7F80_0000
                        } else if exponent == 0x7F80_0000 && fraction != 0 {
                            // Preserve the NaN payload/sign and quiet signaling NaNs.
                            raw | 0x0040_0000
                        } else if *op == VecUnaryOp::FRsqrtEstimate && sign != 0 {
                            // Negative finite values and -infinity produce FP indefinite;
                            // -0 was handled by the exponent==0 case above.
                            0xFFC0_0000
                        } else if exponent == 0x7F80_0000 {
                            // Reciprocal(+/-inf) is signed zero; rsqrt(+inf) is +zero.
                            if *op == VecUnaryOp::FRecipEstimate {
                                sign
                            } else {
                                0
                            }
                        } else {
                            let value = f32::from_bits(raw);
                            let exact = if *op == VecUnaryOp::FRecipEstimate {
                                1.0 / value
                            } else {
                                1.0 / value.sqrt()
                            };
                            let bits = exact.to_bits();
                            // Architectural estimate results that are tiny are flushed.
                            if bits & 0x7F80_0000 == 0 {
                                bits & 0x8000_0000
                            } else {
                                bits
                            }
                        };
                        Self::set_lane(&mut result, lane, 32, u64::from(estimate));
                    }
                    Self::write_vec(ctx, *dst, result);
                }
                (VecUnaryOp::FRecipEstimate | VecUnaryOp::FRsqrtEstimate, VecElementType::F64) => {
                    unreachable!("x86 reciprocal estimate instructions are F32-only")
                }
                (VecUnaryOp::Neg, _) => {
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        let neg = (a as i64).wrapping_neg() as u64;
                        if bits >= 64 {
                            neg
                        } else {
                            neg & ((1u64 << bits) - 1)
                        }
                    });
                }
                (VecUnaryOp::Abs, _) => {
                    let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        // Sign-extend the lane, take abs, re-truncate.
                        let shift = 64 - bits;
                        let signed = ((a << shift) as i64) >> shift;
                        let abs = signed.unsigned_abs();
                        if bits >= 64 {
                            abs
                        } else {
                            abs & ((1u64 << bits) - 1)
                        }
                    });
                    Self::restore_legacy_xmm_upper(ctx, *dst, old);
                }
                (VecUnaryOp::Clz, _) => {
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        let v = if bits >= 64 {
                            a
                        } else {
                            a & ((1u64 << bits) - 1)
                        };
                        if v == 0 {
                            bits as u64
                        } else {
                            (v.leading_zeros() - (64 - bits)) as u64
                        }
                    });
                }
                (VecUnaryOp::Cls, _) => {
                    let bits = elem.bytes() * 8;
                    self.vec_unary_op(ctx, *dst, *src, *elem, *lanes, |a| {
                        // Count consecutive bits below the MSB equal to it.
                        let sign = (a >> (bits - 1)) & 1;
                        let mut count = 0u64;
                        for i in (0..bits - 1).rev() {
                            if (a >> i) & 1 == sign {
                                count += 1;
                            } else {
                                break;
                            }
                        }
                        count
                    });
                }
                (VecUnaryOp::Rbit, _) => {
                    self.vec_unary_op(ctx, *dst, *src, VecElementType::I8, *lanes, |a| {
                        (a as u8).reverse_bits() as u64
                    });
                }
                (VecUnaryOp::Cnt, _) => {
                    self.vec_unary_op(ctx, *dst, *src, VecElementType::I8, *lanes, |a| {
                        (a as u8).count_ones() as u64
                    });
                }
                (VecUnaryOp::Not, _) => {
                    self.vec_unary_op(ctx, *dst, *src, VecElementType::I8, *lanes, |a| {
                        !(a as u8) as u64
                    });
                }
                (VecUnaryOp::Rev16 | VecUnaryOp::Rev32 | VecUnaryOp::Rev64, _) => {
                    // Reverse the order of `elem`-sized elements within each
                    // container (16/32/64 bits). This reorders lanes, so it
                    // can't use the per-lane vec_unary_op helper.
                    let container_bits = match op {
                        VecUnaryOp::Rev16 => 16u32,
                        VecUnaryOp::Rev32 => 32,
                        _ => 64,
                    };
                    let elem_bits = elem.bytes() * 8;
                    let per = (container_bits / elem_bits).max(1);
                    let a = Self::read_vec(ctx, *src);
                    let mut result = [0u64; 16];
                    for lane in 0..u32::from(*lanes) {
                        let container = lane / per;
                        let within = lane % per;
                        let dst_lane = container * per + (per - 1 - within);
                        let v = Self::get_lane(&a, lane as u8, elem_bits);
                        Self::set_lane(&mut result, dst_lane as u8, elem_bits, v);
                    }
                    Self::write_vec(ctx, *dst, result);
                }
                _ => {
                    // FP-only ops with an integer element (or vice versa) should
                    // not be produced; leave dst unchanged defensively.
                }
            },

            _ => return self.execute_op_binary(ctx, memory, op),
        }

        Ok(())
    }
}
