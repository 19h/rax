//! Fail-closed helper-backed EVEX packed-logical broadcast-memory admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexBroadcastLogicMemoryEncoding, X86EvexLogicMemoryKind, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_nonzero_mask_predicate, exact_virtual_definition_use, single_definition_single_use,
    vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// EVEX packed-logical scalar-broadcast memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexBroadcastLogicMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_offset: usize,
    pub(crate) encoding: X86EvexBroadcastLogicMemoryEncoding,
}

fn exact_logic(
    op: &SmirOp,
    dst: VReg,
    src1: VReg,
    src2: VReg,
    encoding: X86EvexBroadcastLogicMemoryEncoding,
) -> bool {
    let operands_match = match (encoding.kind, &op.kind) {
        (
            X86EvexLogicMemoryKind::And,
            OpKind::VAnd {
                dst: actual_dst,
                src1: actual_src1,
                src2: actual_src2,
                width,
            },
        )
        | (
            X86EvexLogicMemoryKind::AndNot,
            OpKind::VAndNot {
                dst: actual_dst,
                src1: actual_src1,
                src2: actual_src2,
                width,
            },
        )
        | (
            X86EvexLogicMemoryKind::Or,
            OpKind::VOr {
                dst: actual_dst,
                src1: actual_src1,
                src2: actual_src2,
                width,
            },
        )
        | (
            X86EvexLogicMemoryKind::Xor,
            OpKind::VXor {
                dst: actual_dst,
                src1: actual_src1,
                src2: actual_src2,
                width,
            },
        ) => {
            *actual_dst == dst
                && *actual_src1 == src1
                && *actual_src2 == src2
                && *width == encoding.width
        }
        _ => false,
    };
    operands_match
        && op.x86_hint
            == Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: if encoding.elem == VecElementType::F32 {
                    X86SsePrefix::None
                } else {
                    X86SsePrefix::OpSize
                },
                opcode: match (encoding.kind, encoding.elem) {
                    (X86EvexLogicMemoryKind::And, VecElementType::F32 | VecElementType::F64) => {
                        0x54
                    }
                    (X86EvexLogicMemoryKind::AndNot, VecElementType::F32 | VecElementType::F64) => {
                        0x55
                    }
                    (X86EvexLogicMemoryKind::Or, VecElementType::F32 | VecElementType::F64) => 0x56,
                    (X86EvexLogicMemoryKind::Xor, VecElementType::F32 | VecElementType::F64) => {
                        0x57
                    }
                    (X86EvexLogicMemoryKind::And, VecElementType::I32 | VecElementType::I64) => {
                        0xDB
                    }
                    (X86EvexLogicMemoryKind::AndNot, VecElementType::I32 | VecElementType::I64) => {
                        0xDF
                    }
                    (X86EvexLogicMemoryKind::Or, VecElementType::I32 | VecElementType::I64) => 0xEB,
                    (X86EvexLogicMemoryKind::Xor, VecElementType::I32 | VecElementType::I64) => {
                        0xEF
                    }
                    _ => return false,
                },
                width: encoding.width,
                w: matches!(encoding.elem, VecElementType::F64 | VecElementType::I64),
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

fn x86_jit_masked_evex_broadcast_logic_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexBroadcastLogicMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBroadcastLogicMemorySequence> {
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == guest_pc)
    };
    let loaded_scalar = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let lane_mask = (1u64 << lanes) - 1;

    let mut offset = 1usize;
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
    let old = if matches!(
        block.ops.get(index + offset).map(|op| &op.kind),
        Some(OpKind::VMov { .. })
    ) {
        let op = block.ops.get(index + offset)?;
        let old = match op.kind {
            OpKind::VMov { dst, src, width }
                if op.x86_hint.is_none()
                    && vector_index(&src, encoding.width) == Some(encoding.destination)
                    && width == encoding.width =>
            {
                dst
            }
            _ => return None,
        };
        let uses = if encoding.zeroing {
            0
        } else {
            usize::from(lanes)
        };
        if !same_pc(offset)
            || !exact_virtual_definition_use(old, 1, uses, virtual_definitions, virtual_uses)
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

    let logic = block.ops.get(index + offset)?;
    let raw = match logic.kind {
        OpKind::VAnd { dst, .. }
        | OpKind::VAndNot { dst, .. }
        | OpKind::VOr { dst, .. }
        | OpKind::VXor { dst, .. } => dst,
        _ => return None,
    };
    if !same_pc(offset)
        || !exact_logic(
            logic,
            raw,
            match encoding.width {
                VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.source1))),
                VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(encoding.source1))),
                VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(encoding.source1))),
                _ => return None,
            },
            loaded,
            encoding,
        )
        || !exact_virtual_definition_use(
            raw,
            1,
            usize::from(lanes) + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    offset += 1;

    let zero = if matches!(
        block.ops.get(index + offset).map(|op| &op.kind),
        Some(OpKind::Mov {
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
            ..
        })
    ) {
        let op = block.ops.get(index + offset)?;
        let zero = match op.kind {
            OpKind::Mov { dst, .. } if op.x86_hint.is_none() => dst,
            _ => return None,
        };
        let uses = if encoding.zeroing {
            usize::from(lanes)
        } else {
            0
        };
        if !same_pc(offset)
            || !exact_virtual_definition_use(zero, 1, uses, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        Some(zero)
    } else {
        None
    };
    if encoding.zeroing && zero.is_none() {
        return None;
    }

    let lane_width = match encoding.elem {
        VecElementType::F32 | VecElementType::I32 => OpWidth::W32,
        VecElementType::F64 | VecElementType::I64 => OpWidth::W64,
        _ => return None,
    };
    for lane in 0..lanes {
        let inactive = if let Some(old) = old.filter(|_| !encoding.zeroing) {
            let op = block.ops.get(index + offset)?;
            let scalar = match op.kind {
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
                || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
            {
                return None;
            }
            offset += 1;
            scalar
        } else {
            zero?
        };

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
                } if vector_index(&dst, encoding.width) == Some(encoding.destination)
                    && vec == if lane == 0 { raw } else { dst }
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
    Some(X86JitEvexBroadcastLogicMemorySequence {
        consumed: offset,
        memory_offset,
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// packed-logical scalar-broadcast memory source. Exact provenance binds
/// operation, width, element type, architectural operands, masking, and helper
/// memory width.
///
/// Classification is O(L) time and O(1) auxiliary space for L <= 16 lanes;
/// callers build global definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_broadcast_logic_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBroadcastLogicMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_broadcast_logic_memory_encoding()?;
    if encoding.writemask.is_some() {
        return x86_jit_masked_evex_broadcast_logic_memory_sequence(
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
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    let load = block.ops.get(index + 1)?;
    if !matches!(
        load.kind,
        OpKind::Load {
            dst,
            ref addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && dst == loaded_scalar
            && width == encoding.memory_width
            && x86_jit_mem_address_shape_valid(addr)
    ) || load.guest_pc != first.guest_pc
        || !exact_virtual_definition_use(loaded_scalar, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let broadcast = block.ops.get(index + 2)?;
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

    let logic = block.ops.get(index + 3)?;
    let destination = match encoding.width {
        VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.destination))),
        VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(encoding.destination))),
        VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(encoding.destination))),
        _ => return None,
    };
    let source1 = match encoding.width {
        VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.source1))),
        VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(encoding.source1))),
        VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(encoding.source1))),
        _ => return None,
    };
    if logic.guest_pc != first.guest_pc
        || !exact_logic(logic, destination, source1, loaded, encoding)
        || block
            .ops
            .get(index + 4)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }

    Some(X86JitEvexBroadcastLogicMemorySequence {
        consumed: 4,
        memory_offset: 1,
        encoding,
    })
}
