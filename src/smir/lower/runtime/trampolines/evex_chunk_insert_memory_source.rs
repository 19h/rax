//! Fail-closed helper-backed EVEX vector-chunk insert memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, SignExtend, VReg, VecWidth, X86Reg};
use crate::smir::ir::{X86EvexChunkInsertMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_vector_mask_result_with_raw_counts,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// vector-chunk insert memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexChunkInsertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexChunkInsertMemoryEncoding,
}

fn encoded_vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX chunk-insert width"),
    }))
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VINSERTF32X4/VINSERTF64X2/VINSERTI32X4/VINSERTI64X2 or
/// VINSERTF32X8/VINSERTF64X4/VINSERTI32X8/VINSERTI64X4 memory source.
///
/// Exact provenance binds opcode, W, widths, operands, imm8, insertion lane,
/// destination mask policy, one unconditional E6NF tuple read, the APX
/// address guard, and the guest-PC frontier. Runtime is O(L) and auxiliary
/// space is O(1), where L <= 16 destination lanes; callers build shared
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_chunk_insert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexChunkInsertMemorySequence> {
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
        .evex_chunk_insert_memory_encoding()?;

    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && *width == encoding.chunk_width
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    let chunk_lanes = encoding.chunk_width.lanes(encoding.elem) as usize;
    if !exact_virtual_definition_use(loaded, 1, chunk_lanes, virtual_definitions, virtual_uses) {
        return None;
    }
    let address = exact_evex_memory_sequence_address(block, index, 0)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    let source1 = encoded_vector(encoding.source1, encoding.width);
    let mut offset = 1usize;
    let seed = block.ops.get(index + offset)?;
    let raw = match seed.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        } if seed.x86_hint.is_none()
            && src1 == source1
            && src2 == source1
            && width == encoding.width =>
        {
            dst
        }
        _ => return None,
    };
    if seed.guest_pc != guest_pc || !matches!(raw, VReg::Virtual(_)) {
        return None;
    }
    offset += 1;

    let chunks = encoding.width.bytes() / encoding.chunk_width.bytes();
    let first_lane = usize::from(encoding.immediate & (chunks as u8 - 1)) * chunk_lanes;
    for lane in 0..chunk_lanes {
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && vec == loaded
                && usize::from(actual_lane) == lane
                && elem == encoding.elem =>
            {
                dst
            }
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.guest_pc != guest_pc
            || insert.x86_hint.is_some()
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
                    && usize::from(actual_lane) == first_lane + lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    let destination_lanes = encoding.width.lanes(encoding.elem) as usize;
    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result_with_raw_counts(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            chunk_lanes + 1,
            chunk_lanes + destination_lanes,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if encoding.zeroing
            || !exact_virtual_definition_use(
                raw,
                chunk_lanes + 1,
                chunk_lanes + 1,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.guest_pc != guest_pc
            || commit.x86_hint.is_some()
            || !matches!(
                commit.kind,
                OpKind::VMov { dst, src, width }
                    if vector_index(&dst, encoding.width) == Some(encoding.destination)
                        && src == raw
                        && width == encoding.width
            )
        {
            return None;
        }
        offset += 1;
    }
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }

    Some(X86JitEvexChunkInsertMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
