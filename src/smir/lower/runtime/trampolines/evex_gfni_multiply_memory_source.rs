//! Fail-closed helper-backed EVEX `VGF2P8MULB` memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecElementType, X86Reg};
use crate::smir::ir::{
    X86EvexGfniMultiplyMemoryEncoding, X86EvexGfniMultiplyMemoryReplay, X86InstructionBytes,
    X86VexGfniMemoryKind,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence_tail,
    exact_evex_vector_mask_result, exact_virtual_definition_use, vector_index,
};
use super::vex_gfni_memory_source::{
    local_gfni_virtual_counts_match, x86_jit_gfni_expansion_sequence,
};

/// Exact contiguous EVEX `VGF2P8MULB` memory decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexGfniMultiplyMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexGfniMultiplyMemoryEncoding,
}

#[allow(clippy::too_many_arguments)]
fn exact_multiply_tail(
    block: &crate::smir::ir::SmirBlock,
    tail_index: usize,
    guest_pc: GuestAddr,
    loaded: VReg,
    encoding: X86EvexGfniMultiplyMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let (raw, core_ops) = x86_jit_gfni_expansion_sequence(
        block,
        tail_index,
        guest_pc,
        X86VexGfniMemoryKind::Multiply,
        encoding.width,
        encoding.source1,
        loaded,
        None,
    )?;
    let mut offset = core_ops;
    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            tail_index,
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
        if encoding.zeroing
            || !exact_virtual_definition_use(raw, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let commit = block.ops.get(tail_index + offset)?;
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
    Some(offset)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// `VGF2P8MULB` Full Mem source.
///
/// Exact byte provenance binds W0, vector width, operands, byte writemask,
/// Type E4 source reconstruction, the complete eight-round GF(2^8)
/// multiplication expansion, destination merge/zero policy, APX address
/// guard, local SSA ownership, and guest-PC frontier. Classification is O(L +
/// K) time and O(V) auxiliary space, where L <= 64 byte lanes, K <= 86 GFNI
/// operations, and V is the number of sequence-local virtual registers.
pub(crate) fn x86_jit_evex_gfni_multiply_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexGfniMultiplyMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_gfni_multiply_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexGfniMultiplyMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexGfniMultiplyMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: VecElementType::I8,
        writemask: encoding.writemask,
        zeroing: encoding.zeroing,
        vector_load_hint: None,
        form,
        memory_source_uses: 2,
    };
    let exact = exact_evex_e4_memory_sequence_tail(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |block, tail_index, loaded| {
            exact_multiply_tail(
                block,
                tail_index,
                guest_pc,
                loaded,
                encoding,
                virtual_definitions,
                virtual_uses,
            )
        },
    )?;
    let sequence = block.ops.get(index..index.checked_add(exact.consumed)?)?;
    if !local_gfni_virtual_counts_match(sequence, virtual_definitions, virtual_uses) {
        return None;
    }
    Some(X86JitEvexGfniMultiplyMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
