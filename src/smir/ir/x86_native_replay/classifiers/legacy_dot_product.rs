//! Exact register-only legacy SSE4.1 floating-point dot-product replay.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, SignExtend, VReg, VecElementType, VecWidth, X86Reg};

/// Decoded architectural operands and controls of one exact register-only
/// legacy SSE4.1 `DPPS` or `DPPD` instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyDotProductReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) elem: VecElementType,
    pub(crate) lanes: u8,
    pub(crate) immediate: u8,
}

/// Expected block-wide definition/use counts for one temporary elided by
/// exact native replay.
pub(crate) type X86LegacyDotProductVirtualRequirement = (VReg, usize, usize);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn exact_extract(operation: &SmirOp, vector: VReg, lane: u8, elem: VecElementType) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem: actual_elem,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane && actual_elem == elem => Some(scalar),
        _ => None,
    }
}

fn exact_insert(
    operation: &SmirOp,
    destination: u8,
    scalar: VReg,
    lane: u8,
    elem: VecElementType,
) -> bool {
    operation.x86_hint.is_none()
        && matches!(
            operation.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: actual_elem,
            } if dst == xmm(destination)
                && vec == xmm(destination)
                && actual_scalar == scalar
                && actual_lane == lane
                && actual_elem == elem
        )
}

/// Validate the complete stable semantic graph emitted for one register-only
/// legacy dot product. Each returned tuple is `(virtual register,
/// definitions, uses)` so the grouping layer can prove that no elided
/// temporary escapes the source instruction.
pub(crate) fn x86_legacy_dot_product_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyDotProductReplay,
) -> Option<Vec<X86LegacyDotProductVirtualRequirement>> {
    let lanes = usize::from(replay.lanes);
    if ops.len() != 1 + 2 * lanes || ops[0].x86_hint.is_some() {
        return None;
    }

    let raw = match ops[0].kind {
        OpKind::X86DotProduct {
            dst: raw @ VReg::Virtual(_),
            src1,
            src2,
            elem,
            width: VecWidth::V128,
            imm,
        } if src1 == xmm(replay.destination)
            && src2 == xmm(replay.source)
            && elem == replay.elem
            && imm == replay.immediate =>
        {
            raw
        }
        _ => return None,
    };

    let mut requirements = Vec::with_capacity(lanes + 1);
    requirements.push((raw, 1, lanes));
    for lane in 0..replay.lanes {
        let scalar = exact_extract(&ops[1 + usize::from(lane)], raw, lane, replay.elem)?;
        if !exact_insert(
            &ops[1 + lanes + usize::from(lane)],
            replay.destination,
            scalar,
            lane,
            replay.elem,
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
    /// Decode one exact register-only legacy SSE4.1 `DPPS` or `DPPD`.
    ///
    /// Both forms require mandatory 66H followed by an optional final REX,
    /// map 0F3A, a register ModR/M source, and an imm8. REX.R/B extend the two
    /// XMM operands; REX.W/X and the architecturally unused immediate bits are
    /// ignored by execution but retained in the exact replay bytes. Memory,
    /// other or reordered prefixes, non-final or duplicate REX, REX2/VEX/EVEX,
    /// truncated, and trailing-byte forms fail closed.
    pub(crate) fn legacy_register_dot_product_replay(&self) -> Option<X86LegacyDotProductReplay> {
        let (rex, opcode, modrm, immediate) = match self.as_slice() {
            [
                0x66,
                rex @ 0x40..=0x4F,
                0x0F,
                0x3A,
                opcode,
                modrm,
                immediate,
            ] => (Some(*rex), *opcode, *modrm, *immediate),
            [0x66, 0x0F, 0x3A, opcode, modrm, immediate] => (None, *opcode, *modrm, *immediate),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let (elem, lanes) = match opcode {
            0x40 => (VecElementType::F32, 4),
            0x41 => (VecElementType::F64, 2),
            _ => return None,
        };
        let rex = rex.unwrap_or(0);
        Some(X86LegacyDotProductReplay {
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
            elem,
            lanes,
            immediate,
        })
    }
}
