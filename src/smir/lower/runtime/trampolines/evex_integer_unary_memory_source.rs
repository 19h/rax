//! Fail-closed helper-backed EVEX unary packed-integer memory admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexIntegerUnaryMemoryEncoding, X86EvexIntegerUnaryMemoryKind,
    X86EvexIntegerUnaryMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence_tail,
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, exact_lane_address,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 unary
/// packed-integer memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexIntegerUnaryMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexIntegerUnaryMemoryEncoding,
}

fn exact_integer_unary(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexIntegerUnaryMemoryEncoding,
) -> bool {
    if op.x86_hint.is_some() {
        return false;
    }
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    match op.kind {
        OpKind::VConflict {
            dst,
            src,
            mask,
            elem,
            width,
            zeroing,
        } => {
            encoding.kind == X86EvexIntegerUnaryMemoryKind::Conflict
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && zeroing == encoding.zeroing
        }
        OpKind::VLeadingZeros {
            dst,
            src,
            mask,
            elem,
            width,
            zeroing,
        } => {
            encoding.kind == X86EvexIntegerUnaryMemoryKind::LeadingZeros
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && zeroing == encoding.zeroing
        }
        OpKind::VPopcnt {
            dst,
            src,
            mask,
            elem,
            width,
            zeroing,
        } => {
            encoding.kind == X86EvexIntegerUnaryMemoryKind::Popcnt
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && zeroing == encoding.zeroing
        }
        _ => false,
    }
}

fn element_memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 => Some(MemWidth::B4),
        VecElementType::I64 => Some(MemWidth::B8),
        _ => None,
    }
}

/// Match the exact prefix-closure predicate graph used by masked
/// VPCONFLICTD/Q memory sources. Source lane `n` is required iff at least one
/// destination mask bit in `[n, L)` is set because every active destination
/// compares against all lower lanes.
#[allow(clippy::too_many_arguments)]
fn exact_masked_conflict(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerUnaryMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexIntegerUnaryMemorySequence> {
    if encoding.kind != X86EvexIntegerUnaryMemoryKind::Conflict
        || !matches!(
            encoding.replay,
            X86EvexIntegerUnaryMemoryReplay::MaskedVector { .. }
        )
    {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let valid_mask = (1u64 << lanes) - 1;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }

    let zero = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !single_definition_single_use(zero, virtual_definitions, virtual_uses) {
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
            && elem == encoding.elem
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
            usize::from(lanes) + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let bounded_op = block.ops.get(index + 2)?;
    let bounded_mask = match bounded_op.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(actual_mask),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if bounded_op.x86_hint.is_none() && src1 == mask && actual_mask == valid_mask as i64 => {
            dst
        }
        _ => return None,
    };
    if bounded_op.guest_pc != guest_pc || !matches!(bounded_mask, VReg::Virtual(_)) {
        return None;
    }

    let address_offset = 3usize;
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

    let expected_width = element_memory_width(encoding.elem)?;
    let mut offset = address_offset + 1;
    let mut direct_lane_zero = false;
    for lane in 0..lanes {
        let initial = block.ops.get(index + offset)?;
        let required = if lane == 0
            && !matches!(
                initial.kind,
                OpKind::Shr {
                    src,
                    amount: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                    ..
                } if initial.x86_hint.is_none() && src == bounded_mask
            ) {
            direct_lane_zero = true;
            bounded_mask
        } else {
            let shifted = match initial.kind {
                OpKind::Shr {
                    dst,
                    src,
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if initial.x86_hint.is_none()
                    && src == bounded_mask
                    && amount == i64::from(lane) =>
                {
                    dst
                }
                _ => return None,
            };
            if initial.guest_pc != guest_pc
                || !exact_virtual_definition_use(shifted, 1, 2, virtual_definitions, virtual_uses)
            {
                return None;
            }
            offset += 1;
            shifted
        };

        let mut folded = required;
        for shift in [32i64, 16, 8, 4, 2, 1] {
            let shr = block.ops.get(index + offset)?;
            let upper = match shr.kind {
                OpKind::Shr {
                    dst,
                    src,
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if shr.x86_hint.is_none() && src == folded && amount == shift => dst,
                _ => return None,
            };
            if shr.guest_pc != guest_pc
                || !single_definition_single_use(upper, virtual_definitions, virtual_uses)
            {
                return None;
            }
            offset += 1;

            let or = block.ops.get(index + offset)?;
            let combined = match or.kind {
                OpKind::Or {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if or.x86_hint.is_none() && src1 == folded && src2 == upper => dst,
                _ => return None,
            };
            let expected_uses = if shift == 1 { 1 } else { 2 };
            if or.guest_pc != guest_pc
                || !exact_virtual_definition_use(
                    combined,
                    1,
                    expected_uses,
                    virtual_definitions,
                    virtual_uses,
                )
            {
                return None;
            }
            folded = combined;
            offset += 1;
        }

        let and = block.ops.get(index + offset)?;
        let required_bit = match and.kind {
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if and.x86_hint.is_none() && src1 == folded => dst,
            _ => return None,
        };
        if and.guest_pc != guest_pc
            || !single_definition_single_use(required_bit, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

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
        let source_offset = if encoding.broadcast {
            0
        } else {
            i64::from(lane) * i64::from(encoding.elem.bytes())
        };
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
                && *cond == required_bit
                && *width == expected_width
                && exact_lane_address(addr, base, source_offset)
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
                    && elem == encoding.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    let bounded_uses = usize::from(lanes) + usize::from(direct_lane_zero);
    if !exact_virtual_definition_use(
        bounded_mask,
        1,
        bounded_uses,
        virtual_definitions,
        virtual_uses,
    ) {
        return None;
    }
    let operation = block.ops.get(index + offset)?;
    if operation.guest_pc != guest_pc || !exact_integer_unary(operation, loaded, encoding) {
        return None;
    }
    offset += 1;
    if !no_following_same_pc(block, index, offset, guest_pc)
        || !exact_evex_memory_apx_frontier(
            block,
            index,
            guest_pc,
            match &lea.kind {
                OpKind::Lea { addr, .. } => addr,
                _ => unreachable!("validated conflict LEA"),
            },
        )
    {
        return None;
    }

    Some(X86JitEvexIntegerUnaryMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: if encoding.broadcast {
            encoding.elem.bytes()
        } else {
            encoding.width.bytes()
        },
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX unary
/// packed-integer memory source.
///
/// Exact provenance binds the operation, element/vector widths, architectural
/// destination and writemask, tuple form, helper address, every fault-
/// suppressing lane predicate (including VPCONFLICT prefix closure), APX
/// guard, and sole same-PC frontier. Matching is O(L) time and O(1) auxiliary
/// space for at most 64 lanes; callers build definition/use maps once in O(N)
/// time and O(V) space.
pub(crate) fn x86_jit_evex_integer_unary_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexIntegerUnaryMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_integer_unary_memory_encoding()?;
    if encoding.kind == X86EvexIntegerUnaryMemoryKind::Conflict && encoding.writemask.is_some() {
        return exact_masked_conflict(block, index, encoding, virtual_definitions, virtual_uses);
    }

    let form = match encoding.replay {
        X86EvexIntegerUnaryMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexIntegerUnaryMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexIntegerUnaryMemoryReplay::MaskedVector { .. } if encoding.broadcast => {
            X86EvexE4MemoryReplayForm::Broadcast
        }
        X86EvexIntegerUnaryMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: encoding.zeroing,
        vector_load_hint: None,
        form,
        memory_source_uses: 1,
    };
    let exact = exact_evex_e4_memory_sequence_tail(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |block, tail_index, memory_source| {
            exact_integer_unary(block.ops.get(tail_index)?, memory_source, encoding).then_some(1)
        },
    )?;
    // The shared broadcast matcher also recognizes the aggregate one-load
    // shape used by other lifter families. This replay deliberately preserves
    // the unary lifters' independently predicated per-lane accesses, and its
    // lowerer therefore requires their common LEA frontier.
    if matches!(
        encoding.replay,
        X86EvexIntegerUnaryMemoryReplay::MaskedVector { .. }
    ) && !matches!(
        block.ops.get(index + exact.address_offset)?.kind,
        OpKind::Lea { .. }
    ) {
        return None;
    }
    Some(X86JitEvexIntegerUnaryMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
