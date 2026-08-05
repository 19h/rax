//! Fail-closed helper-backed EVEX `VPSADBW` memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecWidth};
use crate::smir::ir::{X86EvexPsadbwMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_virtual_definition_use, no_following_same_pc, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact two-op decomposition consumed by the helper-backed x86-64 EVEX
/// `VPSADBW` memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPsadbwMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexPsadbwMemoryEncoding,
}

/// Validate the complete O0/O1/O2 `VLoad`/`VSadBytes` decomposition emitted
/// for one EVEX `VPSADBW` Full Mem source.
///
/// Exact provenance binds WIG, vector width, all three vector operands, one
/// unconditional E4NF.nb tuple read, the APX address guard, and the sole
/// same-PC frontier. Matching is O(1); callers build definition/use maps once
/// in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_psadbw_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPsadbwMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let (temporary, address, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, addr, *width)
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(temporary, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let operation = block.ops.get(index + 1)?;
    let (destination, source1) = match operation.kind {
        OpKind::VSadBytes {
            dst,
            src1,
            src2,
            width: operation_width,
        } if operation.guest_pc == guest_pc
            && operation.x86_hint.is_none()
            && src2 == temporary
            && operation_width == width =>
        {
            (vector_index(&dst, width)?, vector_index(&src1, width)?)
        }
        _ => return None,
    };
    if !no_following_same_pc(block, index, 2, guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }

    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_psadbw_memory_encoding()?;
    if (encoding.width, encoding.destination, encoding.source1) != (width, destination, source1) {
        return None;
    }

    Some(X86JitEvexPsadbwMemorySequence {
        consumed: 2,
        address_offset: 0,
        encoding,
    })
}
