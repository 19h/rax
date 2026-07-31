//! Fail-closed helper-backed EVEX VPERMI2*/VPERMT2* memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, SignExtend, VReg, X86Reg};
use crate::smir::ir::{
    X86EvexTwoTablePermuteMemoryEncoding, X86EvexTwoTablePermuteMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// VPERMI2*/VPERMT2* memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexTwoTablePermuteMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexTwoTablePermuteMemoryEncoding,
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

/// Validate the complete O0/O1/O2 decomposition for one EVEX
/// VPERMI2*/VPERMT2* memory source.
///
/// Exact provenance binds opcode direction, element/vector width,
/// destination, EVEX.vvvv, mask policy, and tuple shape. The matcher requires
/// one unconditional E4NF/E4NF.nb memory operation, the exact two-table
/// permute, every merge/zero lane, all virtual definition/use counts, and the
/// guest-PC frontier. Runtime is O(L) and auxiliary space is O(1) for
/// L <= 64 lanes; callers construct definition/use maps once in O(N) time and
/// O(V) space.
pub(crate) fn x86_jit_evex_two_table_permute_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexTwoTablePermuteMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_two_table_permute_memory_encoding()?;
    let (loaded, mut offset) = match encoding.replay {
        X86EvexTwoTablePermuteMemoryReplay::Vector { .. } => {
            let loaded = match &first.kind {
                OpKind::VLoad { dst, addr, width }
                    if matches!(
                        first.x86_hint,
                        None | Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                    ) && *width == encoding.width
                        && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
                return None;
            }
            (loaded, 1)
        }
        X86EvexTwoTablePermuteMemoryReplay::Broadcast { memory_width, .. } => {
            let scalar = match &first.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                } if first.x86_hint.is_none()
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
                    && lanes == encoding.width.lanes(encoding.elem) as u8 =>
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
            (loaded, 2)
        }
    };

    let expected_table1 = if encoding.overwrite_table {
        encoding.destination
    } else {
        encoding.source1
    };
    let expected_indices = if encoding.overwrite_table {
        encoding.source1
    } else {
        encoding.destination
    };
    let permute = block.ops.get(index + offset)?;
    let raw = match permute.kind {
        OpKind::VPermute {
            dst,
            src1,
            src2: Some(src2),
            indices,
            elem,
            width,
            overwrite_table,
        } if permute.x86_hint.is_none()
            && vector_index(&src1, encoding.width) == Some(expected_table1)
            && src2 == loaded
            && vector_index(&indices, encoding.width) == Some(expected_indices)
            && elem == encoding.elem
            && width == encoding.width
            && overwrite_table == encoding.overwrite_table
            && matches!(dst, VReg::Virtual(_)) =>
        {
            dst
        }
        _ => return None,
    };
    if permute.guest_pc != guest_pc {
        return None;
    }
    offset += 1;

    if let Some(mask_index) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask_index))),
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if encoding.zeroing || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.x86_hint.is_some()
            || commit.guest_pc != guest_pc
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
    Some(X86JitEvexTwoTablePermuteMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size: encoding.memory_size,
        encoding,
    })
}
