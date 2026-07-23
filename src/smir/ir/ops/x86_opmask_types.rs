//! Structured AVX-512 opmask-operation payloads.

use crate::smir::ir::types::{Address, OpWidth, VReg};

/// Binary operation selected by the VEX-encoded AVX-512 opmask family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86OpmaskBinaryKind {
    Add,
    And,
    AndNot,
    Or,
    Xnor,
    Xor,
}

/// Flag-only operation selected by KTEST or KORTEST.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86OpmaskTestKind {
    And,
    Or,
}

/// Immediate shift direction selected by KSHIFTL or KSHIFTR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86OpmaskShiftKind {
    Left,
    Right,
}

/// Architecturally distinct source classes accepted by KMOV.
#[derive(Clone, Debug, PartialEq)]
pub enum X86OpmaskMoveSource {
    Mask(VReg),
    Gpr(VReg),
    Memory(Address),
}

/// Architecturally distinct destination classes accepted by KMOV.
#[derive(Clone, Debug, PartialEq)]
pub enum X86OpmaskMoveDestination {
    Gpr(VReg),
    Memory(Address),
}

/// One complete VEX-encoded AVX-512 opmask instruction.
///
/// Opmask destinations are always committed as zero-extended 64-bit K values,
/// even for byte, word, or dword forms. Keeping each instruction atomic in the
/// IR preserves KMOV memory-fault precision and prevents generic scalar
/// register allocation from treating architectural K registers as GPRs.
#[derive(Clone, Debug, PartialEq)]
pub enum X86OpmaskOp {
    MoveToMask {
        dst: VReg,
        src: X86OpmaskMoveSource,
        width: OpWidth,
    },
    MoveFromMask {
        dst: X86OpmaskMoveDestination,
        src: VReg,
        width: OpWidth,
    },
    Not {
        dst: VReg,
        src: VReg,
        width: OpWidth,
    },
    Binary {
        kind: X86OpmaskBinaryKind,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
    },
    /// Concatenate the low half of `src1` above the low half of `src2`.
    Unpack {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
    },
    Shift {
        kind: X86OpmaskShiftKind,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        count: u8,
    },
    Test {
        kind: X86OpmaskTestKind,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
    },
}

impl X86OpmaskOp {
    pub fn dests(&self) -> Vec<VReg> {
        match self {
            Self::MoveToMask { dst, .. }
            | Self::Not { dst, .. }
            | Self::Binary { dst, .. }
            | Self::Unpack { dst, .. }
            | Self::Shift { dst, .. } => vec![*dst],
            Self::MoveFromMask {
                dst: X86OpmaskMoveDestination::Gpr(dst),
                ..
            } => vec![*dst],
            Self::MoveFromMask {
                dst: X86OpmaskMoveDestination::Memory(_),
                ..
            }
            | Self::Test { .. } => vec![],
        }
    }

    pub fn source_vregs(&self) -> Vec<VReg> {
        match self {
            Self::MoveToMask {
                src: X86OpmaskMoveSource::Mask(src) | X86OpmaskMoveSource::Gpr(src),
                ..
            }
            | Self::Not { src, .. }
            | Self::Shift { src, .. } => vec![*src],
            Self::MoveToMask {
                src: X86OpmaskMoveSource::Memory(addr),
                ..
            } => addr.regs(),
            Self::MoveFromMask { dst, src, .. } => {
                let mut regs = vec![*src];
                if let X86OpmaskMoveDestination::Memory(addr) = dst {
                    regs.extend(addr.regs());
                }
                regs
            }
            Self::Binary { src1, src2, .. }
            | Self::Unpack { src1, src2, .. }
            | Self::Test { src1, src2, .. } => vec![*src1, *src2],
        }
    }

    pub fn reads_memory(&self) -> bool {
        matches!(
            self,
            Self::MoveToMask {
                src: X86OpmaskMoveSource::Memory(_),
                ..
            }
        )
    }

    pub fn writes_memory(&self) -> bool {
        matches!(
            self,
            Self::MoveFromMask {
                dst: X86OpmaskMoveDestination::Memory(_),
                ..
            }
        )
    }

    pub fn memory_address(&self) -> Option<&Address> {
        match self {
            Self::MoveToMask {
                src: X86OpmaskMoveSource::Memory(addr),
                ..
            }
            | Self::MoveFromMask {
                dst: X86OpmaskMoveDestination::Memory(addr),
                ..
            } => Some(addr),
            _ => None,
        }
    }

    pub fn width(&self) -> OpWidth {
        match self {
            Self::MoveToMask { width, .. }
            | Self::MoveFromMask { width, .. }
            | Self::Not { width, .. }
            | Self::Binary { width, .. }
            | Self::Unpack { width, .. }
            | Self::Shift { width, .. }
            | Self::Test { width, .. } => *width,
        }
    }

    pub fn is_test(&self) -> bool {
        matches!(self, Self::Test { .. })
    }
}
