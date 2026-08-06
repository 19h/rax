//! Fail-closed helper-backed EVEX extraction-to-memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, X86EvexChunkExtractMemoryEncoding, X86EvexScalarExtractMemoryEncoding,
    X86InstructionBytes,
};

use super::evex_expand_memory_source::{
    exact_local_virtual_counts, exact_mask_condition, insert_fresh,
};
use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact two-op Type-E9NF scalar extraction to memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarExtractMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexScalarExtractMemoryEncoding,
}

/// Exact Type-E6NF vector-chunk extraction to memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexChunkExtractMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexChunkExtractMemoryEncoding,
}

/// Either architecturally defined EVEX extraction-to-memory graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86JitEvexExtractMemorySequence {
    Scalar(X86JitEvexScalarExtractMemorySequence),
    Chunk(X86JitEvexChunkExtractMemorySequence),
}

impl X86JitEvexExtractMemorySequence {
    pub(crate) const fn consumed(self) -> usize {
        match self {
            Self::Scalar(sequence) => sequence.consumed,
            Self::Chunk(sequence) => sequence.consumed,
        }
    }

    pub(crate) const fn needs_avx512vl(self) -> bool {
        match self {
            Self::Scalar(_) => false,
            Self::Chunk(sequence) => sequence.encoding.needs_avx512vl,
        }
    }

    pub(crate) const fn needs_avx512dq(self) -> bool {
        match self {
            Self::Scalar(sequence) => sequence.encoding.needs_avx512dq,
            Self::Chunk(sequence) => sequence.encoding.needs_avx512dq,
        }
    }

    pub(crate) const fn writemask(self) -> Option<u8> {
        match self {
            Self::Scalar(_) => None,
            Self::Chunk(sequence) => sequence.encoding.writemask,
        }
    }

    pub(crate) const fn mask_lanes(self) -> u32 {
        match self {
            Self::Scalar(_) => 0,
            Self::Chunk(sequence) => sequence.encoding.chunk_width.lanes(sequence.encoding.elem),
        }
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX extraction vector width"),
    }))
}

/// Validate the optimizer-stable two-op graph for EVEX `VEXTRACTPS` or
/// `VPEXTRB/W/D/Q` with a memory destination.
///
/// Complete byte provenance binds source register, W/opcode feature class,
/// immediate-selected lane, Tuple1 Scalar width, and address/APX frontier.
/// Classification is O(V) time and space for the supplied virtual-value maps.
pub(crate) fn x86_jit_evex_scalar_extract_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarExtractMemorySequence> {
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
        .evex_scalar_extract_memory_encoding()?;
    let sequence = block.ops.get(index..index + 2)?;
    let extracted = match sequence[0].kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem,
            sign: SignExtend::Zero,
        } if sequence[0].x86_hint.is_none()
            && vec == vector(encoding.source, VecWidth::V128)
            && lane == encoding.lane
            && elem == encoding.elem
            && matches!(dst, VReg::Virtual(_)) =>
        {
            dst
        }
        _ => return None,
    };
    let address = match &sequence[1].kind {
        OpKind::Store { src, addr, width }
            if sequence[1].x86_hint.is_none()
                && *src == extracted
                && *width == encoding.memory_width
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            addr
        }
        _ => return None,
    };
    if sequence.iter().any(|op| op.guest_pc != guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
        || !no_following_same_pc(block, index, 2, guest_pc)
        || !exact_local_virtual_counts(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitEvexScalarExtractMemorySequence {
        consumed: 2,
        address_offset: 1,
        encoding,
    })
}

/// Validate the complete O0/O1/O2 graph for one Type-E6NF EVEX vector-chunk
/// extraction to memory.
///
/// Unmasked forms build the selected 128- or 256-bit chunk and store it once.
/// Masked forms additionally perform the architecturally required complete
/// destination load, merge every 32- or 64-bit element, and perform a complete
/// store even for an empty mask. O2 may remove only the lane-zero mask shift.
/// Matching is O(L + V) time and O(V) space for L <= 8 lanes and V local
/// virtual registers.
pub(crate) fn x86_jit_evex_chunk_extract_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexChunkExtractMemorySequence> {
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
        .evex_chunk_extract_memory_encoding()?;
    let lanes = u8::try_from(encoding.chunk_width.lanes(encoding.elem)).ok()?;
    let expected_source = vector(encoding.source, encoding.source_width);
    let mut owned = HashSet::new();
    let mut offset = 0usize;

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

    for lane in 0..lanes {
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
                && vec == expected_source
                && actual_lane == encoding.first_lane + lane
                && elem == encoding.elem
                && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
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
                } if dst == raw
                    && vec == raw
                    && actual_scalar == scalar
                    && actual_lane == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    let address_offset = offset;
    if encoding.writemask.is_none() {
        let store = block.ops.get(index + offset)?;
        let address = match &store.kind {
            OpKind::VStore { src, addr, width }
                if store.x86_hint.is_none()
                    && store.guest_pc == guest_pc
                    && *src == raw
                    && *width == encoding.chunk_width
                    && x86_jit_mem_address_shape_valid(addr) =>
            {
                addr
            }
            _ => return None,
        };
        offset += 1;
        if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
            return None;
        }
    } else {
        let load = block.ops.get(index + offset)?;
        let (merged, address) = match &load.kind {
            OpKind::VLoad { dst, addr, width }
                if load.x86_hint.is_none()
                    && load.guest_pc == guest_pc
                    && *width == encoding.chunk_width
                    && x86_jit_mem_address_shape_valid(addr)
                    && insert_fresh(&mut owned, *dst) =>
            {
                (*dst, addr)
            }
            _ => return None,
        };
        if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
            return None;
        }
        offset += 1;

        let snapshot = block.ops.get(index + offset)?;
        let old = match snapshot.kind {
            OpKind::VMov { dst, src, width }
                if snapshot.x86_hint.is_none()
                    && snapshot.guest_pc == guest_pc
                    && src == merged
                    && width == encoding.chunk_width
                    && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;

        let zero_op = block.ops.get(index + offset)?;
        let mask_zero = match zero_op.kind {
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

        let result_seed = block.ops.get(index + offset)?;
        let result_base = match result_seed.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes: actual_lanes,
            } if result_seed.x86_hint.is_none()
                && result_seed.guest_pc == guest_pc
                && scalar == mask_zero
                && elem == encoding.elem
                && actual_lanes == lanes
                && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;

        let mask = VReg::Arch(ArchReg::X86(X86Reg::K(
            encoding.writemask.expect("masked extraction"),
        )));
        let select_width = match encoding.elem {
            crate::smir::ir::types::VecElementType::F32
            | crate::smir::ir::types::VecElementType::I32 => OpWidth::W32,
            crate::smir::ir::types::VecElementType::F64
            | crate::smir::ir::types::VecElementType::I64 => OpWidth::W64,
            _ => unreachable!("validated EVEX extraction element"),
        };
        for lane in 0..lanes {
            let condition = exact_mask_condition(
                block,
                index,
                &mut offset,
                guest_pc,
                Some(mask),
                lane,
                &mut owned,
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
                    && active_op.guest_pc == guest_pc
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
            let inactive_op = block.ops.get(index + offset)?;
            let inactive = match inactive_op.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: actual_lane,
                    elem,
                    sign: SignExtend::Zero,
                } if inactive_op.x86_hint.is_none()
                    && inactive_op.guest_pc == guest_pc
                    && vec == old
                    && actual_lane == lane
                    && elem == encoding.elem
                    && insert_fresh(&mut owned, dst) =>
                {
                    dst
                }
                _ => return None,
            };
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
                    && select.guest_pc == guest_pc
                    && cond == condition
                    && src_true == active
                    && src_false == inactive
                    && width == select_width
                    && insert_fresh(&mut owned, dst) =>
                {
                    dst
                }
                _ => return None,
            };
            offset += 1;
            let insert = block.ops.get(index + offset)?;
            if insert.x86_hint.is_some()
                || insert.guest_pc != guest_pc
                || !matches!(
                    insert.kind,
                    OpKind::VInsertLane {
                        dst,
                        vec,
                        scalar,
                        lane: actual_lane,
                        elem,
                    } if dst == merged
                        && vec == if lane == 0 { result_base } else { merged }
                        && scalar == selected
                        && actual_lane == lane
                        && elem == encoding.elem
                )
            {
                return None;
            }
            offset += 1;
        }

        let store = block.ops.get(index + offset)?;
        if store.x86_hint.is_some()
            || store.guest_pc != guest_pc
            || !matches!(
                &store.kind,
                OpKind::VStore { src, addr, width }
                    if *src == merged
                        && addr == address
                        && *width == encoding.chunk_width
            )
        {
            return None;
        }
        offset += 1;
    }

    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    let sequence = block.ops.get(index..index + offset)?;
    if !exact_local_virtual_counts(sequence, virtual_definitions, virtual_uses) {
        return None;
    }

    Some(X86JitEvexChunkExtractMemorySequence {
        consumed: offset,
        address_offset,
        encoding,
    })
}

/// Classify either exact EVEX extraction-to-memory graph.
pub(crate) fn x86_jit_evex_extract_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexExtractMemorySequence> {
    x86_jit_evex_scalar_extract_memory_sequence(
        block,
        index,
        allow_mem,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )
    .map(X86JitEvexExtractMemorySequence::Scalar)
    .or_else(|| {
        x86_jit_evex_chunk_extract_memory_sequence(
            block,
            index,
            allow_mem,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        )
        .map(X86JitEvexExtractMemorySequence::Chunk)
    })
}
