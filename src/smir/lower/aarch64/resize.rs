//! Native A64 encodings for typed integer width transitions.

use crate::smir::ir::ops::{
    ArmSimdLongKind, ArmSimdLongOp, ArmSimdNarrowKind, ArmSimdNarrowMode, ArmSimdNarrowOp,
    ArmSimdPart, OpKind, SmirOp,
};
use crate::smir::ir::types::VecWidth;
use crate::smir::lower::LowerError;

use super::Aarch64Lowerer;

pub(crate) fn aarch64_resize_shape_valid(op: &SmirOp) -> bool {
    if op.x86_hint.is_some() {
        return false;
    }
    let (valid, dst, src1, src2) = match &op.kind {
        OpKind::ArmSimdLong(operation) => (
            operation.shape_valid(),
            operation.dst,
            operation.src1,
            operation.src2,
        ),
        OpKind::ArmSimdNarrow(operation) => (
            operation.shape_valid(),
            operation.dst,
            operation.src1,
            operation.src2,
        ),
        _ => return false,
    };
    valid
        && Aarch64Lowerer::fp_reg(dst).is_ok()
        && Aarch64Lowerer::fp_reg(src1).is_ok()
        && src2.is_none_or(|reg| Aarch64Lowerer::fp_reg(reg).is_ok())
}

fn base_and_q(part: ArmSimdPart) -> (u32, u32) {
    match part {
        ArmSimdPart::LowerHalf => (0x0e20_0000, 0),
        ArmSimdPart::UpperHalf => (0x0e20_0000, 1),
        ArmSimdPart::Scalar => (0x5e20_0000, 0),
    }
}

impl Aarch64Lowerer {
    pub(crate) fn lower_arm_simd_long(
        &mut self,
        operation: &ArmSimdLongOp,
    ) -> Result<(), LowerError> {
        let rd = u32::from(Self::fp_reg(operation.dst)?);
        let rn = u32::from(Self::fp_reg(operation.src1)?);
        let rm = match operation.src2 {
            Some(source) => u32::from(Self::fp_reg(source)?),
            None => 0,
        };
        let size = operation.elem.bytes().trailing_zeros();
        let bits = operation.elem.bytes() * 8;
        let u = u32::from(!operation.signed);
        let (base, q) = base_and_q(operation.part);
        let instruction = match operation.kind {
            ArmSimdLongKind::ShiftLeft { amount } if u32::from(amount) == bits => {
                0x2e20_0800 | (q << 30) | (size << 22) | (19 << 12) | (rn << 5) | rd
            }
            ArmSimdLongKind::ShiftLeft { amount } => {
                let encoded = bits + u32::from(amount);
                0x0f00_0400 | (q << 30) | (u << 29) | (encoded << 16) | (20 << 11) | (rn << 5) | rd
            }
            ArmSimdLongKind::PairwiseAdd { width, accumulate } => {
                let q = u32::from(width == VecWidth::V128);
                let opcode = match accumulate {
                    false => 2,
                    true => 6,
                };
                0x0e20_0800 | (q << 30) | (u << 29) | (size << 22) | (opcode << 12) | (rn << 5) | rd
            }
            kind => {
                let opcode = match kind {
                    ArmSimdLongKind::Add => 0,
                    ArmSimdLongKind::AddWide => 1,
                    ArmSimdLongKind::Subtract => 2,
                    ArmSimdLongKind::SubtractWide => 3,
                    ArmSimdLongKind::AbsoluteDifferenceAccumulate => 5,
                    ArmSimdLongKind::AbsoluteDifference => 7,
                    ArmSimdLongKind::MultiplyAccumulate => 8,
                    ArmSimdLongKind::SaturatingDoublingMultiplyAccumulate => 9,
                    ArmSimdLongKind::MultiplySubtract => 10,
                    ArmSimdLongKind::SaturatingDoublingMultiplySubtract => 11,
                    ArmSimdLongKind::Multiply => 12,
                    ArmSimdLongKind::SaturatingDoublingMultiply => 13,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "A64 long operation shape".to_string(),
                        });
                    }
                };
                base | (q << 30)
                    | (u << 29)
                    | (size << 22)
                    | (rm << 16)
                    | (opcode << 12)
                    | (rn << 5)
                    | rd
            }
        };
        self.emit(instruction);
        Ok(())
    }

    pub(crate) fn lower_arm_simd_narrow(
        &mut self,
        operation: &ArmSimdNarrowOp,
    ) -> Result<(), LowerError> {
        let rd = u32::from(Self::fp_reg(operation.dst)?);
        let rn = u32::from(Self::fp_reg(operation.src1)?);
        let size = operation.elem.bytes().trailing_zeros();
        let bits = operation.elem.bytes() * 8;
        let (base, q) = base_and_q(operation.part);
        let instruction = match operation.kind {
            ArmSimdNarrowKind::HighArithmetic { subtract, rounding } => {
                let Some(source) = operation.src2 else {
                    return Err(LowerError::UnsupportedOp {
                        op: "A64 high narrowing source".to_string(),
                    });
                };
                let rm = u32::from(Self::fp_reg(source)?);
                let opcode = match subtract {
                    false => 4,
                    true => 6,
                };
                base | (q << 30)
                    | (u32::from(rounding) << 29)
                    | (size << 22)
                    | (rm << 16)
                    | (opcode << 12)
                    | (rn << 5)
                    | rd
            }
            ArmSimdNarrowKind::Extract { mode } => {
                let (u, opcode) = match mode {
                    ArmSimdNarrowMode::Truncate => (0, 18),
                    ArmSimdNarrowMode::SignedToUnsigned => (1, 18),
                    ArmSimdNarrowMode::SignedSaturate => (0, 20),
                    ArmSimdNarrowMode::UnsignedSaturate => (1, 20),
                };
                base | 0x800
                    | (q << 30)
                    | (u << 29)
                    | (size << 22)
                    | (opcode << 12)
                    | (rn << 5)
                    | rd
            }
            ArmSimdNarrowKind::ShiftRight {
                amount,
                rounding,
                mode,
            } => {
                let (u, opcode) = match mode {
                    ArmSimdNarrowMode::Truncate => (0, 16),
                    ArmSimdNarrowMode::SignedToUnsigned => (1, 16),
                    ArmSimdNarrowMode::SignedSaturate => (0, 18),
                    ArmSimdNarrowMode::UnsignedSaturate => (1, 18),
                };
                let base = match operation.part {
                    ArmSimdPart::Scalar => 0x5f00_0400,
                    _ => 0x0f00_0400,
                };
                let encoded = 2 * bits - u32::from(amount);
                base | (q << 30)
                    | (u << 29)
                    | (encoded << 16)
                    | ((opcode + u32::from(rounding)) << 11)
                    | (rn << 5)
                    | rd
            }
        };
        self.emit(instruction);
        Ok(())
    }
}
