//! Exact register-only legacy `PCLMULQDQ` replay classification and semantic
//! graph validation.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

/// Decoded architectural operands and immediate of one exact register-only
/// legacy `PCLMULQDQ` instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyPclmulqdqReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) immediate: u8,
}

/// Expected block-wide definition/use counts for one temporary elided by
/// exact native replay.
pub(crate) type X86LegacyPclmulqdqVirtualRequirement = (VReg, usize, usize);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
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
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane => Some(scalar),
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
                elem: VecElementType::I64,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && actual_lane == lane
        )
}

/// Validate the complete stable extract/multiply/build/legacy-merge graph
/// emitted for one register-only legacy `PCLMULQDQ`. Each returned tuple is
/// `(virtual register, definitions, uses)` so the grouping layer proves that
/// no elided temporary escapes the source instruction.
pub(crate) fn x86_legacy_pclmulqdq_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyPclmulqdqReplay,
) -> Option<Vec<X86LegacyPclmulqdqVirtualRequirement>> {
    let [
        extract_lhs,
        extract_rhs,
        multiply,
        zero,
        broadcast,
        insert_product_low,
        insert_product_high,
        move_result,
        extract_result_low,
        extract_result_high,
        commit_low,
        commit_high,
    ] = ops
    else {
        return None;
    };

    let destination = xmm(replay.destination);
    let source = xmm(replay.source);
    let lhs = exact_extract(extract_lhs, destination, replay.immediate & 1)?;
    let rhs = exact_extract(extract_rhs, source, (replay.immediate >> 4) & 1)?;
    if multiply.x86_hint.is_some() {
        return None;
    }
    let (product_low, product_high) = match multiply.kind {
        OpKind::ClMul {
            dst: product_low @ VReg::Virtual(_),
            dst_hi: Some(product_high @ VReg::Virtual(_)),
            src1: SrcOperand::Reg(actual_lhs),
            src2: SrcOperand::Reg(actual_rhs),
            elem_bits: 64,
            lanes: 1,
            acc: false,
        } if actual_lhs == lhs && actual_rhs == rhs => (product_low, product_high),
        _ => return None,
    };

    if zero.x86_hint.is_some() {
        return None;
    }
    let zero_scalar = match zero.kind {
        OpKind::Mov {
            dst: scalar @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => scalar,
        _ => return None,
    };
    if broadcast.x86_hint.is_some() {
        return None;
    }
    let output = match broadcast.kind {
        OpKind::VBroadcast {
            dst: output @ VReg::Virtual(_),
            scalar,
            elem: VecElementType::I64,
            lanes: 2,
        } if scalar == zero_scalar => output,
        _ => return None,
    };
    if !exact_insert(insert_product_low, output, product_low, 0)
        || !exact_insert(insert_product_high, output, product_high, 1)
    {
        return None;
    }

    if move_result.x86_hint.is_some() {
        return None;
    }
    let raw = match move_result.kind {
        OpKind::VMov {
            dst: raw @ VReg::Virtual(_),
            src,
            width: VecWidth::V128,
        } if src == output => raw,
        _ => return None,
    };
    let result_low = exact_extract(extract_result_low, raw, 0)?;
    let result_high = exact_extract(extract_result_high, raw, 1)?;
    if !exact_insert(commit_low, destination, result_low, 0)
        || !exact_insert(commit_high, destination, result_high, 1)
    {
        return None;
    }

    let requirements = vec![
        (lhs, 1, 1),
        (rhs, 1, 1),
        (product_low, 1, 1),
        (product_high, 1, 1),
        (zero_scalar, 1, 1),
        (output, 3, 3),
        (raw, 1, 2),
        (result_low, 1, 1),
        (result_high, 1, 1),
    ];
    let mut unique = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| unique.insert(*register))
        .then_some(requirements)
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy `PCLMULQDQ`.
    ///
    /// The mandatory 66H prefix may be followed by one final REX prefix.
    /// REX.R/B extend the XMM operands; REX.W/X are unused and retained in the
    /// exact replay bytes. Every imm8 value is accepted because only bits 0
    /// and 4 select source qwords and all other bits are architecturally
    /// ignored. Memory, other/reordered prefixes, duplicate REX, REX2,
    /// VEX/EVEX, truncated, and trailing-byte forms fail closed.
    pub(crate) fn legacy_register_pclmulqdq_replay(&self) -> Option<X86LegacyPclmulqdqReplay> {
        let (rex, modrm, immediate) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, 0x0F, 0x3A, 0x44, modrm, immediate] => {
                (Some(*rex), *modrm, *immediate)
            }
            [0x66, 0x0F, 0x3A, 0x44, modrm, immediate] => (None, *modrm, *immediate),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let rex = rex.unwrap_or(0);
        Some(X86LegacyPclmulqdqReplay {
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
            immediate,
        })
    }
}
