//! Fail-closed helper-backed EVEX VPUNPCK*DQ/QDQ broadcast-memory admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{X86EvexBroadcastInterleaveMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_nonzero_mask_predicate, exact_virtual_definition_use, single_definition_single_use,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// packed D/Q interleave scalar-broadcast memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexBroadcastInterleaveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_offset: usize,
    pub(crate) encoding: X86EvexBroadcastInterleaveMemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> Option<VReg> {
    Some(match width {
        VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))),
        VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(index))),
        VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(index))),
        _ => return None,
    })
}

fn exact_interleave(
    op: &SmirOp,
    dst: VReg,
    src1: VReg,
    src2: VReg,
    encoding: X86EvexBroadcastInterleaveMemoryEncoding,
) -> bool {
    matches!(
        op.kind,
        OpKind::VInterleave {
            dst: actual_dst,
            src1: actual_src1,
            src2: actual_src2,
            elem,
            lanes,
            block_lanes,
            high,
        } if actual_dst == dst
            && actual_src1 == src1
            && actual_src2 == src2
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8
            && block_lanes == (16 / encoding.elem.bytes()) as u8
            && high == encoding.high
    ) && op.x86_hint
        == Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: encoding.opcode,
            width: encoding.width,
            w: encoding.elem == VecElementType::I64,
        })
}

fn exact_lane_predicate(
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

fn x86_jit_masked_evex_broadcast_interleave_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexBroadcastInterleaveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBroadcastInterleaveMemorySequence> {
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == guest_pc)
    };
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let lane_mask = (1u64 << lanes) - 1;

    let mut offset = 0usize;
    let aggregate_condition = exact_nonzero_mask_predicate(
        block,
        index,
        &mut offset,
        guest_pc,
        mask,
        lane_mask,
        virtual_definitions,
        virtual_uses,
    )?;

    let scalar_zero = block.ops.get(index + offset)?;
    let loaded_scalar = match scalar_zero.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if scalar_zero.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !same_pc(offset) {
        return None;
    }
    offset += 1;

    let load = block.ops.get(index + offset)?;
    if !matches!(
        load.kind,
        OpKind::PredLoad {
            dst,
            cond,
            ref addr,
            width,
            signed: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && dst == loaded_scalar
            && cond == aggregate_condition
            && width == encoding.memory_width
            && x86_jit_mem_address_shape_valid(addr)
    ) || !same_pc(offset)
        || !exact_virtual_definition_use(loaded_scalar, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }
    let memory_offset = offset;
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && scalar == loaded_scalar
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if !same_pc(offset) || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let interleave = block.ops.get(index + offset)?;
    let raw = match interleave.kind {
        OpKind::VInterleave { dst, .. } => dst,
        _ => return None,
    };
    if !same_pc(offset)
        || !exact_interleave(
            interleave,
            raw,
            vector(encoding.source1, encoding.width)?,
            loaded,
            encoding,
        )
        || !exact_virtual_definition_use(
            raw,
            1,
            usize::from(lanes),
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    offset += 1;

    let destination = vector(encoding.destination, encoding.width)?;
    let old = if matches!(
        block.ops.get(index + offset).map(|op| &op.kind),
        Some(OpKind::VMov { .. })
    ) {
        let op = block.ops.get(index + offset)?;
        let old = match op.kind {
            OpKind::VMov { dst, src, width }
                if op.x86_hint.is_none() && src == destination && width == encoding.width =>
            {
                dst
            }
            _ => return None,
        };
        if encoding.zeroing
            || !same_pc(offset)
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
        offset += 1;
        Some(old)
    } else {
        None
    };
    if !encoding.zeroing && old.is_none() {
        return None;
    }

    let zero_op = block.ops.get(index + offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    let zero_uses = 1 + if encoding.zeroing {
        usize::from(lanes)
    } else {
        0
    };
    if !same_pc(offset)
        || !exact_virtual_definition_use(zero, 1, zero_uses, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let result_base_op = block.ops.get(index + offset)?;
    let result_base = match result_base_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if result_base_op.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if !same_pc(offset)
        || !single_definition_single_use(result_base, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let lane_width = match encoding.elem {
        VecElementType::I32 => OpWidth::W32,
        VecElementType::I64 => OpWidth::W64,
        _ => return None,
    };
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

        let active_op = block.ops.get(index + offset)?;
        let active = match active_op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if active_op.x86_hint.is_none()
                && vec == raw
                && actual_lane == lane
                && elem == encoding.elem =>
            {
                dst
            }
            _ => return None,
        };
        if !same_pc(offset)
            || !single_definition_single_use(active, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let inactive = if let Some(old) = old {
            let op = block.ops.get(index + offset)?;
            let inactive = match op.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: actual_lane,
                    elem,
                    sign: SignExtend::Zero,
                } if op.x86_hint.is_none()
                    && vec == old
                    && actual_lane == lane
                    && elem == encoding.elem =>
                {
                    dst
                }
                _ => return None,
            };
            if !same_pc(offset)
                || !single_definition_single_use(inactive, virtual_definitions, virtual_uses)
            {
                return None;
            }
            offset += 1;
            inactive
        } else {
            zero
        };

        let select = block.ops.get(index + offset)?;
        let selected = match select.kind {
            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width,
            } if select.x86_hint.is_none()
                && cond == condition
                && src_true == active
                && src_false == inactive
                && width == lane_width =>
            {
                dst
            }
            _ => return None,
        };
        if !same_pc(offset)
            || !single_definition_single_use(selected, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: actual_lane,
                    elem,
                } if dst == destination
                    && vec == if lane == 0 { result_base } else { destination }
                    && scalar == selected
                    && actual_lane == lane
                    && elem == encoding.elem
            )
            || !same_pc(offset)
        {
            return None;
        }
        offset += 1;
    }

    if block
        .ops
        .get(index + offset)
        .is_some_and(|op| op.guest_pc == guest_pc)
    {
        return None;
    }
    Some(X86JitEvexBroadcastInterleaveMemorySequence {
        consumed: offset,
        memory_offset,
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VPUNPCKLDQ/LQDQ/HDQ/HQDQ scalar-broadcast memory source. Exact provenance
/// binds width, element type, interleave half, operands, mask, and helper
/// memory width.
///
/// Classification is O(L) time and O(1) auxiliary space for L <= 16 lanes;
/// callers build global definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_broadcast_interleave_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBroadcastInterleaveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_broadcast_interleave_memory_encoding()?;
    if encoding.writemask.is_some() {
        return x86_jit_masked_evex_broadcast_interleave_memory_sequence(
            block,
            index,
            encoding,
            virtual_definitions,
            virtual_uses,
        );
    }
    if encoding.zeroing {
        return None;
    }

    let loaded_scalar = match first.kind {
        OpKind::Load {
            dst,
            ref addr,
            width,
            sign: SignExtend::Zero,
        } if first.x86_hint.is_none()
            && width == encoding.memory_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            dst
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if broadcast.x86_hint.is_none()
            && scalar == loaded_scalar
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != first.guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let interleave = block.ops.get(index + 2)?;
    if interleave.guest_pc != first.guest_pc
        || !exact_interleave(
            interleave,
            vector(encoding.destination, encoding.width)?,
            vector(encoding.source1, encoding.width)?,
            loaded,
            encoding,
        )
        || block
            .ops
            .get(index + 3)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }

    Some(X86JitEvexBroadcastInterleaveMemorySequence {
        consumed: 3,
        memory_offset: 0,
        encoding,
    })
}
