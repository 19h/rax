//! A64 integer saturation with explicit signedness, shape, and shift sources.

use crate::isa::arm::ExecutionState;
use crate::isa::arm::decoder::{DecodedInsn, Mnemonic, Operand};
use crate::smir::ir::ops::{
    ArmSimdSaturatingForm, ArmSimdSaturatingKind, ArmSimdSaturatingOp, OpKind,
};
use crate::smir::lift::LiftError;

use super::Aarch64Lifter;

impl Aarch64Lifter {
    pub(super) fn lift_neon_saturating(
        &self,
        insn: &DecodedInsn,
    ) -> Result<Option<OpKind>, LiftError> {
        if insn.state != ExecutionState::Aarch64 {
            return Ok(None);
        }
        if !matches!(
            insn.mnemonic,
            Mnemonic::VQADD
                | Mnemonic::VQSUB
                | Mnemonic::SUQADD
                | Mnemonic::USQADD
                | Mnemonic::VQABS
                | Mnemonic::VQNEG
                | Mnemonic::VQSHL
                | Mnemonic::VQRSHL
                | Mnemonic::VQSHLU
                | Mnemonic::VQDMULH
                | Mnemonic::VQRDMULH
        ) {
            return Ok(None);
        }
        let (dst, src1) = match (insn.operands.first(), insn.operands.get(1)) {
            (Some(Operand::FpReg(dst)), Some(Operand::FpReg(src1))) => {
                (Self::fp_vreg(dst), Self::fp_vreg(src1))
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid A64 saturating operands".to_string(),
                ));
            }
        };
        let src2 = match insn.operands.get(2) {
            Some(Operand::FpReg(source)) => Some(Self::fp_vreg(source)),
            _ => None,
        };
        let raw = insn.raw;
        let q = (raw >> 30) & 1;
        let u = (raw >> 29) & 1;
        let scalar = raw & (1 << 28) != 0;
        let immediate_class = raw & 0x0f00_0400 == 0x0f00_0400;
        let indexed_class = raw & 0x0f00_0400 == 0x0f00_0000;
        let size = if immediate_class {
            let immh = (raw >> 19) & 15;
            if immh == 0 {
                return Err(LiftError::Internal(
                    "invalid A64 saturating immediate width".to_string(),
                ));
            }
            31 - immh.leading_zeros()
        } else {
            (raw >> 22) & 3
        };
        let immediate = || -> Result<u8, LiftError> {
            let Some(Operand::Imm(value)) = insn.operands.last() else {
                return Err(LiftError::Internal(
                    "missing A64 saturating immediate".to_string(),
                ));
            };
            u8::try_from(value.effective_value())
                .map_err(|_| LiftError::Internal("invalid A64 saturating immediate".to_string()))
        };
        let kind = match insn.mnemonic {
            Mnemonic::VQADD => ArmSimdSaturatingKind::Add,
            Mnemonic::VQSUB => ArmSimdSaturatingKind::Subtract,
            Mnemonic::SUQADD | Mnemonic::USQADD => ArmSimdSaturatingKind::MixedAccumulate,
            Mnemonic::VQABS => ArmSimdSaturatingKind::Absolute,
            Mnemonic::VQNEG => ArmSimdSaturatingKind::Negate,
            Mnemonic::VQSHL if immediate_class => ArmSimdSaturatingKind::ShiftImmediate {
                amount: immediate()?,
            },
            Mnemonic::VQSHL => ArmSimdSaturatingKind::ShiftRegister { rounding: false },
            Mnemonic::VQRSHL => ArmSimdSaturatingKind::ShiftRegister { rounding: true },
            Mnemonic::VQSHLU => ArmSimdSaturatingKind::ShiftSignedToUnsigned {
                amount: immediate()?,
            },
            Mnemonic::VQDMULH | Mnemonic::VQRDMULH => ArmSimdSaturatingKind::DoublingMultiplyHigh {
                rounding: insn.mnemonic == Mnemonic::VQRDMULH,
                lane: if indexed_class {
                    Some(immediate()?)
                } else {
                    None
                },
            },
            _ => return Ok(None),
        };
        let signed = match kind {
            ArmSimdSaturatingKind::Absolute
            | ArmSimdSaturatingKind::Negate
            | ArmSimdSaturatingKind::DoublingMultiplyHigh { .. } => true,
            ArmSimdSaturatingKind::ShiftSignedToUnsigned { .. } => false,
            _ => u == 0,
        };
        let form = match (scalar, q) {
            (true, _) => ArmSimdSaturatingForm::Scalar,
            (false, 0) => ArmSimdSaturatingForm::Vector64,
            _ => ArmSimdSaturatingForm::Vector128,
        };
        let operation = ArmSimdSaturatingOp {
            dst,
            src1,
            src2,
            elem: Self::neon_integer_element(size)?,
            kind,
            signed,
            form,
        };
        if !operation.shape_valid() {
            return Err(LiftError::Internal(
                "invalid A64 saturating operation shape".to_string(),
            ));
        }
        Ok(Some(OpKind::ArmSimdSaturating(operation)))
    }
}
