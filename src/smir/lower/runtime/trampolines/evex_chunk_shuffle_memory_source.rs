//! Fail-closed helper-backed EVEX 128-bit-chunk shuffle memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexChunkShuffleMemoryEncoding, X86EvexChunkShuffleMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_reconstructed_vector_mask_result,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// 128-bit-chunk shuffle memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexChunkShuffleMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexChunkShuffleMemoryEncoding,
}

fn encoded_vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX chunk-shuffle width"),
    }))
}

fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexChunkShuffleMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(VReg, usize)> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let source_uses = encoding.width.lanes(encoding.elem) as usize / 2;
    match encoding.replay {
        X86EvexChunkShuffleMemoryReplay::Vector { .. } => {
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
            exact_virtual_definition_use(loaded, 1, source_uses, virtual_definitions, virtual_uses)
                .then_some((loaded, 1))
        }
        X86EvexChunkShuffleMemoryReplay::Broadcast { memory_width, .. } => {
            let load = block.ops.get(index)?;
            let scalar = match &load.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                } if load.x86_hint.is_none()
                    && *width == memory_width
                    && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            if !single_definition_single_use(scalar, virtual_definitions, virtual_uses) {
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
                    && u32::from(lanes) == encoding.width.lanes(encoding.elem) =>
                {
                    dst
                }
                _ => return None,
            };
            if broadcast.guest_pc != guest_pc
                || !exact_virtual_definition_use(
                    loaded,
                    1,
                    source_uses,
                    virtual_definitions,
                    virtual_uses,
                )
            {
                return None;
            }
            Some((loaded, 2))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn exact_chunk_graph(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source1: VReg,
    source2: VReg,
    encoding: X86EvexChunkShuffleMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let chunks = (encoding.width.bytes() / 16) as u8;
    let chunk_lanes = (16 / encoding.elem.bytes()) as u8;

    let zero_op = block.ops.get(index + *offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if zero_op.guest_pc != guest_pc
        || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let seed_op = block.ops.get(index + *offset)?;
    let raw = match seed_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if seed_op.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if seed_op.guest_pc != guest_pc || !matches!(raw, VReg::Virtual(_)) {
        return None;
    }
    *offset += 1;

    for destination_lane in 0..lanes {
        let destination_chunk = destination_lane / chunk_lanes;
        let chunk_lane = destination_lane % chunk_lanes;
        let (source, selector) = if chunks == 2 {
            if destination_chunk == 0 {
                (source1, encoding.immediate & 1)
            } else {
                (source2, (encoding.immediate >> 1) & 1)
            }
        } else if destination_chunk < 2 {
            (source1, (encoding.immediate >> (destination_chunk * 2)) & 3)
        } else {
            (source2, (encoding.immediate >> (destination_chunk * 2)) & 3)
        };
        let extract = block.ops.get(index + *offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && vec == source
                && lane == selector * chunk_lanes + chunk_lane
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
        *offset += 1;

        let insert = block.ops.get(index + *offset)?;
        if insert.guest_pc != guest_pc
            || insert.x86_hint.is_some()
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane,
                    elem,
                } if dst == raw
                    && vec == raw
                    && actual_scalar == scalar
                    && lane == destination_lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        *offset += 1;
    }
    Some(raw)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VSHUFF32X4/VSHUFF64X2 or VSHUFI32X4/VSHUFI64X2 memory source.
///
/// Exact provenance binds opcode, W, width, operands, imm8, every 128-bit
/// selector, destination mask policy, one unconditional E4NF tuple read, the
/// APX address guard, and the guest-PC frontier. Runtime is O(L) and auxiliary
/// space is O(1), where L <= 16 lanes; callers build definition/use maps once
/// in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_chunk_shuffle_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexChunkShuffleMemorySequence> {
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
        .evex_chunk_shuffle_memory_encoding()?;
    let (loaded, mut offset) =
        exact_memory_source(block, index, encoding, virtual_definitions, virtual_uses)?;
    let address = exact_evex_memory_sequence_address(block, index, 0)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    let raw = exact_chunk_graph(
        block,
        index,
        &mut offset,
        guest_pc,
        encoded_vector(encoding.source1, encoding.width),
        loaded,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    let lanes = encoding.width.lanes(encoding.elem) as usize;
    if let Some(mask) = encoding.writemask {
        exact_evex_reconstructed_vector_mask_result(
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
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if encoding.zeroing
            || !exact_virtual_definition_use(
                raw,
                lanes + 1,
                lanes + 1,
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

    Some(X86JitEvexChunkShuffleMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
