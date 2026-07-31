//! Fail-closed helper-backed EVEX affine GFNI memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{
    X86EvexGfniAffineMemoryEncoding, X86EvexGfniAffineMemoryReplay, X86InstructionBytes,
    X86VexGfniMemoryKind,
};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, exact_virtual_definition_use, vector_index,
};
use super::vex_gfni_memory_source::{
    local_gfni_virtual_counts_match, x86_jit_gfni_expansion_sequence,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous EVEX VGF2P8AFFINE[INV]QB memory decomposition consumed by
/// the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexGfniAffineMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexGfniAffineMemoryEncoding,
}

fn exact_unmasked_commit(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    encoding: X86EvexGfniAffineMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    if !exact_virtual_definition_use(raw, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let commit = block.ops.get(index + *offset)?;
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
    *offset += 1;
    Some(())
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VGF2P8AFFINEQB or VGF2P8AFFINEINVQB memory source.
///
/// Exact byte provenance binds the operation, Full tuple, vector width,
/// architectural operands, immediate, mask policy, and native rewrite. The
/// source must be one unconditional complete-vector load or one unconditional
/// 8-byte load plus I64 broadcast, as required by Type E4NF. The expanded GFNI
/// graph, merge/zero tail, virtual dataflow, and guest-PC frontier must all be
/// exact. Classification is O(K + L) time and O(V) auxiliary space for K <=
/// 1,228 affine operations, L <= 64 byte lanes, and V local virtual registers.
pub(crate) fn x86_jit_evex_gfni_affine_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexGfniAffineMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_gfni_affine_memory_encoding()?;
    if !matches!(
        encoding.kind,
        X86VexGfniMemoryKind::Affine | X86VexGfniMemoryKind::AffineInverse
    ) {
        return None;
    }

    let (loaded, mut offset, memory_size) = match encoding.replay {
        X86EvexGfniAffineMemoryReplay::Vector { .. } => {
            let loaded = match &first.kind {
                OpKind::VLoad { dst, addr, width }
                    if first.x86_hint.is_none()
                        && matches!(dst, VReg::Virtual(_))
                        && *width == encoding.width
                        && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            (loaded, 1usize, encoding.width.bytes())
        }
        X86EvexGfniAffineMemoryReplay::Broadcast { .. } => {
            let scalar = match &first.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                } if first.x86_hint.is_none()
                    && matches!(dst, VReg::Virtual(_))
                    && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            let broadcast = block.ops.get(index + 1)?;
            let loaded = match broadcast.kind {
                OpKind::VBroadcast {
                    dst,
                    scalar: actual_scalar,
                    elem: VecElementType::I64,
                    lanes,
                } if broadcast.x86_hint.is_none()
                    && actual_scalar == scalar
                    && lanes == encoding.width.lanes(VecElementType::I64) as u8 =>
                {
                    dst
                }
                _ => return None,
            };
            if broadcast.guest_pc != guest_pc {
                return None;
            }
            (loaded, 2usize, MemWidth::B8.bytes())
        }
    };

    let (raw, core_ops) = x86_jit_gfni_expansion_sequence(
        block,
        index + offset,
        guest_pc,
        encoding.kind,
        encoding.width,
        encoding.source1,
        loaded,
        Some(encoding.immediate),
    )?;
    offset += core_ops;

    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            VecElementType::I8,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        exact_unmasked_commit(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            encoding,
            virtual_definitions,
            virtual_uses,
        )?;
    }

    let sequence = block.ops.get(index..index.checked_add(offset)?)?;
    if block
        .ops
        .get(index + offset)
        .is_some_and(|op| op.guest_pc == guest_pc)
        || !local_gfni_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitEvexGfniAffineMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size,
        encoding,
    })
}
