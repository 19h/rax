//! AArch64 AdvSIMD integer width transitions and cumulative saturation.
//!
//! These operations read 128-bit vector state and leave NZCV unchanged. Long
//! operations select a 64-bit source half or one scalar element; narrowing
//! operations write 64 bits, merge the upper half, or write one scalar element.
//! Saturation ORs FPSR.QC (bit 27) without changing any other status bit.

use crate::smir::ir::types::{ArchReg, ArmReg, VReg, VecElementType, VecWidth};

pub(crate) const ARM_FPSR_QC: u32 = 1 << 27;

/// Source-half selection for long operations, or destination-part selection
/// for narrow operations. Scalar operations clear all unused destination bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSimdPart {
    LowerHalf,
    UpperHalf,
    Scalar,
}

/// Arithmetic performed after extending narrow inputs to twice their width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSimdLongKind {
    Add,
    Subtract,
    AddWide,
    SubtractWide,
    AbsoluteDifference,
    AbsoluteDifferenceAccumulate,
    Multiply,
    MultiplyAccumulate,
    MultiplySubtract,
    SaturatingDoublingMultiply,
    SaturatingDoublingMultiplyAccumulate,
    SaturatingDoublingMultiplySubtract,
    ShiftLeft { amount: u8 },
    PairwiseAdd { width: VecWidth, accumulate: bool },
}

/// A64 long arithmetic. `elem` describes the narrow source elements (8, 16,
/// or 32 bits). Results have twice that width. AddWide/SubtractWide read src1
/// at the result width. Accumulating forms read the old destination before any
/// write. Saturating doubling forms clamp the product, then the accumulation,
/// independently, setting QC if either clamp changes a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmSimdLongOp {
    pub dst: VReg,
    pub src1: VReg,
    pub src2: Option<VReg>,
    pub elem: VecElementType,
    pub kind: ArmSimdLongKind,
    pub signed: bool,
    pub part: ArmSimdPart,
}

/// Interpretation and clipping applied to a wide source before narrowing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSimdNarrowMode {
    Truncate,
    SignedSaturate,
    UnsignedSaturate,
    SignedToUnsigned,
}

/// Narrowing arithmetic. Rounded shifts add the bias in an unbounded signed
/// or unsigned domain before shifting, including 64-bit source lanes. High
/// arithmetic wraps at the wide lane width before extracting its upper half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmSimdNarrowKind {
    Extract {
        mode: ArmSimdNarrowMode,
    },
    HighArithmetic {
        subtract: bool,
        rounding: bool,
    },
    ShiftRight {
        amount: u8,
        rounding: bool,
        mode: ArmSimdNarrowMode,
    },
}

/// A64 narrow arithmetic. `elem` is the destination element width (8, 16,
/// or 32 bits); sources are twice that width. UpperHalf preserves the old
/// low 64 bits. LowerHalf clears the upper 64 bits. Scalar writes only lane
/// zero and clears every other bit. Saturating modes update cumulative QC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmSimdNarrowOp {
    pub dst: VReg,
    pub src1: VReg,
    pub src2: Option<VReg>,
    pub elem: VecElementType,
    pub kind: ArmSimdNarrowKind,
    pub part: ArmSimdPart,
}

fn vector_operand_valid(reg: VReg) -> bool {
    matches!(reg, VReg::Virtual(_))
        || matches!(reg, VReg::Arch(ArchReg::Arm(ArmReg::V(index))) if index < 32)
}

fn narrow_element_valid(elem: VecElementType) -> bool {
    matches!(
        elem,
        VecElementType::I8 | VecElementType::I16 | VecElementType::I32
    )
}

impl ArmSimdLongOp {
    pub fn updates_qc(&self) -> bool {
        matches!(
            self.kind,
            ArmSimdLongKind::SaturatingDoublingMultiply
                | ArmSimdLongKind::SaturatingDoublingMultiplyAccumulate
                | ArmSimdLongKind::SaturatingDoublingMultiplySubtract
        )
    }

    pub fn reads_destination(&self) -> bool {
        matches!(
            self.kind,
            ArmSimdLongKind::AbsoluteDifferenceAccumulate
                | ArmSimdLongKind::MultiplyAccumulate
                | ArmSimdLongKind::MultiplySubtract
                | ArmSimdLongKind::SaturatingDoublingMultiplyAccumulate
                | ArmSimdLongKind::SaturatingDoublingMultiplySubtract
                | ArmSimdLongKind::PairwiseAdd {
                    accumulate: true,
                    ..
                }
        )
    }

    pub fn shape_valid(&self) -> bool {
        if !narrow_element_valid(self.elem)
            || !vector_operand_valid(self.dst)
            || !vector_operand_valid(self.src1)
            || self.src2.is_some_and(|reg| !vector_operand_valid(reg))
        {
            return false;
        }
        if self.updates_qc() && (!self.signed || self.elem == VecElementType::I8) {
            return false;
        }
        if self.part == ArmSimdPart::Scalar && !self.updates_qc() {
            return false;
        }
        match self.kind {
            ArmSimdLongKind::ShiftLeft { amount } => {
                self.src2.is_none()
                    && u32::from(amount) <= self.elem.bytes() * 8
                    && (u32::from(amount) != self.elem.bytes() * 8 || !self.signed)
            }
            ArmSimdLongKind::PairwiseAdd { width, .. } => {
                self.src2.is_none()
                    && self.part == ArmSimdPart::LowerHalf
                    && matches!(width, VecWidth::V64 | VecWidth::V128)
            }
            _ => self.src2.is_some(),
        }
    }

    pub(crate) fn sources(&self) -> Vec<VReg> {
        let mut sources = vec![self.src1];
        sources.extend(self.src2);
        if self.reads_destination() {
            sources.push(self.dst);
        }
        if self.updates_qc() {
            sources.push(VReg::Arch(ArchReg::Arm(ArmReg::Fpsr)));
        }
        sources
    }

    pub(crate) fn destinations(&self) -> Vec<VReg> {
        let mut destinations = vec![self.dst];
        if self.updates_qc() {
            destinations.push(VReg::Arch(ArchReg::Arm(ArmReg::Fpsr)));
        }
        destinations
    }
}

impl ArmSimdNarrowOp {
    pub fn updates_qc(&self) -> bool {
        matches!(
            self.kind,
            ArmSimdNarrowKind::Extract { mode }
                | ArmSimdNarrowKind::ShiftRight { mode, .. }
                if mode != ArmSimdNarrowMode::Truncate
        )
    }

    pub fn shape_valid(&self) -> bool {
        if !narrow_element_valid(self.elem)
            || !vector_operand_valid(self.dst)
            || !vector_operand_valid(self.src1)
            || self.src2.is_some_and(|reg| !vector_operand_valid(reg))
            || (self.part == ArmSimdPart::Scalar && !self.updates_qc())
        {
            return false;
        }
        match self.kind {
            ArmSimdNarrowKind::HighArithmetic { .. } => self.src2.is_some(),
            ArmSimdNarrowKind::Extract { .. } => self.src2.is_none(),
            ArmSimdNarrowKind::ShiftRight { amount, .. } => {
                self.src2.is_none() && (1..=self.elem.bytes() * 8).contains(&u32::from(amount))
            }
        }
    }

    pub(crate) fn sources(&self) -> Vec<VReg> {
        let mut sources = vec![self.src1];
        sources.extend(self.src2);
        if self.part == ArmSimdPart::UpperHalf {
            sources.push(self.dst);
        }
        if self.updates_qc() {
            sources.push(VReg::Arch(ArchReg::Arm(ArmReg::Fpsr)));
        }
        sources
    }

    pub(crate) fn destinations(&self) -> Vec<VReg> {
        let mut destinations = vec![self.dst];
        if self.updates_qc() {
            destinations.push(VReg::Arch(ArchReg::Arm(ArmReg::Fpsr)));
        }
        destinations
    }
}
