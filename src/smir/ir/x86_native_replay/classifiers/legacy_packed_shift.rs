//! Register-only legacy SSE2 packed-shift replay.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, ShiftOp, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};

/// Count source selected by one legacy packed-shift encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86LegacyPackedShiftCount {
    Immediate { amount: u8, byte_lane: bool },
    Register { source: u8 },
}

/// Decoded architectural operation of one canonical register-only legacy
/// SSE2 packed shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyPackedShiftReplay {
    pub(crate) destination: u8,
    pub(crate) elem: VecElementType,
    pub(crate) shift: ShiftOp,
    pub(crate) count: X86LegacyPackedShiftCount,
}

/// Expected block-wide definition/use counts for one virtual register elided
/// by exact legacy packed-shift replay.
pub(crate) type X86LegacyPackedShiftVirtualRequirement = (VReg, usize, usize);

fn immediate_spec(opcode: u8, group: u8) -> Option<(VecElementType, ShiftOp, bool)> {
    match (opcode, group) {
        (0x71, 2) => Some((VecElementType::I16, ShiftOp::Lsr, false)),
        (0x71, 4) => Some((VecElementType::I16, ShiftOp::Asr, false)),
        (0x71, 6) => Some((VecElementType::I16, ShiftOp::Lsl, false)),
        (0x72, 2) => Some((VecElementType::I32, ShiftOp::Lsr, false)),
        (0x72, 4) => Some((VecElementType::I32, ShiftOp::Asr, false)),
        (0x72, 6) => Some((VecElementType::I32, ShiftOp::Lsl, false)),
        (0x73, 2) => Some((VecElementType::I64, ShiftOp::Lsr, false)),
        (0x73, 3) => Some((VecElementType::I8, ShiftOp::Lsr, true)),
        (0x73, 6) => Some((VecElementType::I64, ShiftOp::Lsl, false)),
        (0x73, 7) => Some((VecElementType::I8, ShiftOp::Lsl, true)),
        _ => None,
    }
}

fn register_spec(opcode: u8) -> Option<(VecElementType, ShiftOp)> {
    match opcode {
        0xD1 => Some((VecElementType::I16, ShiftOp::Lsr)),
        0xD2 => Some((VecElementType::I32, ShiftOp::Lsr)),
        0xD3 => Some((VecElementType::I64, ShiftOp::Lsr)),
        0xE1 => Some((VecElementType::I16, ShiftOp::Asr)),
        0xE2 => Some((VecElementType::I32, ShiftOp::Asr)),
        0xF1 => Some((VecElementType::I16, ShiftOp::Lsl)),
        0xF2 => Some((VecElementType::I32, ShiftOp::Lsl)),
        0xF3 => Some((VecElementType::I64, ShiftOp::Lsl)),
        _ => None,
    }
}

/// Validate the complete raw-shift/extract/legacy-merge graph emitted for one
/// register-only legacy XMM packed shift. Each returned tuple is `(virtual
/// register, definitions, uses)` so the grouping layer proves that no elided
/// temporary escapes this source instruction.
pub(crate) fn x86_legacy_packed_shift_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyPackedShiftReplay,
) -> Option<Vec<X86LegacyPackedShiftVirtualRequirement>> {
    let lanes = VecWidth::V128.lanes(replay.elem) as usize;
    if ops.len() != 1 + 2 * lanes || ops[0].x86_hint.is_some() {
        return None;
    }

    let destination = VReg::Arch(ArchReg::X86(X86Reg::Xmm(replay.destination)));
    let raw = match replay.count {
        X86LegacyPackedShiftCount::Immediate { amount, byte_lane } => match ops[0].kind {
            OpKind::X86PackedShiftImm {
                dst: raw @ VReg::Virtual(_),
                src,
                width: VecWidth::V128,
                elem,
                shift,
                amount: actual_amount,
                byte_lane: actual_byte_lane,
            } if src == destination
                && elem == replay.elem
                && shift == replay.shift
                && actual_amount == amount
                && actual_byte_lane == byte_lane =>
            {
                raw
            }
            _ => return None,
        },
        X86LegacyPackedShiftCount::Register { source } => match ops[0].kind {
            OpKind::X86PackedShift {
                dst: raw @ VReg::Virtual(_),
                src,
                count,
                width: VecWidth::V128,
                elem,
                shift,
            } if src == destination
                && count == VReg::Arch(ArchReg::X86(X86Reg::Xmm(source)))
                && elem == replay.elem
                && shift == replay.shift =>
            {
                raw
            }
            _ => return None,
        },
    };

    let mut requirements = Vec::with_capacity(lanes + 1);
    requirements.push((raw, 1, lanes));
    for lane in 0..lanes {
        let scalar = match ops[1 + lane].kind {
            OpKind::VExtractLane {
                dst: scalar @ VReg::Virtual(_),
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if vec == raw && usize::from(actual_lane) == lane && elem == replay.elem => scalar,
            _ => return None,
        };
        if ops[1 + lane].x86_hint.is_some()
            || ops[1 + lanes + lane].x86_hint.is_some()
            || !matches!(
                ops[1 + lanes + lane].kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if dst == destination
                    && vec == destination
                    && actual_scalar == scalar
                    && usize::from(actual_lane) == lane
                    && elem == replay.elem
            )
        {
            return None;
        }
        requirements.push((scalar, 1, 1));
    }

    let mut unique = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| unique.insert(*register))
        .then_some(requirements)
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy SSE2 packed shift.
    /// Only mandatory 66H followed by an optional final REX prefix is
    /// accepted. For shared-count forms REX.R/B extend destination/count;
    /// for immediate forms REX.B extends the ModR/M.r/m destination while
    /// REX.R is part of the fixed group field. REX.W/X are ignored. Memory,
    /// MMX, segment/address-size, repeat/lock, REX2, VEX/EVEX, truncated, and
    /// trailing encodings fail closed.
    pub(crate) fn legacy_register_packed_shift_replay(&self) -> Option<X86LegacyPackedShiftReplay> {
        let (rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (*rex, tail),
            [0x66, tail @ ..] => (0, tail),
            _ => return None,
        };

        match tail {
            [0x0F, opcode, modrm, amount] if modrm >> 6 == 3 => {
                let (elem, shift, byte_lane) = immediate_spec(*opcode, (modrm >> 3) & 7)?;
                Some(X86LegacyPackedShiftReplay {
                    destination: (modrm & 7) | ((rex & 1) << 3),
                    elem,
                    shift,
                    count: X86LegacyPackedShiftCount::Immediate {
                        amount: *amount,
                        byte_lane,
                    },
                })
            }
            [0x0F, opcode, modrm] if modrm >> 6 == 3 => {
                let (elem, shift) = register_spec(*opcode)?;
                Some(X86LegacyPackedShiftReplay {
                    destination: ((modrm >> 3) & 7) | ((rex & 4) << 1),
                    elem,
                    shift,
                    count: X86LegacyPackedShiftCount::Register {
                        source: (modrm & 7) | ((rex & 1) << 3),
                    },
                })
            }
            _ => None,
        }
    }
}
