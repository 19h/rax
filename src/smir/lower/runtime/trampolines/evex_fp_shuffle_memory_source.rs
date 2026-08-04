//! Fail-closed helper-backed EVEX VSHUFPS/VSHUFPD memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, SignExtend, VReg, VecWidth, X86Reg};
use crate::smir::ir::{
    X86EvexFpShuffleMemoryEncoding, X86EvexFpShuffleMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_vector_mask_result,
    exact_two_source_fp_shuffle_imm_graph, exact_virtual_definition_use, no_following_same_pc,
    single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// floating-point shuffle memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexFpShuffleMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexFpShuffleMemoryEncoding,
}

fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexFpShuffleMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(VReg, usize)> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    match encoding.replay {
        X86EvexFpShuffleMemoryReplay::Vector { .. } => {
            let load = block.ops.get(index)?;
            let loaded = match &load.kind {
                OpKind::VLoad { dst, addr, width }
                    if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                        && *width == encoding.width
                        && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            single_definition_single_use(loaded, virtual_definitions, virtual_uses)
                .then_some((loaded, 1))
        }
        X86EvexFpShuffleMemoryReplay::Broadcast { memory_width, .. } => {
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
                || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
            {
                return None;
            }
            Some((loaded, 2))
        }
    }
}

fn encoded_vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX floating shuffle width"),
    }))
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VSHUFPS/VSHUFPD memory source.
///
/// Exact provenance binds W/pp, vector and element widths, operands, imm8,
/// every generated selector, destination mask policy, one unconditional E4NF
/// tuple read, the APX address guard, and the guest-PC frontier. Runtime is
/// O(L) and auxiliary space is O(1) for L <= 16 lanes; callers construct
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_fp_shuffle_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFpShuffleMemorySequence> {
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
        .evex_fp_shuffle_memory_encoding()?;
    let (loaded, mut offset) =
        exact_memory_source(block, index, encoding, virtual_definitions, virtual_uses)?;
    let address = exact_evex_memory_sequence_address(block, index, 0)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    let raw = exact_two_source_fp_shuffle_imm_graph(
        block,
        index,
        &mut offset,
        guest_pc,
        encoded_vector(encoding.source1, encoding.width),
        loaded,
        encoding.width,
        encoding.elem,
        encoding.immediate,
        virtual_definitions,
        virtual_uses,
    )?;

    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
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
            || !exact_virtual_definition_use(raw, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.guest_pc != guest_pc
            || commit.x86_hint.is_some()
            || !matches!(
                commit.kind,
                OpKind::VMov {
                    dst,
                    src,
                    width,
                } if vector_index(&dst, encoding.width) == Some(encoding.destination)
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
    Some(X86JitEvexFpShuffleMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
