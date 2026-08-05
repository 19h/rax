//! Shared exact tails for helper-backed EVEX scalar memory replays.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

use super::evex_memory_source_common::{
    exact_virtual_definition_use, single_definition_single_use,
};

pub(super) fn xmm_index(reg: &VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))) => Some(*index),
        _ => None,
    }
}

/// Match the exact `K[0]` condition used to predicate one EVEX scalar memory
/// access. The caller supplies the complete expected use count so any hidden
/// consumer or optimizer rewrite fails closed.
#[allow(clippy::too_many_arguments)]
pub(super) fn exact_evex_scalar_mask_condition(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    mask: u8,
    uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let op = block.ops.get(index)?;
    let condition = match op.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if op.x86_hint.is_none() && src1 == VReg::Arch(ArchReg::X86(X86Reg::K(mask))) => dst,
        _ => return None,
    };
    (op.guest_pc == guest_pc
        && exact_virtual_definition_use(condition, 1, uses, virtual_definitions, virtual_uses))
    .then_some(condition)
}

/// Match the exact scalar result reconstruction shared by EVEX scalar
/// arithmetic families: upper XMM lanes come from source 1, lane 0 comes from
/// the already-selected scalar result, and bits above 128 are zeroed by the
/// architectural XMM destination write.
///
/// Runtime is O(L) and auxiliary space is O(L), where L is the fixed XMM lane
/// count and therefore at most eight for binary16.
#[allow(clippy::too_many_arguments)]
pub(super) fn exact_evex_scalar_result_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    tail_offset: usize,
    scalar_result: VReg,
    upper_source: VReg,
    elem: VecElementType,
    destination: u8,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == guest_pc)
    };
    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    let mut upper_scalars = Vec::with_capacity(xmm_lanes - 1);
    for lane in 1..xmm_lanes {
        let offset = tail_offset + lane - 1;
        let extract = block.ops.get(index + offset)?;
        let upper_scalar = match &extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extract_lane,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && *vec == upper_source
                && usize::from(*extract_lane) == lane
                && *extract_elem == elem =>
            {
                *dst
            }
            _ => return None,
        };
        if !same_pc(offset)
            || !single_definition_single_use(upper_scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        upper_scalars.push(upper_scalar);
    }

    let zero_offset = tail_offset + xmm_lanes - 1;
    let zero_op = block.ops.get(index + zero_offset)?;
    let zero = match &zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => *dst,
        _ => return None,
    };
    if !same_pc(zero_offset)
        || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let clear_offset = zero_offset + 1;
    let clear = block.ops.get(index + clear_offset)?;
    if clear.x86_hint.is_some()
        || !matches!(
            &clear.kind,
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: broadcast_elem,
                lanes: 1,
            } if xmm_index(dst) == Some(destination)
                && *scalar == zero
                && *broadcast_elem == elem
        )
        || !same_pc(clear_offset)
    {
        return None;
    }

    let low_insert_offset = clear_offset + 1;
    let low_insert = block.ops.get(index + low_insert_offset)?;
    if low_insert.x86_hint.is_some()
        || !matches!(
            &low_insert.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: 0,
                elem: insert_elem,
            } if xmm_index(dst) == Some(destination)
                && dst == vec
                && *scalar == scalar_result
                && *insert_elem == elem
        )
        || !same_pc(low_insert_offset)
    {
        return None;
    }
    for (lane, upper_scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        let offset = low_insert_offset + lane;
        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || !matches!(
                &insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: insert_lane,
                    elem: insert_elem,
                } if xmm_index(dst) == Some(destination)
                    && dst == vec
                    && *scalar == upper_scalar
                    && usize::from(*insert_lane) == lane
                    && *insert_elem == elem
            )
            || !same_pc(offset)
        {
            return None;
        }
    }

    let consumed = low_insert_offset + xmm_lanes;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == guest_pc)
    {
        return None;
    }
    Some(consumed)
}
