//! Exact register-only legacy SSSE3 `PALIGNR` replay.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, X86Reg,
};

/// Decoded architectural operands and immediate of one exact register-only
/// legacy XMM `PALIGNR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyAlignrReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) immediate: u8,
}

/// Expected block-wide definition/use counts for one temporary elided by
/// exact native replay.
pub(crate) type X86LegacyAlignrVirtualRequirement = (VReg, usize, usize);

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

fn exact_insert(operation: &SmirOp, vector: VReg, scalar: VReg, lane: u8) -> bool {
    operation.x86_hint.is_none()
        && matches!(
            operation.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: VecElementType::I8,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && actual_lane == lane
        )
}

fn exact_extract(operation: &SmirOp, vector: VReg, lane: u8) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem: VecElementType::I8,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane => Some(scalar),
        _ => None,
    }
}

/// Validate the complete 67-operation semantic graph emitted for one
/// register-only legacy XMM `PALIGNR`. Each returned tuple is `(virtual
/// register, definitions, uses)` so the grouping layer can prove that every
/// elided reconstruction temporary is confined to this source instruction.
pub(crate) fn x86_legacy_alignr_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyAlignrReplay,
) -> Option<Vec<X86LegacyAlignrVirtualRequirement>> {
    const LANES: u8 = 16;
    const RAW_INDEX: usize = 34;
    if ops.len() != 67 {
        return None;
    }

    let zero = exact_immediate(&ops[0], 0)?;
    let indices = match ops[1].kind {
        OpKind::VBroadcast {
            dst: indices @ VReg::Virtual(_),
            scalar,
            elem: VecElementType::I8,
            lanes: LANES,
        } if scalar == zero => indices,
        _ => return None,
    };
    if ops[1].x86_hint.is_some() {
        return None;
    }

    let mut requirements = Vec::with_capacity(35);
    requirements.push((zero, 1, 1));
    requirements.push((indices, usize::from(LANES) + 1, usize::from(LANES) + 1));
    for lane in 0..LANES {
        let selector = (u16::from(replay.immediate) + u16::from(lane)).min(32) as u8;
        let scalar = exact_immediate(&ops[2 + 2 * usize::from(lane)], selector)?;
        if !exact_insert(&ops[3 + 2 * usize::from(lane)], indices, scalar, lane) {
            return None;
        }
        requirements.push((scalar, 1, 1));
    }

    let raw = match ops[RAW_INDEX].kind {
        OpKind::VShuffle {
            dst: raw @ VReg::Virtual(_),
            src1,
            src2: Some(src2),
            indices: actual_indices,
            elem: VecElementType::I8,
            lanes: LANES,
        } if src1 == xmm(replay.source)
            && src2 == xmm(replay.destination)
            && actual_indices == indices =>
        {
            raw
        }
        _ => return None,
    };
    if ops[RAW_INDEX].x86_hint.is_some() {
        return None;
    }
    requirements.push((raw, 1, usize::from(LANES)));

    for lane in 0..LANES {
        let lane_index = usize::from(lane);
        let scalar = exact_extract(&ops[RAW_INDEX + 1 + lane_index], raw, lane)?;
        if !exact_insert(
            &ops[RAW_INDEX + 1 + usize::from(LANES) + lane_index],
            xmm(replay.destination),
            scalar,
            lane,
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
    /// Decode one exact canonical register-only legacy XMM `PALIGNR`.
    ///
    /// Exactly one mandatory 66H prefix followed by one optional final REX
    /// prefix is accepted. REX.R/B extend the XMM operands; REX.W/X are
    /// ignored architecturally and retained in the exact replay bytes. MMX,
    /// memory, other or duplicate/reordered prefixes, REX2/VEX/EVEX,
    /// truncated instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_alignr_replay(&self) -> Option<X86LegacyAlignrReplay> {
        let (rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (Some(*rex), tail),
            [0x66, tail @ ..] => (None, tail),
            _ => return None,
        };
        let [0x0F, 0x3A, 0x0F, modrm, immediate] = tail else {
            return None;
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let rex = rex.unwrap_or(0);
        Some(X86LegacyAlignrReplay {
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
            immediate: *immediate,
        })
    }
}
