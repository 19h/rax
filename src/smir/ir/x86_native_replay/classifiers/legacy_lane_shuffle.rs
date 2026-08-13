//! Exact register-only legacy SSE2/SSE3 lane-shuffle replay.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, X86Reg,
};

/// Exact legacy lane-shuffle operation selected by its mandatory prefix and
/// opcode byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86LegacyLaneShuffleKind {
    MovDDup,
    MovShDup,
    MovSlDup,
    PshufD,
    PshufHighW,
    PshufLowW,
}

impl X86LegacyLaneShuffleKind {
    pub(crate) fn requires_sse3(self) -> bool {
        matches!(self, Self::MovDDup | Self::MovShDup | Self::MovSlDup)
    }

    fn element(self) -> VecElementType {
        match self {
            Self::MovDDup => VecElementType::F64,
            Self::MovShDup | Self::MovSlDup => VecElementType::F32,
            Self::PshufD => VecElementType::I32,
            Self::PshufHighW | Self::PshufLowW => VecElementType::I16,
        }
    }

    fn lanes(self) -> u8 {
        match self {
            Self::MovDDup => 2,
            Self::MovShDup | Self::MovSlDup | Self::PshufD => 4,
            Self::PshufHighW | Self::PshufLowW => 8,
        }
    }

    fn selector(self, lane: u8, immediate: Option<u8>) -> Option<u8> {
        match (self, immediate) {
            (Self::MovDDup, None) => Some(0),
            (Self::MovShDup, None) => Some((lane & !1) | 1),
            (Self::MovSlDup, None) => Some(lane & !1),
            (Self::PshufD, Some(immediate)) => Some((immediate >> (2 * lane)) & 3),
            (Self::PshufHighW, Some(immediate)) if lane < 4 => Some(lane),
            (Self::PshufHighW, Some(immediate)) => Some(4 + ((immediate >> (2 * (lane - 4))) & 3)),
            (Self::PshufLowW, Some(immediate)) if lane < 4 => Some((immediate >> (2 * lane)) & 3),
            (Self::PshufLowW, Some(_)) => Some(lane),
            _ => None,
        }
    }
}

/// Decoded architectural operands and immediate of one exact register-only
/// legacy XMM lane shuffle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyLaneShuffleReplay {
    pub(crate) kind: X86LegacyLaneShuffleKind,
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) immediate: Option<u8>,
}

/// Expected block-wide definition/use counts for one temporary elided by
/// exact native replay.
pub(crate) type X86LegacyLaneShuffleVirtualRequirement = (VReg, usize, usize);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn exact_immediate(operation: &SmirOp, expected: u8) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::Mov {
            dst: scalar @ VReg::Virtual(_),
            src: SrcOperand::Imm(value),
            width: OpWidth::W64,
        } if value == i64::from(expected) => Some(scalar),
        _ => None,
    }
}

fn exact_insert(
    operation: &SmirOp,
    vector: VReg,
    scalar: VReg,
    lane: u8,
    element: VecElementType,
) -> bool {
    operation.x86_hint.is_none()
        && matches!(
            operation.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && actual_lane == lane
                && elem == element
        )
}

fn exact_extract(
    operation: &SmirOp,
    vector: VReg,
    lane: u8,
    element: VecElementType,
) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane && elem == element => Some(scalar),
        _ => None,
    }
}

/// Validate the complete stable semantic graph emitted for one register-only
/// legacy lane shuffle. Each returned tuple is `(virtual register,
/// definitions, uses)` so the grouping layer can prove that every elided
/// reconstruction temporary is confined to this source instruction.
pub(crate) fn x86_legacy_lane_shuffle_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyLaneShuffleReplay,
) -> Option<Vec<X86LegacyLaneShuffleVirtualRequirement>> {
    let element = replay.kind.element();
    let lanes = replay.kind.lanes();
    let lane_count = usize::from(lanes);
    if ops.len() != 4 * lane_count + 3 {
        return None;
    }

    let zero = exact_immediate(&ops[0], 0)?;
    let indices = match ops[1].kind {
        OpKind::VBroadcast {
            dst: indices @ VReg::Virtual(_),
            scalar,
            elem,
            lanes: actual_lanes,
        } if scalar == zero && elem == element && actual_lanes == lanes => indices,
        _ => return None,
    };
    if ops[1].x86_hint.is_some() {
        return None;
    }

    let mut requirements = Vec::with_capacity(2 * lane_count + 3);
    requirements.push((zero, 1, 1));
    requirements.push((indices, lane_count + 1, lane_count + 1));
    for lane in 0..lanes {
        let selector = replay.kind.selector(lane, replay.immediate)?;
        let scalar = exact_immediate(&ops[2 + 2 * usize::from(lane)], selector)?;
        if !exact_insert(
            &ops[3 + 2 * usize::from(lane)],
            indices,
            scalar,
            lane,
            element,
        ) {
            return None;
        }
        requirements.push((scalar, 1, 1));
    }

    let raw_index = 2 + 2 * lane_count;
    let raw = match ops[raw_index].kind {
        OpKind::VShuffle {
            dst: raw @ VReg::Virtual(_),
            src1,
            src2: None,
            indices: actual_indices,
            elem,
            lanes: actual_lanes,
        } if src1 == xmm(replay.source)
            && actual_indices == indices
            && elem == element
            && actual_lanes == lanes =>
        {
            raw
        }
        _ => return None,
    };
    if ops[raw_index].x86_hint.is_some() {
        return None;
    }
    requirements.push((raw, 1, lane_count));

    for lane in 0..lanes {
        let lane_index = usize::from(lane);
        let scalar = exact_extract(&ops[raw_index + 1 + lane_index], raw, lane, element)?;
        if !exact_insert(
            &ops[raw_index + 1 + lane_count + lane_index],
            xmm(replay.destination),
            scalar,
            lane,
            element,
        ) {
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
    /// Decode one exact canonical register-only legacy `MOVDDUP`, `MOVSHDUP`,
    /// `MOVSLDUP`, `PSHUFD`, `PSHUFHW`, or `PSHUFLW` instruction.
    ///
    /// Exactly one mandatory F2H, F3H, or 66H prefix followed by one optional
    /// final REX prefix is accepted. REX.R/B extend the XMM operands; REX.W/X
    /// are ignored architecturally and retained in the exact replay bytes.
    /// Memory, other or duplicate/reordered prefixes, REX2/VEX/EVEX,
    /// truncated instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_lane_shuffle_replay(&self) -> Option<X86LegacyLaneShuffleReplay> {
        let (mandatory, rex, tail) = match self.as_slice() {
            [
                mandatory @ (0x66 | 0xF2 | 0xF3),
                rex @ 0x40..=0x4F,
                tail @ ..,
            ] => (*mandatory, Some(*rex), tail),
            [mandatory @ (0x66 | 0xF2 | 0xF3), tail @ ..] => (*mandatory, None, tail),
            _ => return None,
        };
        let (kind, modrm, immediate) = match (mandatory, tail) {
            (0xF2, [0x0F, 0x12, modrm]) => (X86LegacyLaneShuffleKind::MovDDup, *modrm, None),
            (0xF3, [0x0F, 0x16, modrm]) => (X86LegacyLaneShuffleKind::MovShDup, *modrm, None),
            (0xF3, [0x0F, 0x12, modrm]) => (X86LegacyLaneShuffleKind::MovSlDup, *modrm, None),
            (0x66, [0x0F, 0x70, modrm, immediate]) => {
                (X86LegacyLaneShuffleKind::PshufD, *modrm, Some(*immediate))
            }
            (0xF3, [0x0F, 0x70, modrm, immediate]) => (
                X86LegacyLaneShuffleKind::PshufHighW,
                *modrm,
                Some(*immediate),
            ),
            (0xF2, [0x0F, 0x70, modrm, immediate]) => (
                X86LegacyLaneShuffleKind::PshufLowW,
                *modrm,
                Some(*immediate),
            ),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let rex = rex.unwrap_or(0);
        Some(X86LegacyLaneShuffleReplay {
            kind,
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
            immediate,
        })
    }
}
