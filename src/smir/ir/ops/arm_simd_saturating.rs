//! A64 same-width integer saturation with explicit cumulative FPSR.QC effects.

use crate::smir::ir::types::{ArchReg, ArmReg, VReg, VecElementType};

/// Active destination extent. Every form clears unused destination bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSimdSaturatingForm {
    Scalar,
    Vector64,
    Vector128,
}

/// Arithmetic before clipping to the element range. Register shifts use only
/// the signed low byte of each shift lane. MixedAccumulate interprets src1
/// with the opposite signedness to the old destination and result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSimdSaturatingKind {
    Add,
    Subtract,
    MixedAccumulate,
    Absolute,
    Negate,
    ShiftRegister { rounding: bool },
    ShiftImmediate { amount: u8 },
    ShiftSignedToUnsigned { amount: u8 },
    DoublingMultiplyHigh { rounding: bool, lane: Option<u8> },
}

/// A64 integer saturation. `signed` selects the result range; Absolute,
/// Negate, and DoublingMultiplyHigh require signed results, while
/// ShiftSignedToUnsigned requires unsigned results and a signed source.
/// Saturation ORs FPSR.QC without modifying NZCV or other FPSR bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmSimdSaturatingOp {
    pub dst: VReg,
    pub src1: VReg,
    pub src2: Option<VReg>,
    pub elem: VecElementType,
    pub kind: ArmSimdSaturatingKind,
    pub signed: bool,
    pub form: ArmSimdSaturatingForm,
}

fn vector_operand_valid(reg: VReg) -> bool {
    matches!(reg, VReg::Virtual(_))
        || matches!(reg, VReg::Arch(ArchReg::Arm(ArmReg::V(index))) if index < 32)
}

impl ArmSimdSaturatingOp {
    /// Validate architectural widths, source roles, and indexed restrictions.
    pub fn shape_valid(&self) -> bool {
        if !matches!(
            self.elem,
            VecElementType::I8 | VecElementType::I16 | VecElementType::I32 | VecElementType::I64
        ) || !vector_operand_valid(self.dst)
            || !vector_operand_valid(self.src1)
            || self.src2.is_some_and(|reg| !vector_operand_valid(reg))
            || (self.form == ArmSimdSaturatingForm::Vector64 && self.elem == VecElementType::I64)
        {
            return false;
        }
        let bits = self.elem.bytes() * 8;
        match self.kind {
            ArmSimdSaturatingKind::Add
            | ArmSimdSaturatingKind::Subtract
            | ArmSimdSaturatingKind::ShiftRegister { .. } => self.src2.is_some(),
            ArmSimdSaturatingKind::MixedAccumulate => self.src2.is_none(),
            ArmSimdSaturatingKind::Absolute | ArmSimdSaturatingKind::Negate => {
                self.src2.is_none() && self.signed
            }
            ArmSimdSaturatingKind::ShiftImmediate { amount } => {
                self.src2.is_none() && u32::from(amount) < bits
            }
            ArmSimdSaturatingKind::ShiftSignedToUnsigned { amount } => {
                self.src2.is_none() && !self.signed && u32::from(amount) < bits
            }
            ArmSimdSaturatingKind::DoublingMultiplyHigh { lane, .. } => {
                let indexed_source_valid = match (self.elem, lane, self.src2) {
                    (
                        VecElementType::I16,
                        Some(_),
                        Some(VReg::Arch(ArchReg::Arm(ArmReg::V(index)))),
                    ) => index < 16,
                    _ => true,
                };
                self.signed
                    && matches!(self.elem, VecElementType::I16 | VecElementType::I32)
                    && self.src2.is_some()
                    && lane.is_none_or(|index| u32::from(index) < 128 / bits)
                    && indexed_source_valid
            }
        }
    }

    pub(crate) fn sources(&self) -> Vec<VReg> {
        let mut sources = vec![self.src1];
        sources.extend(self.src2);
        if self.kind == ArmSimdSaturatingKind::MixedAccumulate {
            sources.push(self.dst);
        }
        sources.push(VReg::Arch(ArchReg::Arm(ArmReg::Fpsr)));
        sources
    }

    pub(crate) fn destinations(&self) -> Vec<VReg> {
        vec![self.dst, VReg::Arch(ArchReg::Arm(ArmReg::Fpsr))]
    }
}
