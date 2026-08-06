//! Fail-closed helper-backed EVEX packed expand memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand,
    VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{SmirBlock, X86EvexExpandMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact VEXPAND*/VPEXPAND* dense-memory reconstruction consumed by x86-64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexExpandMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexExpandMemoryEncoding,
}

fn destination(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated packed expand width"),
    }))
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 | VecElementType::F32 => Some(MemWidth::B4),
        VecElementType::I64 | VecElementType::F64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn insert_fresh(owned: &mut HashSet<VReg>, register: VReg) -> bool {
    matches!(register, VReg::Virtual(_)) && owned.insert(register)
}

fn exact_local_virtual_counts(
    ops: &[crate::smir::ir::ops::SmirOp],
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let mut local_definitions = HashMap::new();
    let mut local_uses = HashMap::new();
    for op in ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *local_definitions.entry(register).or_insert(0usize) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *local_uses.entry(register).or_insert(0usize) += 1;
            }
        }
    }
    local_definitions.iter().all(|(register, count)| {
        virtual_definitions.get(register) == Some(count)
            && virtual_uses.get(register).copied().unwrap_or(0)
                == local_uses.get(register).copied().unwrap_or(0)
    })
}

#[allow(clippy::too_many_arguments)]
fn exact_mask_condition(
    block: &SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    mask: Option<VReg>,
    lane: u8,
    owned: &mut HashSet<VReg>,
) -> Option<VReg> {
    if let Some(mask) = mask {
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
        if direct_lane_zero {
            let condition = match first.kind {
                OpKind::And { dst, .. } if insert_fresh(owned, dst) => dst,
                _ => return None,
            };
            if first.guest_pc != guest_pc {
                return None;
            }
            *offset += 1;
            return Some(condition);
        }

        let shifted = match first.kind {
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Imm(amount),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if first.x86_hint.is_none()
                && first.guest_pc == guest_pc
                && src == mask
                && amount == i64::from(lane)
                && insert_fresh(owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        *offset += 1;
        let and = block.ops.get(index + *offset)?;
        let condition = match and.kind {
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if and.x86_hint.is_none()
                && and.guest_pc == guest_pc
                && src1 == shifted
                && insert_fresh(owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        *offset += 1;
        Some(condition)
    } else {
        let op = block.ops.get(index + *offset)?;
        let condition = match op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            } if op.x86_hint.is_none() && op.guest_pc == guest_pc && insert_fresh(owned, dst) => {
                dst
            }
            _ => return None,
        };
        *offset += 1;
        Some(condition)
    }
}

#[allow(clippy::too_many_arguments)]
fn exact_count_update(
    block: &SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    count: VReg,
    condition: VReg,
    owned: &mut HashSet<VReg>,
) -> Option<VReg> {
    let add = block.ops.get(index + *offset)?;
    let incremented = match add.kind {
        OpKind::Add {
            dst,
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if add.x86_hint.is_none()
            && add.guest_pc == guest_pc
            && src1 == count
            && insert_fresh(owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    *offset += 1;
    let select = block.ops.get(index + *offset)?;
    let selected = match select.kind {
        OpKind::Select {
            dst,
            cond,
            src_true,
            src_false,
            width: OpWidth::W64,
        } if select.x86_hint.is_none()
            && select.guest_pc == guest_pc
            && cond == condition
            && src_true == incremented
            && src_false == count
            && insert_fresh(owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    *offset += 1;
    Some(selected)
}

/// Validate the complete optimizer-stable scalar reconstruction emitted for
/// one Type-E4 expand memory source.
///
/// The matcher accepts the O0 terminal count update and the O1/O2 form where
/// that dead pair is removed; O2 may also remove the lane-zero shift. Every
/// other operation, virtual value, address, width, lane order, and final
/// architectural commit is exact. Matching is O(L) time and O(V) auxiliary
/// space for at most 64 lanes and the V virtual registers in this one graph.
pub(crate) fn x86_jit_evex_expand_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexExpandMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_expand_memory_encoding()?;
    let expected_destination = destination(encoding.destination, encoding.width);
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let expected_memory_width = memory_width(encoding.elem)?;
    let mut owned = HashSet::new();
    let mut offset = 0usize;

    let (base, address) = match &first.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if first.x86_hint.is_none()
            && x86_jit_mem_address_shape_valid(addr)
            && insert_fresh(&mut owned, *base) =>
        {
            (*base, addr)
        }
        _ => return None,
    };
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }
    offset += 1;

    let count_op = block.ops.get(index + offset)?;
    let mut count = match count_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if count_op.x86_hint.is_none()
            && count_op.guest_pc == guest_pc
            && insert_fresh(&mut owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    offset += 1;

    let raw = if encoding.zeroing {
        let zero_op = block.ops.get(index + offset)?;
        let zero = match zero_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } if zero_op.x86_hint.is_none()
                && zero_op.guest_pc == guest_pc
                && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;
        let broadcast = block.ops.get(index + offset)?;
        let raw = match broadcast.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes: actual_lanes,
            } if broadcast.x86_hint.is_none()
                && broadcast.guest_pc == guest_pc
                && scalar == zero
                && elem == encoding.elem
                && actual_lanes == lanes
                && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;
        raw
    } else {
        let move_op = block.ops.get(index + offset)?;
        let raw = match move_op.kind {
            OpKind::VMov { dst, src, width }
                if move_op.x86_hint.is_none()
                    && move_op.guest_pc == guest_pc
                    && src == expected_destination
                    && width == encoding.width
                    && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;
        raw
    };

    for lane in 0..lanes {
        let condition = exact_mask_condition(
            block,
            index,
            &mut offset,
            guest_pc,
            expected_mask,
            lane,
            &mut owned,
        )?;
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && extract.guest_pc == guest_pc
                && vec == raw
                && actual_lane == lane
                && elem == encoding.elem
                && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;
        let load = block.ops.get(index + offset)?;
        if !matches!(
            &load.kind,
            OpKind::PredLoad {
                dst,
                cond,
                addr: Address::BaseIndexScale {
                    base: Some(actual_base),
                    index: actual_index,
                    scale,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                width,
                signed: SignExtend::Zero,
            } if *dst == scalar
                && *cond == condition
                && *actual_base == base
                && *actual_index == count
                && *scale == encoding.elem.bytes() as u8
                && *width == expected_memory_width
        ) || load.x86_hint.is_some()
            || load.guest_pc != guest_pc
        {
            return None;
        }
        offset += 1;
        let insert = block.ops.get(index + offset)?;
        if !matches!(
            insert.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem,
            } if dst == raw
                && vec == raw
                && actual_scalar == scalar
                && actual_lane == lane
                && elem == encoding.elem
        ) || insert.x86_hint.is_some()
            || insert.guest_pc != guest_pc
        {
            return None;
        }
        offset += 1;

        if lane + 1 < lanes {
            count = exact_count_update(
                block,
                index,
                &mut offset,
                guest_pc,
                count,
                condition,
                &mut owned,
            )?;
        } else {
            let saved_offset = offset;
            let mut saved_owned = owned.clone();
            if exact_count_update(
                block,
                index,
                &mut offset,
                guest_pc,
                count,
                condition,
                &mut saved_owned,
            )
            .is_some()
            {
                owned = saved_owned;
            } else {
                offset = saved_offset;
            }
        }
    }

    let commit = block.ops.get(index + offset)?;
    let expected_hint = X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: match encoding.elem {
            VecElementType::I8 | VecElementType::I16 => 0x62,
            VecElementType::F32 | VecElementType::F64 => 0x88,
            VecElementType::I32 | VecElementType::I64 => 0x89,
            _ => return None,
        },
        width: encoding.width,
        w: matches!(
            encoding.elem,
            VecElementType::I16 | VecElementType::I64 | VecElementType::F64
        ),
    };
    if !matches!(
        commit.kind,
        OpKind::VMov { dst, src, width }
            if dst == expected_destination && src == raw && width == encoding.width
    ) || commit.x86_hint != Some(expected_hint)
        || commit.guest_pc != guest_pc
    {
        return None;
    }
    offset += 1;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    let sequence = block.ops.get(index..index + offset)?;
    if !exact_local_virtual_counts(sequence, virtual_definitions, virtual_uses) {
        return None;
    }

    Some(X86JitEvexExpandMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}
