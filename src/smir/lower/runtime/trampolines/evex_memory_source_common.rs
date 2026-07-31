//! Shared structural predicates for exact helper-backed EVEX sequences.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
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

/// Match the complete merge/zero lane reconstruction emitted after one
/// writemasked EVEX vector operation. `raw` must be a nonarchitectural full
/// result; this routine validates its exact use by all active lanes and the
/// single architectural destination reconstruction.
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
    let lanes = width.lanes(elem) as u8;
    if !exact_virtual_definition_use(
        raw,
        1,
        usize::from(lanes),
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
        VecElementType::F16 => OpWidth::W16,
        VecElementType::F32 => OpWidth::W32,
        VecElementType::F64 => OpWidth::W64,
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
