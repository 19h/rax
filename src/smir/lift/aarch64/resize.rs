//! A64 long and narrow arithmetic with explicit lane width and part selection.

use crate::isa::arm::ExecutionState;
use crate::isa::arm::decoder::{DecodedInsn, Mnemonic, Operand};
use crate::smir::ir::ops::{
    ArmSimdLongKind, ArmSimdLongOp, ArmSimdNarrowKind, ArmSimdNarrowMode, ArmSimdNarrowOp,
    ArmSimdPart, OpKind,
};
use crate::smir::ir::types::VecWidth;
use crate::smir::lift::LiftError;

use super::Aarch64Lifter;

impl Aarch64Lifter {
    pub(super) fn lift_neon_resize(&self, insn: &DecodedInsn) -> Result<Option<OpKind>, LiftError> {
        if insn.state != ExecutionState::Aarch64 {
            return Ok(None);
        }
        let supported = matches!(
            insn.mnemonic,
            Mnemonic::VADDL
                | Mnemonic::VADDW
                | Mnemonic::VSUBL
                | Mnemonic::VSUBW
                | Mnemonic::VABDL
                | Mnemonic::VABAL
                | Mnemonic::VMULL
                | Mnemonic::VMLAL
                | Mnemonic::VMLSL
                | Mnemonic::VQDMULL
                | Mnemonic::VQDMLAL
                | Mnemonic::VQDMLSL
                | Mnemonic::VADDHN
                | Mnemonic::VRADDHN
                | Mnemonic::VSUBHN
                | Mnemonic::VRSUBHN
                | Mnemonic::VMOVL
                | Mnemonic::VMOVN
                | Mnemonic::VQMOVN
                | Mnemonic::VQMOVUN
                | Mnemonic::VSHRN
                | Mnemonic::VRSHRN
                | Mnemonic::VQSHRN
                | Mnemonic::VQRSHRN
                | Mnemonic::VQSHRUN
                | Mnemonic::VQRSHRUN
                | Mnemonic::VPADDL
                | Mnemonic::VPADAL
        );
        if !supported {
            return Ok(None);
        }
        let (dst, src1) = match (insn.operands.first(), insn.operands.get(1)) {
            (Some(Operand::FpReg(dst)), Some(Operand::FpReg(src1))) => {
                (Self::fp_vreg(dst), Self::fp_vreg(src1))
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid A64 resize operands".to_string(),
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
        let part = match (scalar, q) {
            (true, _) => ArmSimdPart::Scalar,
            (false, 0) => ArmSimdPart::LowerHalf,
            _ => ArmSimdPart::UpperHalf,
        };
        let immediate_class = (raw >> 24) & 15 == 15;
        let size = if immediate_class {
            let immh = (raw >> 19) & 15;
            if immh == 0 {
                return Err(LiftError::Internal(
                    "invalid A64 resize immediate width".to_string(),
                ));
            }
            31 - immh.leading_zeros()
        } else {
            (raw >> 22) & 3
        };
        let elem = Self::neon_integer_element(size)?;
        let amount = || -> Result<u8, LiftError> {
            let Some(Operand::Imm(immediate)) = insn.operands.last() else {
                return Err(LiftError::Internal(
                    "missing A64 resize shift amount".to_string(),
                ));
            };
            u8::try_from(immediate.effective_value())
                .map_err(|_| LiftError::Internal("invalid A64 resize shift amount".to_string()))
        };
        let long_kind = match insn.mnemonic {
            Mnemonic::VADDL => Some(ArmSimdLongKind::Add),
            Mnemonic::VSUBL => Some(ArmSimdLongKind::Subtract),
            Mnemonic::VADDW => Some(ArmSimdLongKind::AddWide),
            Mnemonic::VSUBW => Some(ArmSimdLongKind::SubtractWide),
            Mnemonic::VABDL => Some(ArmSimdLongKind::AbsoluteDifference),
            Mnemonic::VABAL => Some(ArmSimdLongKind::AbsoluteDifferenceAccumulate),
            Mnemonic::VMULL => Some(ArmSimdLongKind::Multiply),
            Mnemonic::VMLAL => Some(ArmSimdLongKind::MultiplyAccumulate),
            Mnemonic::VMLSL => Some(ArmSimdLongKind::MultiplySubtract),
            Mnemonic::VQDMULL => Some(ArmSimdLongKind::SaturatingDoublingMultiply),
            Mnemonic::VQDMLAL => Some(ArmSimdLongKind::SaturatingDoublingMultiplyAccumulate),
            Mnemonic::VQDMLSL => Some(ArmSimdLongKind::SaturatingDoublingMultiplySubtract),
            Mnemonic::VMOVL => Some(ArmSimdLongKind::ShiftLeft { amount: amount()? }),
            Mnemonic::VPADDL | Mnemonic::VPADAL => Some(ArmSimdLongKind::PairwiseAdd {
                width: match q {
                    0 => VecWidth::V64,
                    _ => VecWidth::V128,
                },
                accumulate: insn.mnemonic == Mnemonic::VPADAL,
            }),
            _ => None,
        };
        if let Some(kind) = long_kind {
            let operation = ArmSimdLongOp {
                dst,
                src1,
                src2,
                elem,
                kind,
                signed: u == 0,
                part: match kind {
                    ArmSimdLongKind::PairwiseAdd { .. } => ArmSimdPart::LowerHalf,
                    _ => part,
                },
            };
            if !operation.shape_valid() {
                return Err(LiftError::Internal("invalid A64 long shape".to_string()));
            }
            return Ok(Some(OpKind::ArmSimdLong(operation)));
        }

        let mode = match insn.mnemonic {
            Mnemonic::VMOVN | Mnemonic::VSHRN | Mnemonic::VRSHRN => ArmSimdNarrowMode::Truncate,
            Mnemonic::VQMOVUN | Mnemonic::VQSHRUN | Mnemonic::VQRSHRUN => {
                ArmSimdNarrowMode::SignedToUnsigned
            }
            _ if u == 0 => ArmSimdNarrowMode::SignedSaturate,
            _ => ArmSimdNarrowMode::UnsignedSaturate,
        };
        let kind = match insn.mnemonic {
            Mnemonic::VADDHN | Mnemonic::VRADDHN | Mnemonic::VSUBHN | Mnemonic::VRSUBHN => {
                ArmSimdNarrowKind::HighArithmetic {
                    subtract: matches!(insn.mnemonic, Mnemonic::VSUBHN | Mnemonic::VRSUBHN),
                    rounding: matches!(insn.mnemonic, Mnemonic::VRADDHN | Mnemonic::VRSUBHN),
                }
            }
            Mnemonic::VMOVN | Mnemonic::VQMOVN | Mnemonic::VQMOVUN => {
                ArmSimdNarrowKind::Extract { mode }
            }
            _ => ArmSimdNarrowKind::ShiftRight {
                amount: amount()?,
                rounding: matches!(
                    insn.mnemonic,
                    Mnemonic::VRSHRN | Mnemonic::VQRSHRN | Mnemonic::VQRSHRUN
                ),
                mode,
            },
        };
        let operation = ArmSimdNarrowOp {
            dst,
            src1,
            src2,
            elem,
            kind,
            part,
        };
        if !operation.shape_valid() {
            return Err(LiftError::Internal("invalid A64 narrow shape".to_string()));
        }
        Ok(Some(OpKind::ArmSimdNarrow(operation)))
    }
}
