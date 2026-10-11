//! Independent A64 integer saturation kernels. Carry and rounding remain in
//! i128 until the architectural clipping boundary, including 64-bit elements.

use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::ops::{
    ARM_FPSR_QC, ArmSimdSaturatingForm, ArmSimdSaturatingKind, ArmSimdSaturatingOp,
};

use super::super::SmirInterpreter;

fn integer_lane(value: u64, bits: u32, signed: bool) -> i128 {
    if signed {
        let shift = 64 - bits;
        i128::from(((value << shift) as i64) >> shift)
    } else {
        i128::from(value)
    }
}

fn bounds(bits: u32, signed: bool) -> (i128, i128) {
    if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    }
}

fn saturating_shift(value: i128, amount: i32, bits: u32, signed: bool, rounding: bool) -> i128 {
    if amount >= 0 {
        if amount as u32 >= bits {
            let (minimum, maximum) = bounds(bits, signed);
            // A nonzero value cannot fit after a shift by the element width.
            // Return one past the bound so the common clamp also records QC.
            return match value.cmp(&0) {
                std::cmp::Ordering::Less => minimum - 1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => maximum + 1,
            };
        }
        return value << amount;
    }
    let right = (-amount) as u32;
    if right > bits {
        return match (rounding, value < 0) {
            (false, true) => -1,
            _ => 0,
        };
    }
    let bias = match rounding {
        true => 1i128 << (right - 1),
        false => 0,
    };
    (value + bias) >> right
}

impl SmirInterpreter {
    pub(crate) fn execute_arm_simd_saturating(
        ctx: &mut SmirContext,
        operation: &ArmSimdSaturatingOp,
        guest_pc: u64,
    ) {
        if !operation.shape_valid() || !matches!(ctx.arch_regs, ArchRegState::Aarch64(_)) {
            ctx.exit_reason = Some(ExitReason::Undefined {
                addr: guest_pc,
                opcode: 0,
            });
            return;
        }

        // Snapshot all sources before writing, including the implicit mixed
        // accumulator and an indexed source that aliases the destination.
        let source1 = Self::read_vec(ctx, operation.src1);
        let source2 = operation.src2.map(|reg| Self::read_vec(ctx, reg));
        let destination = Self::read_vec(ctx, operation.dst);
        let bits = operation.elem.bytes() * 8;
        let lanes = match operation.form {
            ArmSimdSaturatingForm::Scalar => 1,
            ArmSimdSaturatingForm::Vector64 => 64 / bits,
            ArmSimdSaturatingForm::Vector128 => 128 / bits,
        };
        let (minimum, maximum) = bounds(bits, operation.signed);
        let mut result = [0; 16];
        let mut saturated = false;
        for lane in 0..lanes {
            let first_raw = Self::get_lane(&source1, lane as u8, bits);
            let source_signed = match operation.kind {
                ArmSimdSaturatingKind::MixedAccumulate => !operation.signed,
                ArmSimdSaturatingKind::ShiftSignedToUnsigned { .. } => true,
                _ => operation.signed,
            };
            let first = integer_lane(first_raw, bits, source_signed);
            let source_lane = match operation.kind {
                ArmSimdSaturatingKind::DoublingMultiplyHigh {
                    lane: Some(index), ..
                } => index,
                _ => lane as u8,
            };
            let second_raw = source2
                .as_ref()
                .map(|source| Self::get_lane(source, source_lane, bits))
                .unwrap_or(0);
            let second = integer_lane(second_raw, bits, operation.signed);
            let value = match operation.kind {
                ArmSimdSaturatingKind::Add => first + second,
                ArmSimdSaturatingKind::Subtract => first - second,
                ArmSimdSaturatingKind::MixedAccumulate => {
                    let accumulator = Self::get_lane(&destination, lane as u8, bits);
                    first + integer_lane(accumulator, bits, operation.signed)
                }
                ArmSimdSaturatingKind::Absolute => first.abs(),
                ArmSimdSaturatingKind::Negate => -first,
                ArmSimdSaturatingKind::ShiftRegister { rounding } => {
                    let amount = i32::from(second_raw as u8 as i8);
                    saturating_shift(first, amount, bits, operation.signed, rounding)
                }
                ArmSimdSaturatingKind::ShiftImmediate { amount }
                | ArmSimdSaturatingKind::ShiftSignedToUnsigned { amount } => first << amount,
                ArmSimdSaturatingKind::DoublingMultiplyHigh { rounding, .. } => {
                    let product = 2 * first * second;
                    let bias = match rounding {
                        true => 1i128 << (bits - 1),
                        false => 0,
                    };
                    (product + bias) >> bits
                }
            };
            let clipped = value.clamp(minimum, maximum);
            saturated |= clipped != value;
            Self::set_lane(&mut result, lane as u8, bits, clipped as u64);
        }
        Self::write_vec(ctx, operation.dst, result);
        if saturated {
            if let ArchRegState::Aarch64(state) = &mut ctx.arch_regs {
                state.fpsr |= ARM_FPSR_QC;
            }
        }
    }
}
