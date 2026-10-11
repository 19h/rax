//! A64 integer width transitions. Wide intermediates retain carry and rounding
//! bits until the architectural truncation or saturation boundary.

use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::ops::{
    ARM_FPSR_QC, ArmSimdLongKind, ArmSimdLongOp, ArmSimdNarrowKind, ArmSimdNarrowMode,
    ArmSimdNarrowOp, ArmSimdPart,
};

use super::super::SmirInterpreter;

fn extend_lane(value: u64, bits: u32, signed: bool) -> i128 {
    if signed {
        let shift = 64 - bits;
        i128::from(((value << shift) as i64) >> shift)
    } else {
        i128::from(value)
    }
}

fn saturate_signed(value: i128, bits: u32) -> (i128, bool) {
    let minimum = -(1i128 << (bits - 1));
    let maximum = (1i128 << (bits - 1)) - 1;
    let result = value.clamp(minimum, maximum);
    (result, result != value)
}

fn narrow_lane(value: i128, bits: u32, mode: ArmSimdNarrowMode) -> (u64, bool) {
    let mask = (1u64 << bits) - 1;
    let (result, saturated) = match mode {
        ArmSimdNarrowMode::Truncate => (value, false),
        ArmSimdNarrowMode::SignedSaturate => saturate_signed(value, bits),
        ArmSimdNarrowMode::UnsignedSaturate | ArmSimdNarrowMode::SignedToUnsigned => {
            let result = value.clamp(0, i128::from(mask));
            (result, result != value)
        }
    };
    (result as u64 & mask, saturated)
}

impl SmirInterpreter {
    fn reject_arm_resize(ctx: &mut SmirContext, guest_pc: u64) {
        ctx.exit_reason = Some(ExitReason::Undefined {
            addr: guest_pc,
            opcode: 0,
        });
    }

    pub(crate) fn execute_arm_simd_long(
        ctx: &mut SmirContext,
        operation: &ArmSimdLongOp,
        guest_pc: u64,
    ) {
        if !operation.shape_valid() || !matches!(ctx.arch_regs, ArchRegState::Aarch64(_)) {
            Self::reject_arm_resize(ctx, guest_pc);
            return;
        }
        let source1 = Self::read_vec(ctx, operation.src1);
        let source2 = operation.src2.map(|reg| Self::read_vec(ctx, reg));
        let destination = Self::read_vec(ctx, operation.dst);
        let bits = operation.elem.bytes() * 8;
        let wide_bits = bits * 2;
        let (lanes, source_offset) = match (operation.kind, operation.part) {
            (ArmSimdLongKind::PairwiseAdd { width, .. }, _) => (width.bytes() * 8 / wide_bits, 0),
            (_, ArmSimdPart::Scalar) => (1, 0),
            (_, ArmSimdPart::UpperHalf) => (64 / bits, 64 / bits),
            _ => (64 / bits, 0),
        };
        let mut result = [0; 16];
        let mut saturated = false;
        for lane in 0..lanes {
            let pairwise = matches!(operation.kind, ArmSimdLongKind::PairwiseAdd { .. });
            let wide_first = matches!(
                operation.kind,
                ArmSimdLongKind::AddWide | ArmSimdLongKind::SubtractWide
            );
            let first_bits = match wide_first {
                true => wide_bits,
                false => bits,
            };
            let first_index = match (wide_first, pairwise) {
                (true, _) => lane,
                (false, true) => 2 * lane,
                _ => lane + source_offset,
            };
            let first = extend_lane(
                Self::get_lane(&source1, first_index as u8, first_bits),
                first_bits,
                operation.signed,
            );
            let second = match (&source2, pairwise) {
                (_, true) => extend_lane(
                    Self::get_lane(&source1, (2 * lane + 1) as u8, bits),
                    bits,
                    operation.signed,
                ),
                (Some(source), false) => extend_lane(
                    Self::get_lane(source, (lane + source_offset) as u8, bits),
                    bits,
                    operation.signed,
                ),
                (None, false) => 0,
            };
            let accumulator = extend_lane(
                Self::get_lane(&destination, lane as u8, wide_bits),
                wide_bits,
                true,
            );
            let value = match operation.kind {
                ArmSimdLongKind::Add | ArmSimdLongKind::AddWide => first + second,
                ArmSimdLongKind::Subtract | ArmSimdLongKind::SubtractWide => first - second,
                ArmSimdLongKind::AbsoluteDifference => (first - second).abs(),
                ArmSimdLongKind::AbsoluteDifferenceAccumulate => {
                    accumulator + (first - second).abs()
                }
                ArmSimdLongKind::Multiply => first * second,
                ArmSimdLongKind::MultiplyAccumulate => accumulator + first * second,
                ArmSimdLongKind::MultiplySubtract => accumulator - first * second,
                ArmSimdLongKind::ShiftLeft { amount } => first << amount,
                ArmSimdLongKind::PairwiseAdd { accumulate, .. } => {
                    let mut sum = first + second;
                    if accumulate {
                        sum += accumulator;
                    }
                    sum
                }
                ArmSimdLongKind::SaturatingDoublingMultiply
                | ArmSimdLongKind::SaturatingDoublingMultiplyAccumulate
                | ArmSimdLongKind::SaturatingDoublingMultiplySubtract => {
                    // Arm SignedSatQ is applied independently to the doubled
                    // product and the final accumulator (DDI 0602 SQDMLAL).
                    let (product, product_saturated) =
                        saturate_signed(2 * first * second, wide_bits);
                    let combined = match operation.kind {
                        ArmSimdLongKind::SaturatingDoublingMultiplyAccumulate => {
                            accumulator + product
                        }
                        ArmSimdLongKind::SaturatingDoublingMultiplySubtract => {
                            accumulator - product
                        }
                        _ => product,
                    };
                    let (value, accumulator_saturated) = saturate_signed(combined, wide_bits);
                    saturated |= product_saturated || accumulator_saturated;
                    value
                }
            };
            Self::set_lane(&mut result, lane as u8, wide_bits, value as u64);
        }
        Self::write_vec(ctx, operation.dst, result);
        if saturated {
            if let ArchRegState::Aarch64(state) = &mut ctx.arch_regs {
                state.fpsr |= ARM_FPSR_QC;
            }
        }
    }

    pub(crate) fn execute_arm_simd_narrow(
        ctx: &mut SmirContext,
        operation: &ArmSimdNarrowOp,
        guest_pc: u64,
    ) {
        if !operation.shape_valid() || !matches!(ctx.arch_regs, ArchRegState::Aarch64(_)) {
            Self::reject_arm_resize(ctx, guest_pc);
            return;
        }
        let source1 = Self::read_vec(ctx, operation.src1);
        let source2 = operation.src2.map(|reg| Self::read_vec(ctx, reg));
        let old_destination = Self::read_vec(ctx, operation.dst);
        let bits = operation.elem.bytes() * 8;
        let wide_bits = bits * 2;
        let wide_mask = u128::MAX >> (128 - wide_bits);
        let lanes = match operation.part {
            ArmSimdPart::Scalar => 1,
            _ => 64 / bits,
        };
        let destination_offset = match operation.part {
            ArmSimdPart::UpperHalf => 64 / bits,
            _ => 0,
        };
        let mut result = [0; 16];
        if operation.part == ArmSimdPart::UpperHalf {
            result[0] = old_destination[0];
        }
        let mut saturated = false;
        for lane in 0..lanes {
            let raw = Self::get_lane(&source1, lane as u8, wide_bits);
            let (value, clipped) = match operation.kind {
                ArmSimdNarrowKind::HighArithmetic { subtract, rounding } => {
                    let second = source2
                        .as_ref()
                        .map(|source| Self::get_lane(source, lane as u8, wide_bits))
                        .unwrap_or(0);
                    let mut sum = match subtract {
                        true => u128::from(raw).wrapping_sub(u128::from(second)),
                        false => u128::from(raw) + u128::from(second),
                    };
                    if rounding {
                        sum = sum.wrapping_add(1u128 << (bits - 1));
                    }
                    (((sum & wide_mask) >> bits) as u64, false)
                }
                ArmSimdNarrowKind::Extract { mode } => {
                    let signed = matches!(
                        mode,
                        ArmSimdNarrowMode::SignedSaturate | ArmSimdNarrowMode::SignedToUnsigned
                    );
                    narrow_lane(extend_lane(raw, wide_bits, signed), bits, mode)
                }
                ArmSimdNarrowKind::ShiftRight {
                    amount,
                    rounding,
                    mode,
                } => {
                    let signed = matches!(
                        mode,
                        ArmSimdNarrowMode::SignedSaturate | ArmSimdNarrowMode::SignedToUnsigned
                    );
                    let mut wide = extend_lane(raw, wide_bits, signed);
                    if rounding {
                        wide += 1i128 << (amount - 1);
                    }
                    narrow_lane(wide >> amount, bits, mode)
                }
            };
            Self::set_lane(&mut result, (lane + destination_offset) as u8, bits, value);
            saturated |= clipped;
        }
        Self::write_vec(ctx, operation.dst, result);
        if saturated {
            if let ArchRegState::Aarch64(state) = &mut ctx.arch_regs {
                state.fpsr |= ARM_FPSR_QC;
            }
        }
    }
}
