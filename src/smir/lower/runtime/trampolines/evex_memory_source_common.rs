//! Shared structural predicates for exact helper-backed EVEX sequences.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, GuestAddr, OpWidth, SrcOperand, VReg, VecWidth, X86Reg,
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
