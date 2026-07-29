//! Fail-closed helper-backed VEX memory-broadcast admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand,
    VReg, VecElementType, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX memory-broadcast decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexBroadcastMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) elem: VecElementType,
    pub(crate) source_lanes: u8,
    pub(crate) width: VecWidth,
    pub(crate) opcode: u8,
    pub(crate) needs_avx2: bool,
}

fn virtual_counts_are(
    register: VReg,
    definitions: usize,
    uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&definitions)
        && virtual_uses.get(&register) == Some(&uses)
}

fn low_vex_vector_index(reg: VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(index),
        _ => None,
    }
}

fn memory_width(elem: VecElementType) -> MemWidth {
    match elem {
        VecElementType::I8 => MemWidth::B1,
        VecElementType::I16 => MemWidth::B2,
        VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
        VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
        _ => unreachable!("validated VEX memory-broadcast element"),
    }
}

/// Validate the complete per-lane SMIR decomposition of one AVX/AVX2 VEX
/// memory broadcast. Exact source-byte provenance binds the destination,
/// element type, source tuple width, vector length, opcode, and feature class.
/// Every virtual definition/use count, lane address, zero extension, and
/// guest-PC boundary is checked before the graph may be replaced by one
/// precise helper load and a non-faulting register sequence.
///
/// Classification is O(L) time and O(L) auxiliary space for L destination
/// lanes; VEX bounds L to at most 32. Callers build global definition/use maps
/// once in O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_broadcast_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBroadcastMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if first.x86_hint.is_some() || (index != 0 && block.ops[index - 1].guest_pc == first.guest_pc) {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, first.guest_pc))?;
    let encoded = instruction.vex_memory_broadcast_fields()?;
    let destination_lanes = encoded.width.lanes(encoded.elem) as usize;
    let source_lanes = usize::from(encoded.source_lanes);
    let same_frontier = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == first.guest_pc && op.x86_hint.is_none())
    };
    let mut virtuals = HashSet::new();

    let address_base = match &first.kind {
        OpKind::Lea { dst, addr } if x86_jit_mem_address_shape_valid(addr) => *dst,
        _ => return None,
    };
    if !virtuals.insert(address_base)
        || !virtual_counts_are(
            address_base,
            1,
            source_lanes,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let source_zero = match &block.ops.get(index + 1)?.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => *dst,
        _ => return None,
    };
    if !same_frontier(1)
        || !virtuals.insert(source_zero)
        || !virtual_counts_are(source_zero, 1, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let source = match &block.ops.get(index + 2)?.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if *scalar == source_zero
            && *elem == encoded.elem
            && usize::from(*lanes) == destination_lanes =>
        {
            *dst
        }
        _ => return None,
    };
    let source_uses = source_lanes
        + if source_lanes == 1 {
            1
        } else {
            destination_lanes
        };
    if !same_frontier(2)
        || !virtuals.insert(source)
        || !virtual_counts_are(
            source,
            source_lanes + 1,
            source_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let lane_width = memory_width(encoded.elem);
    for lane in 0..source_lanes {
        let offset = 3 + lane * 3;
        let scalar = match &block.ops.get(index + offset)?.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => *dst,
            _ => return None,
        };
        if !same_frontier(offset)
            || !virtuals.insert(scalar)
            || !virtual_counts_are(scalar, 2, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        if !matches!(
            &block.ops.get(index + offset + 1)?.kind,
            OpKind::Load {
                dst,
                addr: Address::BaseOffset {
                    base,
                    offset,
                    disp_size: DispSize::Auto,
                },
                width,
                sign: SignExtend::Zero,
            } if *dst == scalar
                && *base == address_base
                && *offset == (lane as i64) * i64::from(encoded.elem.bytes())
                && *width == lane_width
        ) || !same_frontier(offset + 1)
        {
            return None;
        }
        if !matches!(
            &block.ops.get(index + offset + 2)?.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted,
                lane: inserted_lane,
                elem,
            } if *dst == source
                && *vec == source
                && *inserted == scalar
                && usize::from(*inserted_lane) == lane
                && *elem == encoded.elem
        ) || !same_frontier(offset + 2)
        {
            return None;
        }
    }

    let result_start = 3 + source_lanes * 3;
    let raw = if source_lanes == 1 {
        let extracted = match &block.ops.get(index + result_start)?.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem,
                sign: SignExtend::Zero,
            } if *vec == source && *elem == encoded.elem => *dst,
            _ => return None,
        };
        if !same_frontier(result_start)
            || !virtuals.insert(extracted)
            || !virtual_counts_are(extracted, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let raw = match &block.ops.get(index + result_start + 1)?.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes,
            } if *scalar == extracted
                && *elem == encoded.elem
                && usize::from(*lanes) == destination_lanes =>
            {
                *dst
            }
            _ => return None,
        };
        if !same_frontier(result_start + 1)
            || !virtuals.insert(raw)
            || !virtual_counts_are(raw, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        raw
    } else {
        let result_zero = match &block.ops.get(index + result_start)?.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => *dst,
            _ => return None,
        };
        if !same_frontier(result_start)
            || !virtuals.insert(result_zero)
            || !virtual_counts_are(result_zero, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let zeroed = match &block.ops.get(index + result_start + 1)?.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes,
            } if *scalar == result_zero
                && *elem == encoded.elem
                && usize::from(*lanes) == destination_lanes =>
            {
                *dst
            }
            _ => return None,
        };
        if !same_frontier(result_start + 1)
            || !virtuals.insert(zeroed)
            || !virtual_counts_are(zeroed, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }

        let insert_start = result_start + 2;
        let mut raw = None;
        for lane in 0..destination_lanes {
            let offset = insert_start + lane * 2;
            let extracted = match &block.ops.get(index + offset)?.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: extracted_lane,
                    elem,
                    sign: SignExtend::Zero,
                } if *vec == source
                    && usize::from(*extracted_lane) == lane % source_lanes
                    && *elem == encoded.elem =>
                {
                    *dst
                }
                _ => return None,
            };
            if !same_frontier(offset)
                || !virtuals.insert(extracted)
                || !virtual_counts_are(extracted, 1, 1, virtual_definitions, virtual_uses)
            {
                return None;
            }
            let (destination, vector) = match &block.ops.get(index + offset + 1)?.kind {
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: inserted_lane,
                    elem,
                } if *scalar == extracted
                    && usize::from(*inserted_lane) == lane
                    && *elem == encoded.elem =>
                {
                    (*dst, *vec)
                }
                _ => return None,
            };
            if !same_frontier(offset + 1)
                || !matches!(destination, VReg::Virtual(_))
                || vector != raw.unwrap_or(zeroed)
                || raw.is_some_and(|prior| destination != prior)
            {
                return None;
            }
            raw = Some(destination);
        }
        let raw = raw?;
        if !virtuals.insert(raw)
            || !virtual_counts_are(
                raw,
                destination_lanes,
                destination_lanes,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        raw
    };

    let move_offset = if source_lanes == 1 {
        result_start + 2
    } else {
        result_start + 2 + destination_lanes * 2
    };
    let destination = match &block.ops.get(index + move_offset)?.kind {
        OpKind::VMov { dst, src, width } if *src == raw && *width == encoded.width => {
            low_vex_vector_index(*dst, encoded.width)?
        }
        _ => return None,
    };
    if !same_frontier(move_offset) || destination != encoded.destination {
        return None;
    }
    let consumed = move_offset + 1;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }

    Some(X86JitVexBroadcastMemorySequence {
        consumed,
        memory_size: encoded.memory_size,
        destination,
        elem: encoded.elem,
        source_lanes: encoded.source_lanes,
        width: encoded.width,
        opcode: encoded.opcode,
        needs_avx2: encoded.needs_avx2,
    })
}
