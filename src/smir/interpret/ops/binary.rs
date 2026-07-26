//! Exact x86 SIMD binary floating-point operation execution.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;

impl SmirInterpreter {
    /// Intel SDM Vol. 1 D.4.2.2: a NaN result has precedence over every
    /// non-invalid floating-point exception. In particular, a denormal in the
    /// other source does not report DE when this lane returns a NaN.
    pub(crate) fn x86_simd_fp_arithmetic_nan(
        first: u64,
        second: u64,
        format: X86SimdFpFormat,
    ) -> Option<X86SimdFpResult> {
        if !Self::x86_simd_fp_is_nan(first, format) && !Self::x86_simd_fp_is_nan(second, format) {
            return None;
        }
        Some(X86SimdFpResult {
            bits: Self::x86_simd_fp_propagate_nan(first, second, format),
            status: u32::from(
                Self::x86_simd_fp_is_snan(first, format)
                    || Self::x86_simd_fp_is_snan(second, format),
            ),
        })
    }

    pub(crate) fn x86_simd_fp_sub(
        first: u64,
        second: u64,
        format: X86SimdFpFormat,
        mode: FpRoundMode,
        mxcsr: u32,
    ) -> X86SimdFpResult {
        if let Some(result) = Self::x86_simd_fp_arithmetic_nan(first, second, format) {
            return result;
        }
        let first = Self::x86_simd_fp_apply_daz(first, format, mxcsr);
        let second = Self::x86_simd_fp_apply_daz(second, format, mxcsr);
        let status = first.status | second.status;
        let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
        let mut result = Self::x86_simd_fp_add(first.bits, second.bits ^ sign, format, mode, mxcsr);
        result.status |= status;
        result
    }

    /// Correctly rounded binary32/binary64 division. The finite significands
    /// are scaled into a `u128` quotient with at least 21 guard bits beyond the
    /// destination precision; the exact remainder supplies every lower sticky
    /// bit to the common IEEE-754 rounding core.
    pub(crate) fn x86_simd_fp_div(
        first: u64,
        second: u64,
        format: X86SimdFpFormat,
        mode: FpRoundMode,
        mxcsr: u32,
    ) -> X86SimdFpResult {
        if let Some(result) = Self::x86_simd_fp_arithmetic_nan(first, second, format) {
            return result;
        }
        let first = Self::x86_simd_fp_apply_daz(first, format, mxcsr);
        let second = Self::x86_simd_fp_apply_daz(second, format, mxcsr);
        let status = first.status | second.status;

        let first_infinite = Self::x86_simd_fp_is_infinite(first.bits, format);
        let second_infinite = Self::x86_simd_fp_is_infinite(second.bits, format);
        let first_zero = Self::x86_simd_fp_is_zero(first.bits, format);
        let second_zero = Self::x86_simd_fp_is_zero(second.bits, format);
        let (sign, exponent, _, _) = Self::x86_simd_fp_masks(format);
        let negative = (first.bits ^ second.bits) & sign != 0;
        let signed = if negative { sign } else { 0 };

        if (first_infinite && second_infinite) || (first_zero && second_zero) {
            return X86SimdFpResult {
                bits: Self::x86_simd_fp_indefinite(format),
                status: status | 1,
            };
        }
        if first_infinite {
            return X86SimdFpResult {
                bits: signed | exponent,
                status,
            };
        }
        if second_infinite {
            return X86SimdFpResult {
                bits: signed,
                status,
            };
        }
        if second_zero {
            // Intel SDM Vol. 1 §4.9.2: divide-by-zero has precedence over
            // denormal-operand. A denormal numerator divided by zero therefore
            // reports masked #Z, not #D|#Z, for this lane.
            return X86SimdFpResult {
                bits: signed | exponent,
                status: 1 << 2,
            };
        }
        if first_zero {
            return X86SimdFpResult {
                bits: signed,
                status,
            };
        }

        let mut numerator = Self::x86_simd_fp_decode(first.bits, format);
        let mut denominator = Self::x86_simd_fp_decode(second.bits, format);
        // Subnormal significands can contain as little as one significant bit.
        // Normalize both operands to the destination precision before forming
        // the fixed-point quotient; otherwise a binary64 quotient such as
        // min_subnormal/max_subnormal would retain only 23 significant bits.
        let precision_msb = format.fraction_bits as i32;
        let numerator_shift = precision_msb - (127 - numerator.significand.leading_zeros() as i32);
        let denominator_shift =
            precision_msb - (127 - denominator.significand.leading_zeros() as i32);
        numerator.significand <<= numerator_shift;
        numerator.exponent -= numerator_shift;
        denominator.significand <<= denominator_shift;
        denominator.exponent -= denominator_shift;
        let quotient_bits = if format.total_bits == 32 { 100 } else { 74 };
        let scaled = numerator.significand << quotient_bits;
        let quotient = scaled / denominator.significand;
        let remainder = scaled % denominator.significand;
        let rounded = Self::x86_simd_fp_round_exact(
            negative,
            quotient,
            numerator.exponent - denominator.exponent - quotient_bits as i32,
            remainder != 0,
            format,
            mode,
            mxcsr,
        );
        X86SimdFpResult {
            bits: rounded.bits,
            status: status | rounded.status,
        }
    }

    /// Intel MIN*/MAX* selection: unordered and equal operands select src2,
    /// and every NaN (including a QNaN) reports invalid. MXCSR.DAZ converts
    /// every denormal source operand to signed zero before selection, including
    /// a denormal src2 selected because src1 is NaN, without accruing DE.
    pub(crate) fn x86_simd_fp_min_max(
        first: u64,
        second: u64,
        format: X86SimdFpFormat,
        mxcsr: u32,
        min: bool,
    ) -> X86SimdFpResult {
        let any_nan =
            Self::x86_simd_fp_is_nan(first, format) || Self::x86_simd_fp_is_nan(second, format);
        if any_nan {
            // MIN/MAX selects src2 for every unordered lane and reports
            // invalid for both quiet and signaling NaNs. DAZ still transforms
            // a selected denormal src2 to signed zero before the computation;
            // discard the helper's DE status so NaN exception precedence
            // remains exact when DAZ is clear.
            return X86SimdFpResult {
                bits: Self::x86_simd_fp_apply_daz(second, format, mxcsr).bits,
                status: 1,
            };
        }
        let first = Self::x86_simd_fp_apply_daz(first, format, mxcsr);
        let second = Self::x86_simd_fp_apply_daz(second, format, mxcsr);
        let status = first.status | second.status;

        let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
        let first_magnitude = first.bits & !sign;
        let second_magnitude = second.bits & !sign;
        if first_magnitude == second_magnitude {
            return X86SimdFpResult {
                bits: second.bits,
                status,
            };
        }
        let first_negative = first.bits & sign != 0;
        let second_negative = second.bits & sign != 0;
        let first_less = if first_negative != second_negative {
            first_negative
        } else if first_negative {
            first_magnitude > second_magnitude
        } else {
            first_magnitude < second_magnitude
        };
        let select_first = if min { first_less } else { !first_less };
        X86SimdFpResult {
            bits: if select_first {
                first.bits
            } else {
                second.bits
            },
            status,
        }
    }

    pub(crate) fn execute_op_binary(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        match &op.kind {
            OpKind::X86FpBinary {
                dst,
                src1,
                src2,
                mask,
                elem,
                lanes,
                op,
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
                let sse3_paired = matches!(
                    op,
                    X86FpBinaryOp::AddSub
                        | X86FpBinaryOp::HorizontalAdd
                        | X86FpBinaryOp::HorizontalSub
                );
                let arithmetic = matches!(
                    op,
                    X86FpBinaryOp::Add
                        | X86FpBinaryOp::Sub
                        | X86FpBinaryOp::Mul
                        | X86FpBinaryOp::Div
                ) || sse3_paired;
                let valid_rounding = if sse3_paired {
                    *round == FpRoundMode::Dynamic
                        && !*suppress_exceptions
                        && mask.is_none()
                        && matches!(
                            (elem, *lanes),
                            (VecElementType::F32, 4 | 8) | (VecElementType::F64, 2 | 4)
                        )
                } else if arithmetic {
                    matches!(
                        (round, suppress_exceptions),
                        (FpRoundMode::Dynamic, false)
                            | (
                                FpRoundMode::RoundNearest
                                    | FpRoundMode::RoundDown
                                    | FpRoundMode::RoundUp
                                    | FpRoundMode::RoundTowardZero,
                                true
                            )
                    )
                } else {
                    *round == FpRoundMode::Dynamic
                };
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
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let elem_bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut status = 0u32;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        continue;
                    }
                    let (first_bits, second_bits, lane_op) = match *op {
                        X86FpBinaryOp::AddSub => (
                            Self::get_lane(&first, lane, elem_bits),
                            Self::get_lane(&second, lane, elem_bits),
                            if lane & 1 == 0 {
                                X86FpBinaryOp::Sub
                            } else {
                                X86FpBinaryOp::Add
                            },
                        ),
                        X86FpBinaryOp::HorizontalAdd | X86FpBinaryOp::HorizontalSub => {
                            let per_128 = (16 / elem.bytes()) as u8;
                            let pairs = per_128 / 2;
                            let group = lane / per_128;
                            let position = lane % per_128;
                            let (source, pair) = if position < pairs {
                                (&first, position)
                            } else {
                                (&second, position - pairs)
                            };
                            let left_lane = group * per_128 + pair * 2;
                            (
                                Self::get_lane(source, left_lane, elem_bits),
                                Self::get_lane(source, left_lane + 1, elem_bits),
                                if *op == X86FpBinaryOp::HorizontalAdd {
                                    X86FpBinaryOp::Add
                                } else {
                                    X86FpBinaryOp::Sub
                                },
                            )
                        }
                        lane_op => (
                            Self::get_lane(&first, lane, elem_bits),
                            Self::get_lane(&second, lane, elem_bits),
                            lane_op,
                        ),
                    };
                    let computed = match lane_op {
                        X86FpBinaryOp::Add => {
                            Self::x86_simd_fp_add(first_bits, second_bits, format, mode, mxcsr)
                        }
                        X86FpBinaryOp::Sub => {
                            Self::x86_simd_fp_sub(first_bits, second_bits, format, mode, mxcsr)
                        }
                        X86FpBinaryOp::Mul => {
                            Self::x86_simd_fp_mul(first_bits, second_bits, format, mode, mxcsr)
                        }
                        X86FpBinaryOp::Div => {
                            Self::x86_simd_fp_div(first_bits, second_bits, format, mode, mxcsr)
                        }
                        X86FpBinaryOp::Min => {
                            Self::x86_simd_fp_min_max(first_bits, second_bits, format, mxcsr, true)
                        }
                        X86FpBinaryOp::Max => {
                            Self::x86_simd_fp_min_max(first_bits, second_bits, format, mxcsr, false)
                        }
                        X86FpBinaryOp::AddSub
                        | X86FpBinaryOp::HorizontalAdd
                        | X86FpBinaryOp::HorizontalSub => {
                            unreachable!("paired operation must map to scalar add or subtract")
                        }
                    };
                    status |= computed.status;
                    Self::set_lane(&mut result, lane, elem_bits, computed.bits);
                }

                if !*suppress_exceptions && status != 0 {
                    // Intel SDM Vol. 1 §11.5.3: an unmasked pre-computation
                    // exception (#I/#D/#Z) is reported before any
                    // post-computation #O/#U/#P condition from another active
                    // lane. If every pre-computation condition is masked, all
                    // accumulated status remains observable.
                    let pre_status = status & 0x07;
                    let reported_status = if Self::x86_simd_fp_unmasked(pre_status, mxcsr) {
                        pre_status
                    } else {
                        status
                    };
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= reported_status;
                    }
                    if Self::x86_simd_fp_unmasked(reported_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            _ => return self.execute_op_flags(ctx, memory, op),
        }

        Ok(())
    }
}
