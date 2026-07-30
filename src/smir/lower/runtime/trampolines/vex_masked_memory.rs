//! Fail-closed helper-backed VEX masked-memory sequence admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{SmirBlock, X86InstructionBytes, X86VexMaskedMemoryEncoding};

/// Exact contiguous `VMASKMOVPS/PD` or `VPMASKMOVD/Q` decomposition consumed
/// by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexMaskedMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexMaskedMemoryEncoding,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("VEX masked memory has only 128- and 256-bit forms"),
    }))
}

fn exact_lane_address(addr: &Address, base: VReg, offset: i64) -> bool {
    matches!(
        addr,
        Address::BaseOffset {
            base: actual_base,
            offset: actual_offset,
            disp_size: DispSize::Auto,
        } if *actual_base == base && *actual_offset == offset
    )
}

fn local_once(
    reg: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(reg, VReg::Virtual(_))
        && virtual_definitions.get(&reg) == Some(&1)
        && virtual_uses.get(&reg) == Some(&1)
}

/// Validate the complete canonical masked-memory expansion emitted by the
/// strict x86-64 lifter.
///
/// The classifier binds source bytes to load/store direction, element type,
/// vector width, architectural mask, and destination/data register. Every
/// temporary, lane index, lane address, predicate, and final commit must then
/// match the same complete instruction graph. Runtime is O(L), where
/// 2 <= L <= 8 lanes, and auxiliary space is O(L). Definition/use maps are
/// constructed by callers in O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_masked_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexMaskedMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if first.x86_hint.is_some() {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .vex_masked_memory_encoding()?;
    let lanes = encoding.width.lanes(encoding.elem) as usize;
    let base = match &first.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if addr.is_x86_state_backed_shape() => *base,
        _ => return None,
    };
    if virtual_definitions.get(&base) != Some(&1) || virtual_uses.get(&base) != Some(&lanes) {
        return None;
    }
    let guest_pc = first.guest_pc;
    let expected_mask = vector_reg(encoding.mask, encoding.width);
    let mut predicates = Vec::with_capacity(lanes);
    let mut cursor = index + 1;
    for lane in 0..lanes {
        let extract = block.ops.get(cursor)?;
        let shift = block.ops.get(cursor + 1)?;
        if [extract, shift]
            .iter()
            .any(|op| op.guest_pc != guest_pc || op.x86_hint.is_some())
        {
            return None;
        }
        let mask_lane = match &extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if *vec == expected_mask
                && usize::from(*actual_lane) == lane
                && *elem == encoding.elem =>
            {
                *dst
            }
            _ => return None,
        };
        if !local_once(mask_lane, virtual_definitions, virtual_uses) {
            return None;
        }
        let predicate = match &shift.kind {
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Imm(amount),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if *src == mask_lane && *amount == i64::from(encoding.elem.bytes() * 8 - 1) => *dst,
            _ => return None,
        };
        if !local_once(predicate, virtual_definitions, virtual_uses) {
            return None;
        }
        predicates.push(predicate);
        cursor += 2;
    }

    let expected_vector = vector_reg(encoding.vector, encoding.width);
    if encoding.load {
        let zero = block.ops.get(cursor)?;
        let broadcast = block.ops.get(cursor + 1)?;
        if [zero, broadcast]
            .iter()
            .any(|op| op.guest_pc != guest_pc || op.x86_hint.is_some())
        {
            return None;
        }
        let zero_scalar = match &zero.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => *dst,
            _ => return None,
        };
        if !local_once(zero_scalar, virtual_definitions, virtual_uses) {
            return None;
        }
        let loaded = match &broadcast.kind {
            OpKind::VBroadcast {
                dst: loaded @ VReg::Virtual(_),
                scalar,
                elem,
                lanes: actual_lanes,
            } if *scalar == zero_scalar
                && *elem == encoding.elem
                && usize::from(*actual_lanes) == lanes =>
            {
                *loaded
            }
            _ => return None,
        };
        if virtual_definitions.get(&loaded) != Some(&(lanes + 1))
            || virtual_uses.get(&loaded) != Some(&(lanes + 1))
        {
            return None;
        }
        cursor += 2;

        for (lane, predicate) in predicates.iter().copied().enumerate() {
            let zero_lane = block.ops.get(cursor)?;
            let load = block.ops.get(cursor + 1)?;
            let insert = block.ops.get(cursor + 2)?;
            if [zero_lane, load, insert]
                .iter()
                .any(|op| op.guest_pc != guest_pc || op.x86_hint.is_some())
            {
                return None;
            }
            let scalar = match &zero_lane.kind {
                OpKind::Mov {
                    dst: scalar @ VReg::Virtual(_),
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                } => *scalar,
                _ => return None,
            };
            if virtual_definitions.get(&scalar) != Some(&2) || virtual_uses.get(&scalar) != Some(&1)
            {
                return None;
            }
            if !matches!(
                &load.kind,
                OpKind::PredLoad {
                    dst,
                    cond,
                    addr,
                    width,
                    signed: SignExtend::Zero,
                } if *dst == scalar
                    && *cond == predicate
                    && *width == encoding.memory_width
                    && exact_lane_address(
                        addr,
                        base,
                        lane as i64 * i64::from(encoding.elem.bytes()),
                    )
            ) || !matches!(
                &insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if *dst == loaded
                    && *vec == loaded
                    && *actual_scalar == scalar
                    && usize::from(*actual_lane) == lane
                    && *elem == encoding.elem
            ) {
                return None;
            }
            cursor += 3;
        }

        let commit = block.ops.get(cursor)?;
        if commit.guest_pc != guest_pc
            || commit.x86_hint.is_some()
            || !matches!(
                &commit.kind,
                OpKind::VMov { dst, src, width }
                    if *dst == expected_vector && *src == loaded && *width == encoding.width
            )
        {
            return None;
        }
        cursor += 1;
    } else {
        let mut values = Vec::with_capacity(lanes);
        for lane in 0..lanes {
            let extract = block.ops.get(cursor)?;
            if extract.guest_pc != guest_pc || extract.x86_hint.is_some() {
                return None;
            }
            let scalar = match &extract.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: actual_lane,
                    elem,
                    sign: SignExtend::Zero,
                } if *vec == expected_vector
                    && usize::from(*actual_lane) == lane
                    && *elem == encoding.elem =>
                {
                    *dst
                }
                _ => return None,
            };
            if !local_once(scalar, virtual_definitions, virtual_uses) {
                return None;
            }
            values.push(scalar);
            cursor += 1;
        }
        for (lane, (predicate, scalar)) in predicates
            .iter()
            .copied()
            .zip(values.iter().copied())
            .enumerate()
        {
            let store = block.ops.get(cursor)?;
            if store.guest_pc != guest_pc
                || store.x86_hint.is_some()
                || !matches!(
                    &store.kind,
                    OpKind::PredStore {
                        src: SrcOperand::Reg(actual_scalar),
                        cond,
                        addr,
                        width,
                    } if *actual_scalar == scalar
                        && *cond == predicate
                        && *width == encoding.memory_width
                        && exact_lane_address(
                            addr,
                            base,
                            lane as i64 * i64::from(encoding.elem.bytes()),
                        )
                )
            {
                return None;
            }
            cursor += 1;
        }
    }

    if block
        .ops
        .iter()
        .enumerate()
        .any(|(position, op)| op.guest_pc == guest_pc && !(index..cursor).contains(&position))
    {
        return None;
    }
    Some(X86JitVexMaskedMemorySequence {
        consumed: cursor - index,
        encoding,
    })
}
