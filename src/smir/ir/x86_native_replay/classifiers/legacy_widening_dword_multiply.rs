//! Register-only legacy MMX/SSE widening doubleword-multiply replay.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86X87ControlKind};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

/// Decoded architectural operands of one canonical register-only legacy
/// `PMULUDQ` or `PMULDQ` instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyWideningDwordMultiplyReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) signed: bool,
    pub(crate) mmx: bool,
}

/// Expected block-wide definition/use counts for one virtual register elided
/// by exact legacy widening-multiply replay.
pub(crate) type X86LegacyWideningDwordMultiplyVirtualRequirement = (VReg, usize, usize);

fn exact_extract(op: &SmirOp, vector: VReg, lane: u8, sign: SignExtend) -> Option<VReg> {
    if op.x86_hint.is_some() {
        return None;
    }
    match op.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem: VecElementType::I32,
            sign: actual_sign,
        } if vec == vector && actual_lane == lane && actual_sign == sign => Some(scalar),
        _ => None,
    }
}

fn exact_multiply(op: &SmirOp, destination: VReg, lhs: VReg, rhs: VReg, signed: bool) -> bool {
    if op.x86_hint.is_some() {
        return false;
    }
    match (&op.kind, signed) {
        (
            OpKind::MulS {
                dst_lo,
                dst_hi: None,
                src1,
                src2: SrcOperand::Reg(actual_rhs),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            true,
        )
        | (
            OpKind::MulU {
                dst_lo,
                dst_hi: None,
                src1,
                src2: SrcOperand::Reg(actual_rhs),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            false,
        ) => *dst_lo == destination && *src1 == lhs && *actual_rhs == rhs,
        _ => false,
    }
}

fn exact_zero_scalar(op: &SmirOp) -> Option<VReg> {
    if op.x86_hint.is_some() {
        return None;
    }
    match op.kind {
        OpKind::Mov {
            dst: scalar @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => Some(scalar),
        _ => None,
    }
}

fn exact_insert(op: &SmirOp, vector: VReg, scalar: VReg, lane: u8) -> bool {
    op.x86_hint.is_none()
        && matches!(
            op.kind,
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

/// Validate the complete extract/multiply/build/legacy-merge graph emitted for
/// one register-only legacy `PMULUDQ`/`PMULDQ`. Each returned tuple is
/// `(virtual register, definitions, uses)` so the grouping layer proves that
/// no elided temporary escapes this source instruction.
pub(crate) fn x86_legacy_widening_dword_multiply_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyWideningDwordMultiplyReplay,
) -> Option<Vec<X86LegacyWideningDwordMultiplyVirtualRequirement>> {
    let lanes = if replay.mmx { 1usize } else { 2usize };
    let expected_len = if replay.mmx { 8 } else { 15 };
    if ops.len() != expected_len || replay.mmx && replay.signed {
        return None;
    }

    let destination = VReg::Arch(ArchReg::X86(if replay.mmx {
        X86Reg::Mm(replay.destination)
    } else {
        X86Reg::Xmm(replay.destination)
    }));
    let source = VReg::Arch(ArchReg::X86(if replay.mmx {
        X86Reg::Mm(replay.source)
    } else {
        X86Reg::Xmm(replay.source)
    }));
    let sign = if replay.signed {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let mut requirements = Vec::with_capacity(if replay.mmx { 5 } else { 11 });
    let mut products = Vec::with_capacity(lanes);
    for lane in 0..lanes {
        let base = lane * 3;
        let source_lane = (lane * 2) as u8;
        let lhs = exact_extract(&ops[base], destination, source_lane, sign)?;
        let rhs = exact_extract(&ops[base + 1], source, source_lane, sign)?;
        let product = match ops[base + 2].kind {
            OpKind::MulS {
                dst_lo: product @ VReg::Virtual(_),
                ..
            } if replay.signed => product,
            OpKind::MulU {
                dst_lo: product @ VReg::Virtual(_),
                ..
            } if !replay.signed => product,
            _ => return None,
        };
        if !exact_multiply(&ops[base + 2], product, lhs, rhs, replay.signed) {
            return None;
        }
        requirements.extend([(lhs, 1, 1), (rhs, 1, 1), (product, 1, 1)]);
        products.push(product);
    }

    let zero_index = lanes * 3;
    let zero = exact_zero_scalar(&ops[zero_index])?;
    let output = match ops[zero_index + 1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem: VecElementType::I64,
            lanes: actual_lanes,
        } if scalar == zero && usize::from(actual_lanes) == lanes => vector,
        _ => return None,
    };
    if ops[zero_index + 1].x86_hint.is_some() {
        return None;
    }
    requirements.push((zero, 1, 1));
    requirements.push((output, lanes + 1, lanes + 1));

    let insert_start = zero_index + 2;
    for (lane, product) in products.into_iter().enumerate() {
        if !exact_insert(&ops[insert_start + lane], output, product, lane as u8) {
            return None;
        }
    }

    let move_index = insert_start + lanes;
    if replay.mmx {
        if ops[move_index].x86_hint.is_some()
            || !matches!(
                ops[move_index].kind,
                OpKind::VMov {
                    dst,
                    src,
                    width: VecWidth::V64,
                } if dst == destination && src == output
            )
            || ops[move_index + 1].x86_hint.is_some()
            || !matches!(
                ops[move_index + 1].kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                }
            )
        {
            return None;
        }
    } else {
        let raw = match ops[move_index].kind {
            OpKind::VMov {
                dst: raw @ VReg::Virtual(_),
                src,
                width: VecWidth::V128,
            } if src == output => raw,
            _ => return None,
        };
        if ops[move_index].x86_hint.is_some() {
            return None;
        }
        requirements.push((raw, 1, lanes));
        let result_extract_start = move_index + 1;
        let destination_insert_start = result_extract_start + lanes;
        for lane in 0..lanes {
            let scalar = match ops[result_extract_start + lane].kind {
                OpKind::VExtractLane {
                    dst: scalar @ VReg::Virtual(_),
                    vec,
                    lane: actual_lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                } if vec == raw && usize::from(actual_lane) == lane => scalar,
                _ => return None,
            };
            if ops[result_extract_start + lane].x86_hint.is_some()
                || !exact_insert(
                    &ops[destination_insert_start + lane],
                    destination,
                    scalar,
                    lane as u8,
                )
            {
                return None;
            }
            requirements.push((scalar, 1, 1));
        }
    }

    let mut unique = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| unique.insert(*register))
        .then_some(requirements)
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy MMX/SSE `PMULUDQ` or
    /// SSE4.1 `PMULDQ`. Only the mandatory 66H XMM form or the no-mandatory-
    /// prefix MMX form, each followed by an optional final REX prefix, is
    /// accepted. XMM REX.R/B extend the operands and REX.W/X are ignored;
    /// every REX bit is ignored for MMX operands. Memory, segment/address-size
    /// prefixes, repeated/lock prefixes, REX2, VEX/EVEX, truncation, and
    /// trailing bytes fail closed.
    pub(crate) fn legacy_register_widening_dword_multiply_replay(
        &self,
    ) -> Option<X86LegacyWideningDwordMultiplyReplay> {
        let (mmx, rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (false, Some(*rex), tail),
            [0x66, tail @ ..] => (false, None, tail),
            [rex @ 0x40..=0x4F, tail @ ..] => (true, Some(*rex), tail),
            tail => (true, None, tail),
        };
        let (signed, modrm) = match tail {
            [0x0F, 0xF4, modrm] => (false, *modrm),
            [0x0F, 0x38, 0x28, modrm] if !mmx => (true, *modrm),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }

        let extension = if mmx { 0 } else { rex.unwrap_or(0) };
        Some(X86LegacyWideningDwordMultiplyReplay {
            destination: ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
            source: (modrm & 7) | ((extension & 0x01) << 3),
            signed,
            mmx,
        })
    }
}
