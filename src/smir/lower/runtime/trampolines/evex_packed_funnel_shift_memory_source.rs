//! Fail-closed helper-backed EVEX packed funnel-shift memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedFunnelShiftMemoryEncoding, X86EvexPackedFunnelShiftMemoryReplay,
    X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_lane_address, exact_lane_predicate, exact_nonzero_mask_predicate,
    exact_virtual_definition_use, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// packed funnel-shift memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFunnelShiftMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFunnelShiftMemoryEncoding,
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 => Some(MemWidth::B4),
        VecElementType::I64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn exact_funnel(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexPackedFunnelShiftMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let exact = match op.kind {
        OpKind::X86PackedFunnelShift {
            dst,
            src,
            fill,
            count,
            mask,
            amount,
            width,
            elem,
            left,
            zeroing,
        } => {
            let operands_match = if let Some(immediate) = encoding.immediate {
                vector_index(&src, encoding.width) == Some(encoding.source)
                    && fill == memory_source
                    && count.is_none()
                    && amount == immediate
            } else {
                vector_index(&src, encoding.width) == Some(encoding.destination)
                    && vector_index(&fill, encoding.width) == Some(encoding.source)
                    && count == Some(memory_source)
                    && amount == 0
            };
            vector_index(&dst, encoding.width) == Some(encoding.destination)
                && operands_match
                && mask == expected_mask
                && width == encoding.width
                && elem == encoding.elem
                && left == encoding.left
                && zeroing == encoding.zeroing
        }
        _ => false,
    };
    exact && op.x86_hint.is_none()
}

fn no_following_same_pc(
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

fn unmasked_vector_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFunnelShiftMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFunnelShiftMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFunnelShiftMemoryReplay::Vector { .. }
    ) || encoding.writemask.is_some()
        || encoding.zeroing
    {
        return None;
    }
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && *width == encoding.width
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    let funnel = block.ops.get(index + 1)?;
    if !exact_virtual_definition_use(loaded, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let consumed = 2;
    if funnel.guest_pc != load.guest_pc
        || !exact_funnel(funnel, loaded, encoding)
        || !no_following_same_pc(block, index, consumed, load.guest_pc)
    {
        return None;
    }
    Some(X86JitEvexPackedFunnelShiftMemorySequence {
        consumed,
        address_offset: 0,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}

fn unmasked_broadcast_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFunnelShiftMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFunnelShiftMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFunnelShiftMemoryReplay::Broadcast { .. }
    ) || encoding.writemask.is_some()
        || encoding.zeroing
    {
        return None;
    }
    let expected_width = memory_width(encoding.elem)?;
    let load = block.ops.get(index)?;
    let scalar = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(scalar, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem,
            lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != load.guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    let funnel = block.ops.get(index + 2)?;
    let consumed = 3;
    if funnel.guest_pc != load.guest_pc
        || !exact_funnel(funnel, loaded, encoding)
        || !no_following_same_pc(block, index, consumed, load.guest_pc)
    {
        return None;
    }
    Some(X86JitEvexPackedFunnelShiftMemorySequence {
        consumed,
        address_offset: 0,
        memory_size: expected_width.bytes(),
        encoding,
    })
}

fn masked_broadcast_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFunnelShiftMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFunnelShiftMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFunnelShiftMemoryReplay::Broadcast { .. }
    ) {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let applicable_bits = (1u64 << lanes) - 1;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let mut offset = 0usize;
    let condition = exact_nonzero_mask_predicate(
        block,
        index,
        &mut offset,
        guest_pc,
        mask,
        applicable_bits,
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

    let address_offset = offset;
    let expected_width = memory_width(encoding.elem)?;
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
            && x86_jit_mem_address_shape_valid(addr)
    ) || load.guest_pc != guest_pc
    {
        return None;
    }
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let funnel = block.ops.get(index + offset)?;
    if funnel.guest_pc != guest_pc || !exact_funnel(funnel, loaded, encoding) {
        return None;
    }
    offset += 1;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexPackedFunnelShiftMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: expected_width.bytes(),
        encoding,
    })
}

fn masked_vector_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFunnelShiftMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFunnelShiftMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFunnelShiftMemoryReplay::MaskedVector { .. }
    ) {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
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

    let address_offset = 2usize;
    let lea = block.ops.get(index + address_offset)?;
    let (base, original_address) = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => (*base, addr),
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
        || !original_address.is_x86_state_backed_shape()
    {
        return None;
    }

    let expected_width = memory_width(encoding.elem)?;
    let lane_bytes = i64::from(encoding.elem.bytes());
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
                && exact_lane_address(addr, base, i64::from(lane) * lane_bytes)
        ) || load.guest_pc != guest_pc
        {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || insert.guest_pc != guest_pc
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if dst == loaded
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

    let funnel = block.ops.get(index + offset)?;
    if funnel.guest_pc != guest_pc || !exact_funnel(funnel, loaded, encoding) {
        return None;
    }
    offset += 1;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexPackedFunnelShiftMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed
/// AVX-512 VBMI2 word/doubleword/quadword funnel-shift memory source.
///
/// Exact provenance binds the immediate/variable encoding class, direction,
/// element and vector widths, architectural operands, writemask policy,
/// broadcast/full-vector tuple, helper address, and single architectural
/// destination commit. Classification is O(L) time and O(1) auxiliary space
/// for L <= 32 lanes; callers build definition/use maps once in O(N) time and
/// O(V) space.
pub(crate) fn x86_jit_evex_packed_funnel_shift_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFunnelShiftMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_funnel_shift_memory_encoding()?;
    match encoding.replay {
        X86EvexPackedFunnelShiftMemoryReplay::Vector { .. } => {
            unmasked_vector_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFunnelShiftMemoryReplay::Broadcast { .. } if encoding.writemask.is_some() => {
            masked_broadcast_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFunnelShiftMemoryReplay::Broadcast { .. } => {
            unmasked_broadcast_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFunnelShiftMemoryReplay::MaskedVector { .. } => {
            masked_vector_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
    }
}
