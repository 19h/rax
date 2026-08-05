//! Shared structural predicates for exact helper-backed EVEX sequences.

mod reconstructed_broadcast;

use std::collections::HashMap;

use reconstructed_broadcast::{exact_masked_e4_broadcast, exact_masked_e4_reconstructed_broadcast};

use super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, X86Reg,
};

pub(super) fn vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=31))), VecWidth::V256)
        | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(index @ 0..=31))), VecWidth::V512) => Some(*index),
        _ => None,
    }
}

pub(super) fn single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    exact_virtual_definition_use(register, 1, 1, virtual_definitions, virtual_uses)
}

pub(super) fn exact_virtual_definition_use(
    register: VReg,
    definitions: usize,
    uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&definitions)
        && virtual_uses.get(&register).copied().unwrap_or(0) == uses
}

/// Match the exact selector-vector graph emitted for one two-source packed
/// floating-point shuffle with an imm8 control byte. Advances `offset` past
/// the zero seed, every lane selector, and the final `VShuffle`, and returns
/// its raw virtual result.
///
/// Runtime is O(L) and auxiliary space is O(1), where L is the architectural
/// lane count (at most 16). Global definition/use maps are supplied by the
/// caller and bind every internal virtual value to this graph.
#[allow(clippy::too_many_arguments)]
pub(super) fn exact_two_source_fp_shuffle_imm_graph(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source1: VReg,
    source2: VReg,
    width: VecWidth,
    elem: VecElementType,
    immediate: u8,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let lanes = width.lanes(elem) as u8;
    let block_lanes = match elem {
        VecElementType::F32 => 4,
        VecElementType::F64 => 2,
        _ => return None,
    };

    let zero_op = block.ops.get(index + *offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if zero_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(zero, 1, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let indices_op = block.ops.get(index + *offset)?;
    let indices = match indices_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: actual_elem,
            lanes: actual_lanes,
        } if indices_op.x86_hint.is_none()
            && scalar == zero
            && actual_elem == elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if indices_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            indices,
            usize::from(lanes) + 1,
            usize::from(lanes) + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    *offset += 1;

    for lane in 0..lanes {
        let within = lane % block_lanes;
        let block_lane = lane - within;
        let (from_second, control) = if elem == VecElementType::F32 {
            (within >= 2, (immediate >> (within * 2)) & 3)
        } else {
            (within == 1, (immediate >> lane) & 1)
        };
        let selector = block_lane + control + if from_second { lanes } else { 0 };

        let selector_op = block.ops.get(index + *offset)?;
        let selector_reg = match selector_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(actual_selector),
                width: OpWidth::W64,
            } if selector_op.x86_hint.is_none() && actual_selector == i64::from(selector) => dst,
            _ => return None,
        };
        if selector_op.guest_pc != guest_pc
            || !single_definition_single_use(selector_reg, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let insert_op = block.ops.get(index + *offset)?;
        if insert_op.guest_pc != guest_pc
            || insert_op.x86_hint.is_some()
            || !matches!(
                insert_op.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: actual_lane,
                    elem: actual_elem,
                } if dst == indices
                    && vec == indices
                    && scalar == selector_reg
                    && actual_lane == lane
                    && actual_elem == elem
            )
        {
            return None;
        }
        *offset += 1;
    }

    let shuffle_op = block.ops.get(index + *offset)?;
    let raw = match shuffle_op.kind {
        OpKind::VShuffle {
            dst,
            src1: actual_source1,
            src2: Some(actual_source2),
            indices: actual_indices,
            elem: actual_elem,
            lanes: actual_lanes,
        } if shuffle_op.x86_hint.is_none()
            && actual_source1 == source1
            && actual_source2 == source2
            && actual_indices == indices
            && actual_elem == elem
            && actual_lanes == lanes
            && matches!(dst, VReg::Virtual(_)) =>
        {
            dst
        }
        _ => return None,
    };
    if shuffle_op.guest_pc != guest_pc {
        return None;
    }
    *offset += 1;
    Some(raw)
}

pub(super) fn exact_lane_address(address: &Address, base: VReg, offset: i64) -> bool {
    matches!(
        address,
        Address::BaseOffset {
            base: actual_base,
            offset: actual_offset,
            disp_size: DispSize::Auto,
        } if *actual_base == base && *actual_offset == offset
    )
}

/// Match the flag-neutral `(mask >> lane) & 1` predicate emitted at O0/O1,
/// plus the O2 lane-zero form where the redundant shift is removed. Advances
/// `offset` past the complete one- or two-op graph.
pub(super) fn exact_lane_predicate(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    mask: VReg,
    lane: u8,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let first = block.ops.get(index + *offset)?;
    let direct_lane_zero = lane == 0
        && matches!(
            first.kind,
            OpKind::And {
                src1,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
                ..
            } if first.x86_hint.is_none() && src1 == mask
        );
    let condition = if direct_lane_zero {
        match first.kind {
            OpKind::And { dst, .. } => dst,
            _ => unreachable!("direct lane-zero predicate matched And"),
        }
    } else {
        let shifted = match first.kind {
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Imm(amount),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if first.x86_hint.is_none() && src == mask && amount == i64::from(lane) => dst,
            _ => return None,
        };
        if first.guest_pc != guest_pc
            || !single_definition_single_use(shifted, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;
        let and = block.ops.get(index + *offset)?;
        match and.kind {
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if and.x86_hint.is_none() && src1 == shifted => dst,
            _ => return None,
        }
    };
    let condition_op = block.ops.get(index + *offset)?;
    if condition_op.guest_pc != guest_pc
        || !single_definition_single_use(condition, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;
    Some(condition)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn exact_evex_vector_mask_result_with_raw_counts(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    mask: VReg,
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    zeroing: bool,
    raw_definitions: usize,
    raw_uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    let lanes = width.lanes(elem) as u8;
    if !exact_virtual_definition_use(
        raw,
        raw_definitions,
        raw_uses,
        virtual_definitions,
        virtual_uses,
    ) {
        return None;
    }

    let old = if zeroing {
        None
    } else {
        let old_op = block.ops.get(index + *offset)?;
        let old = match old_op.kind {
            OpKind::VMov {
                dst,
                src,
                width: old_width,
            } if old_op.x86_hint.is_none()
                && vector_index(&src, width) == Some(destination)
                && old_width == width =>
            {
                dst
            }
            _ => return None,
        };
        if old_op.guest_pc != guest_pc
            || !exact_virtual_definition_use(
                old,
                1,
                usize::from(lanes),
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        *offset += 1;
        Some(old)
    };

    let zero_op = block.ops.get(index + *offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    let zero_uses = if zeroing { usize::from(lanes) + 1 } else { 1 };
    if zero_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(zero, 1, zero_uses, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let result_base_op = block.ops.get(index + *offset)?;
    let result_base = match result_base_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: broadcast_lanes,
        } if result_base_op.x86_hint.is_none()
            && scalar == zero
            && broadcast_elem == elem
            && broadcast_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if result_base_op.guest_pc != guest_pc
        || !single_definition_single_use(result_base, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let lane_width = match elem {
        VecElementType::I8 => OpWidth::W8,
        VecElementType::I16 | VecElementType::F16 => OpWidth::W16,
        VecElementType::I32 | VecElementType::F32 => OpWidth::W32,
        VecElementType::I64 | VecElementType::F64 => OpWidth::W64,
        _ => return None,
    };
    for lane in 0..lanes {
        let lane_condition = exact_lane_predicate(
            block,
            index,
            offset,
            guest_pc,
            mask,
            lane,
            virtual_definitions,
            virtual_uses,
        )?;

        let active_op = block.ops.get(index + *offset)?;
        let active = match active_op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: active_lane,
                elem: active_elem,
                sign: SignExtend::Zero,
            } if active_op.x86_hint.is_none()
                && vec == raw
                && active_lane == lane
                && active_elem == elem =>
            {
                dst
            }
            _ => return None,
        };
        if active_op.guest_pc != guest_pc
            || !single_definition_single_use(active, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let inactive = if let Some(old) = old {
            let inactive_op = block.ops.get(index + *offset)?;
            let inactive = match inactive_op.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: inactive_lane,
                    elem: inactive_elem,
                    sign: SignExtend::Zero,
                } if inactive_op.x86_hint.is_none()
                    && vec == old
                    && inactive_lane == lane
                    && inactive_elem == elem =>
                {
                    dst
                }
                _ => return None,
            };
            if inactive_op.guest_pc != guest_pc
                || !single_definition_single_use(inactive, virtual_definitions, virtual_uses)
            {
                return None;
            }
            *offset += 1;
            inactive
        } else {
            zero
        };

        let select_op = block.ops.get(index + *offset)?;
        let selected = match select_op.kind {
            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width: select_width,
            } if select_op.x86_hint.is_none()
                && cond == lane_condition
                && src_true == active
                && src_false == inactive
                && select_width == lane_width =>
            {
                dst
            }
            _ => return None,
        };
        if select_op.guest_pc != guest_pc
            || !single_definition_single_use(selected, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let insert_op = block.ops.get(index + *offset)?;
        if insert_op.x86_hint.is_some()
            || insert_op.guest_pc != guest_pc
            || !matches!(
                insert_op.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: insert_lane,
                    elem: insert_elem,
                } if vector_index(&dst, width) == Some(destination)
                    && vec == if lane == 0 { result_base } else { dst }
                    && scalar == selected
                    && insert_lane == lane
                    && insert_elem == elem
            )
        {
            return None;
        }
        *offset += 1;
    }
    Some(())
}

/// Match the complete merge/zero lane reconstruction emitted after one
/// writemasked EVEX vector operation. `raw` must have exactly one definition
/// and one use by each destination lane.
///
/// Advances `offset` past O0/O1/O2-equivalent graphs. Runtime is O(L) and
/// auxiliary space is O(1) for L destination lanes.
#[allow(clippy::too_many_arguments)]
pub(super) fn exact_evex_vector_mask_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    mask: VReg,
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    zeroing: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    exact_evex_vector_mask_result_with_raw_counts(
        block,
        index,
        offset,
        guest_pc,
        raw,
        mask,
        width,
        elem,
        destination,
        zeroing,
        1,
        width.lanes(elem) as usize,
        virtual_definitions,
        virtual_uses,
    )
}

/// Match the merge/zero reconstruction after a `raw` vector that was built by
/// one zero broadcast and one in-place insert per destination lane.
///
/// Such a vector has L + 1 definitions. It is used once by each insert and
/// once per mask lane, for 2L total uses.
#[allow(clippy::too_many_arguments)]
pub(super) fn exact_evex_reconstructed_vector_mask_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    mask: VReg,
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    zeroing: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    let lanes = width.lanes(elem) as usize;
    exact_evex_vector_mask_result_with_raw_counts(
        block,
        index,
        offset,
        guest_pc,
        raw,
        mask,
        width,
        elem,
        destination,
        zeroing,
        lanes + 1,
        2 * lanes,
        virtual_definitions,
        virtual_uses,
    )
}

/// Match the flag-neutral `((x | -x) >> 63)` normalization used to convert
/// `mask & applicable_bits` into the exact bit-0 predicate required by
/// PredLoad. Advances `offset` past the four-op graph.
pub(super) fn exact_nonzero_mask_predicate(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    mask: VReg,
    applicable_bits: u64,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let and = block.ops.get(index + *offset)?;
    let active_mask = match and.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(actual_bits),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if and.x86_hint.is_none() && src1 == mask && actual_bits == applicable_bits as i64 => dst,
        _ => return None,
    };
    if and.guest_pc != guest_pc
        || !exact_virtual_definition_use(active_mask, 1, 2, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let neg = block.ops.get(index + *offset)?;
    let negated = match neg.kind {
        OpKind::Neg {
            dst,
            src,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if neg.x86_hint.is_none() && src == active_mask => dst,
        _ => return None,
    };
    if neg.guest_pc != guest_pc
        || !single_definition_single_use(negated, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let or = block.ops.get(index + *offset)?;
    let combined = match or.kind {
        OpKind::Or {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if or.x86_hint.is_none() && src1 == active_mask && src2 == negated => dst,
        _ => return None,
    };
    if or.guest_pc != guest_pc
        || !single_definition_single_use(combined, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let shr = block.ops.get(index + *offset)?;
    let predicate = match shr.kind {
        OpKind::Shr {
            dst,
            src,
            amount: SrcOperand::Imm(63),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if shr.x86_hint.is_none() && src == combined => dst,
        _ => return None,
    };
    if shr.guest_pc != guest_pc
        || !single_definition_single_use(predicate, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;
    Some(predicate)
}

/// Memory materialization selected by an EVEX E2/E3/E4-compatible native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum X86EvexE4MemoryReplayForm {
    Vector,
    Broadcast,
    MaskedVector,
    /// Scalar E3-class load+op graph; structurally identical to the scalar
    /// staging used by helper-backed replay families.
    Scalar,
}

/// Architectural fields shared by exact EVEX E2/E3/E4-compatible source sequences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct X86EvexE4MemoryShape {
    pub(super) width: VecWidth,
    pub(super) elem: VecElementType,
    pub(super) writemask: Option<u8>,
    pub(super) zeroing: bool,
    pub(super) vector_load_hint: Option<X86OpHint>,
    pub(super) form: X86EvexE4MemoryReplayForm,
    /// Exact number of source-operand occurrences contributed by the
    /// operation-specific semantic tail. This is normally one; a constant
    /// packed-compare predicate deliberately compares the loaded value with
    /// itself and therefore contributes two uses.
    pub(super) memory_source_uses: usize,
}

/// Structural extent of one exact EVEX E2/E3/E4-compatible source decomposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct X86EvexE4MemoryMatch {
    pub(super) consumed: usize,
    pub(super) address_offset: usize,
    pub(super) memory_size: u32,
}

fn evex_e4_memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 | VecElementType::F16 => Some(MemWidth::B2),
        VecElementType::I32 | VecElementType::F32 => Some(MemWidth::B4),
        VecElementType::I64 | VecElementType::F64 => Some(MemWidth::B8),
        _ => None,
    }
}

pub(super) fn no_following_same_pc(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    consumed: usize,
    guest_pc: GuestAddr,
) -> bool {
    !block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == guest_pc)
}

pub(super) fn exact_evex_memory_sequence_frontier(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
) -> bool {
    let Some(previous_index) = index.checked_sub(1) else {
        return true;
    };
    let Some(previous) = block.ops.get(previous_index) else {
        return false;
    };
    if previous.guest_pc != guest_pc {
        return true;
    }
    if previous.x86_hint.is_some() || !matches!(previous.kind, OpKind::X86RequireApx) {
        return false;
    }
    previous_index == 0
        || block
            .ops
            .get(previous_index - 1)
            .is_some_and(|op| op.guest_pc != guest_pc)
}

fn apx_extended_gpr(register: VReg) -> bool {
    matches!(
        register,
        VReg::Arch(ArchReg::X86(register))
            if register.gpr_index().is_some_and(|index| index >= 16)
    )
}

fn evex_address_requires_apx(address: &Address) -> bool {
    match address {
        Address::X86Addr32(inner) => evex_address_requires_apx(inner),
        Address::Direct(register) => apx_extended_gpr(*register),
        Address::BaseOffset { base, .. } => apx_extended_gpr(*base),
        Address::BaseIndexScale { base, index, .. } => {
            base.is_some_and(apx_extended_gpr) || apx_extended_gpr(*index)
        }
        Address::SegmentRel {
            segment,
            base,
            index,
            ..
        } => {
            apx_extended_gpr(*segment)
                || base.is_some_and(apx_extended_gpr)
                || index.is_some_and(apx_extended_gpr)
        }
        Address::PcRel { .. } | Address::GpRel { .. } | Address::Absolute(_) => false,
    }
}

pub(super) fn exact_evex_memory_apx_frontier(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    address: &Address,
) -> bool {
    let has_guard = index.checked_sub(1).is_some_and(|previous_index| {
        block.ops.get(previous_index).is_some_and(|previous| {
            previous.guest_pc == guest_pc
                && previous.x86_hint.is_none()
                && matches!(previous.kind, OpKind::X86RequireApx)
        })
    });
    evex_address_requires_apx(address) == has_guard
}

pub(super) fn exact_evex_memory_sequence_address(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    address_offset: usize,
) -> Option<&Address> {
    match &block.ops.get(index + address_offset)?.kind {
        OpKind::VLoad { addr, .. }
        | OpKind::Load { addr, .. }
        | OpKind::PredLoad { addr, .. }
        | OpKind::Lea { addr, .. } => Some(addr),
        _ => None,
    }
}

fn exact_e4_semantic_tail<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    memory_source: VReg,
    exact_tail: &F,
) -> Option<usize>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    let consumed = exact_tail(block, index, memory_source)?;
    let end = index.checked_add(consumed)?;
    if consumed == 0
        || end > block.ops.len()
        || block.ops[index..end]
            .iter()
            .any(|op| op.guest_pc != guest_pc)
    {
        return None;
    }
    Some(consumed)
}

fn exact_unmasked_e4_vector<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_tail: &F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    if shape.form != X86EvexE4MemoryReplayForm::Vector || shape.writemask.is_some() || shape.zeroing
    {
        return None;
    }
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == shape.vector_load_hint
                && *width == shape.width
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(
        loaded,
        1,
        shape.memory_source_uses,
        virtual_definitions,
        virtual_uses,
    ) {
        return None;
    }
    let tail_consumed =
        exact_e4_semantic_tail(block, index + 1, load.guest_pc, loaded, exact_tail)?;
    let consumed = 1usize.checked_add(tail_consumed)?;
    if !no_following_same_pc(block, index, consumed, load.guest_pc) {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed,
        address_offset: 0,
        memory_size: shape.width.bytes(),
    })
}

fn exact_unmasked_e4_broadcast<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_tail: &F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    if shape.form != X86EvexE4MemoryReplayForm::Broadcast
        || shape.writemask.is_some()
        || shape.zeroing
    {
        return None;
    }
    let expected_width = evex_e4_memory_width(shape.elem)?;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let seed = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => Some(dst),
        _ => None,
    };
    let load_offset = usize::from(seed.is_some());
    let load = block.ops.get(index + load_offset)?;
    let scalar = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr)
            && seed.is_none_or(|seed| seed == *dst) =>
        {
            *dst
        }
        _ => return None,
    };
    let definitions = if seed.is_some() { 2 } else { 1 };
    if load.guest_pc != guest_pc
        || !exact_virtual_definition_use(scalar, definitions, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }
    let broadcast = block.ops.get(index + load_offset + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem,
            lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && elem == shape.elem
            && lanes == shape.width.lanes(shape.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != load.guest_pc
        || !exact_virtual_definition_use(
            loaded,
            1,
            shape.memory_source_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    let source_consumed = load_offset.checked_add(2)?;
    let tail_consumed = exact_e4_semantic_tail(
        block,
        index + source_consumed,
        load.guest_pc,
        loaded,
        exact_tail,
    )?;
    let consumed = source_consumed.checked_add(tail_consumed)?;
    if !no_following_same_pc(block, index, consumed, load.guest_pc) {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed,
        address_offset: load_offset,
        memory_size: expected_width.bytes(),
    })
}

fn exact_masked_e4_vector<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_tail: &F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    if shape.form != X86EvexE4MemoryReplayForm::MaskedVector {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(shape.writemask?)));
    let lanes = shape.width.lanes(shape.elem) as u8;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let zero = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !exact_virtual_definition_use(zero, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && scalar == zero
            && elem == shape.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            loaded,
            usize::from(lanes) + 1,
            usize::from(lanes) + shape.memory_source_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let address_offset = 2usize;
    let lea = block.ops.get(index + address_offset)?;
    let base = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *base,
        _ => return None,
    };
    if lea.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            base,
            1,
            usize::from(lanes),
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let expected_width = evex_e4_memory_width(shape.elem)?;
    let mut offset = address_offset + 1;
    for lane in 0..lanes {
        let condition = exact_lane_predicate(
            block,
            index,
            &mut offset,
            guest_pc,
            mask,
            lane,
            virtual_definitions,
            virtual_uses,
        )?;
        let seed = block.ops.get(index + offset)?;
        let scalar = match seed.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } if seed.x86_hint.is_none() => dst,
            _ => return None,
        };
        if seed.guest_pc != guest_pc
            || !exact_virtual_definition_use(scalar, 2, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let load = block.ops.get(index + offset)?;
        if !matches!(
            &load.kind,
            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed: SignExtend::Zero,
            } if load.x86_hint.is_none()
                && *dst == scalar
                && *cond == condition
                && *width == expected_width
                && exact_lane_address(
                    addr,
                    base,
                    i64::from(lane) * i64::from(shape.elem.bytes()),
                )
        ) || load.guest_pc != guest_pc
        {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.guest_pc != guest_pc
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if insert.x86_hint.is_none()
                    && dst == loaded
                    && vec == loaded
                    && actual_scalar == scalar
                    && actual_lane == lane
                    && elem == shape.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    offset += exact_e4_semantic_tail(block, index + offset, guest_pc, loaded, exact_tail)?;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed: offset,
        address_offset,
        memory_size: shape.width.bytes(),
    })
}

fn exact_scalar_e3_memory<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_tail: &F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    if shape.form != X86EvexE4MemoryReplayForm::Scalar || shape.width != VecWidth::V128 {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let scalar = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !exact_virtual_definition_use(scalar, 2, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let mut offset = 1usize;
    let condition = if let Some(mask_index) = shape.writemask {
        Some(exact_lane_predicate(
            block,
            index,
            &mut offset,
            guest_pc,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask_index))),
            0,
            virtual_definitions,
            virtual_uses,
        )?)
    } else {
        None
    };

    let address_offset = offset;
    let expected_width = evex_e4_memory_width(shape.elem)?;
    let load = block.ops.get(index + offset)?;
    let exact_load = match (&load.kind, condition) {
        (
            OpKind::Load {
                dst,
                addr,
                width,
                sign: SignExtend::Zero,
            },
            None,
        ) => {
            *dst == scalar
                && *width == expected_width
                && load.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr)
        }
        (
            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed: SignExtend::Zero,
            },
            Some(expected_condition),
        ) => {
            *dst == scalar
                && *cond == expected_condition
                && *width == expected_width
                && load.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr)
        }
        _ => false,
    };
    if !exact_load || load.guest_pc != guest_pc {
        return None;
    }
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem,
            lanes: 1,
        } if broadcast.x86_hint.is_none() && actual_scalar == scalar && elem == shape.elem => dst,
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            loaded,
            1,
            shape.memory_source_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    offset += 1;

    offset += exact_e4_semantic_tail(block, index + offset, guest_pc, loaded, exact_tail)?;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed: offset,
        address_offset,
        memory_size: expected_width.bytes(),
    })
}

/// Match an exact O0/O1/O2 EVEX E2/E3/E4-compatible memory-source
/// decomposition whose operation-specific semantic tail may contain multiple
/// operations.
///
/// The callback binds the reconstructed memory value to the exact tail at
/// `tail_index` and returns that tail's nonzero operation count. This common
/// matcher independently verifies same-PC contiguity and the terminal guest
/// frontier. Classification is O(L + T) time and O(1) auxiliary space for L
/// vector lanes and T semantic-tail operations; definition/use maps are built
/// once by the caller in O(N) time and O(V) space.
pub(super) fn exact_evex_e4_memory_sequence_tail<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_tail: F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    let guest_pc = block.ops.get(index)?.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let exact = match (shape.form, shape.writemask) {
        (X86EvexE4MemoryReplayForm::Vector, None) => exact_unmasked_e4_vector(
            block,
            index,
            shape,
            virtual_definitions,
            virtual_uses,
            &exact_tail,
        ),
        (X86EvexE4MemoryReplayForm::Broadcast, None) => exact_unmasked_e4_broadcast(
            block,
            index,
            shape,
            virtual_definitions,
            virtual_uses,
            &exact_tail,
        ),
        (X86EvexE4MemoryReplayForm::Broadcast, Some(_)) => exact_masked_e4_broadcast(
            block,
            index,
            shape,
            virtual_definitions,
            virtual_uses,
            &exact_tail,
        )
        .or_else(|| {
            exact_masked_e4_reconstructed_broadcast(
                block,
                index,
                shape,
                virtual_definitions,
                virtual_uses,
                &exact_tail,
            )
        }),
        (X86EvexE4MemoryReplayForm::MaskedVector, Some(_)) => exact_masked_e4_vector(
            block,
            index,
            shape,
            virtual_definitions,
            virtual_uses,
            &exact_tail,
        ),
        (X86EvexE4MemoryReplayForm::Scalar, _) => exact_scalar_e3_memory(
            block,
            index,
            shape,
            virtual_definitions,
            virtual_uses,
            &exact_tail,
        ),
        _ => None,
    }?;
    let address = exact_evex_memory_sequence_address(block, index, exact.address_offset)?;
    exact_evex_memory_apx_frontier(block, index, guest_pc, address).then_some(exact)
}

/// Compatibility matcher for E2/E3/E4 decompositions with exactly one
/// operation-specific consumer.
pub(super) fn exact_evex_e4_memory_sequence<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_consumer: F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::ops::SmirOp, VReg) -> bool,
{
    if shape.memory_source_uses != 1 {
        return None;
    }
    exact_evex_e4_memory_sequence_tail(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |block, tail_index, memory_source| {
            exact_consumer(block.ops.get(tail_index)?, memory_source).then_some(1)
        },
    )
}
