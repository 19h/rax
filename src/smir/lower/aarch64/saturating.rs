//! Native A64 saturation encodings and fail-closed target admission.

use crate::smir::ir::ops::{
    ArmSimdSaturatingForm, ArmSimdSaturatingKind, ArmSimdSaturatingOp, OpKind, SmirOp,
};
use crate::smir::lower::LowerError;

use super::Aarch64Lowerer;

pub(crate) fn aarch64_saturating_shape_valid(op: &SmirOp) -> bool {
    let OpKind::ArmSimdSaturating(operation) = &op.kind else {
        return false;
    };
    op.x86_hint.is_none()
        && operation.shape_valid()
        && Aarch64Lowerer::fp_reg(operation.dst).is_ok()
        && Aarch64Lowerer::fp_reg(operation.src1).is_ok()
        && operation
            .src2
            .is_none_or(|source| Aarch64Lowerer::fp_reg(source).is_ok())
}

impl Aarch64Lowerer {
    pub(crate) fn lower_arm_simd_saturating(
        &mut self,
        operation: &ArmSimdSaturatingOp,
    ) -> Result<(), LowerError> {
        let rd = u32::from(Self::fp_reg(operation.dst)?);
        let rn = u32::from(Self::fp_reg(operation.src1)?);
        let rm = match operation.src2 {
            Some(source) => u32::from(Self::fp_reg(source)?),
            None => 0,
        };
        let size = operation.elem.bytes().trailing_zeros();
        let bits = operation.elem.bytes() * 8;
        let (q, scalar_bit) = match operation.form {
            ArmSimdSaturatingForm::Vector64 => (0, 0),
            ArmSimdSaturatingForm::Vector128 => (1, 0),
            ArmSimdSaturatingForm::Scalar => (1, 1 << 28),
        };
        let register_and_form_fields = (q << 30) | scalar_bit | (rn << 5) | rd;
        let u = u32::from(!operation.signed);
        let instruction = match operation.kind {
            ArmSimdSaturatingKind::MixedAccumulate => {
                register_and_form_fields | 0x0e20_0800 | (u << 29) | (size << 22) | (3 << 12)
            }
            ArmSimdSaturatingKind::Absolute | ArmSimdSaturatingKind::Negate => {
                let u = u32::from(operation.kind == ArmSimdSaturatingKind::Negate);
                register_and_form_fields | 0x0e20_0800 | (u << 29) | (size << 22) | (7 << 12)
            }
            ArmSimdSaturatingKind::ShiftImmediate { amount }
            | ArmSimdSaturatingKind::ShiftSignedToUnsigned { amount } => {
                let opcode = match operation.kind {
                    ArmSimdSaturatingKind::ShiftSignedToUnsigned { .. } => 12,
                    _ => 14,
                };
                let encoded = bits + u32::from(amount);
                register_and_form_fields
                    | 0x0f00_0400
                    | (u << 29)
                    | (encoded << 16)
                    | (opcode << 11)
            }
            ArmSimdSaturatingKind::DoublingMultiplyHigh {
                rounding,
                lane: Some(lane),
            } => {
                let lane = u32::from(lane);
                let index_fields = match bits {
                    16 => ((lane & 1) << 20) | (((lane >> 1) & 1) << 21) | ((lane >> 2) << 11),
                    _ => ((lane & 1) << 21) | ((lane >> 1) << 11),
                };
                register_and_form_fields
                    | 0x0f00_0000
                    | (size << 22)
                    | (rm << 16)
                    | ((12 + u32::from(rounding)) << 12)
                    | index_fields
            }
            kind => {
                let (u, opcode) = match kind {
                    ArmSimdSaturatingKind::Add => (u, 1),
                    ArmSimdSaturatingKind::Subtract => (u, 5),
                    ArmSimdSaturatingKind::ShiftRegister { rounding: false } => (u, 9),
                    ArmSimdSaturatingKind::ShiftRegister { rounding: true } => (u, 11),
                    ArmSimdSaturatingKind::DoublingMultiplyHigh {
                        rounding,
                        lane: None,
                    } => (u32::from(rounding), 22),
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "A64 saturating operation shape".to_string(),
                        });
                    }
                };
                register_and_form_fields
                    | 0x0e20_0400
                    | (u << 29)
                    | (size << 22)
                    | (rm << 16)
                    | (opcode << 11)
            }
        };
        self.emit(instruction);
        Ok(())
    }
}
