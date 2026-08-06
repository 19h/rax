//! Fail-closed helper-backed EVEX `VMOVNTDQA` memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg};
use crate::smir::ir::{SmirBlock, X86EvexMovntdqaMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, no_following_same_pc,
    single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous EVEX `VMOVNTDQA` decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexMovntdqaMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86EvexMovntdqaMemoryEncoding,
}

/// Validate one complete `X86CheckAlignment`/`VLoad`/`VMov` group derived
/// from an EVEX.128/256/512 `VMOVNTDQA` memory-source instruction.
///
/// The alignment guard and load must use the same state-backed address, and
/// the required alignment must equal the encoded Full Mem transfer width.
/// The loaded virtual has exactly one definition and use. Complete byte
/// provenance binds map, mandatory prefix, W0, reserved `vvvv/V'`, vector
/// length, destination, unmasked memory-only form, APX address ownership, and
/// instruction length.
///
/// Classification is O(1). Callers construct definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_evex_movntdqa_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexMovntdqaMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guard = block.ops.get(index)?;
    let guest_pc = guard.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_movntdqa_memory_encoding()?;

    let guard_address = match &guard.kind {
        OpKind::X86CheckAlignment { addr, alignment }
            if guard.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr)
                && u32::from(*alignment) == encoding.width.bytes() =>
        {
            addr
        }
        _ => return None,
    };
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, guard_address) {
        return None;
    }

    let load = block.ops.get(index + 1)?;
    let temporary = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.guest_pc == guest_pc
                && load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                && addr == guard_address
                && *width == encoding.width
                && single_definition_single_use(*dst, virtual_definitions, virtual_uses) =>
        {
            *dst
        }
        _ => return None,
    };

    let write = block.ops.get(index + 2)?;
    if write.guest_pc != guest_pc || write.x86_hint.is_some() {
        return None;
    }
    if !matches!(
        write.kind,
        OpKind::VMov { dst, src, width }
            if src == temporary
                && width == encoding.width
                && vector_index(&dst, width) == Some(encoding.destination)
    ) || !no_following_same_pc(block, index, 3, guest_pc)
    {
        return None;
    }

    Some(X86JitEvexMovntdqaMemorySequence {
        consumed: 3,
        encoding,
    })
}
