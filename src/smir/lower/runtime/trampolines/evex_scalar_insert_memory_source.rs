//! Fail-closed helper-backed EVEX scalar-insert memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, GuestAddr, VReg};
use crate::smir::ir::{SmirBlock, X86EvexScalarInsertMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
};
use super::vex_scalar_insert_memory_source::{
    X86ScalarInsertGraphEncoding, x86_jit_scalar_insert_memory_graph_sequence,
};

/// Exact contiguous Type-E9NF EVEX scalar-insert memory decomposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarInsertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexScalarInsertMemoryEncoding,
}

/// Validate one complete EVEX `VINSERTPS` or `VPINSR*` memory-source graph.
///
/// Complete byte provenance binds the EVEX.128 encoding, destination and merge
/// registers, W/opcode feature class, imm8, Tuple1 Scalar width, and the
/// helper-only APX address frontier. The shared canonical graph matcher retains
/// the unconditional load even when `VINSERTPS` zeroing discards its value.
/// Classification is O(1) time and space after the caller's O(N) virtual-use
/// maps have been built.
pub(crate) fn x86_jit_evex_scalar_insert_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarInsertMemorySequence> {
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
        .evex_scalar_insert_memory_encoding()?;
    let graph = x86_jit_scalar_insert_memory_graph_sequence(
        block,
        index,
        X86ScalarInsertGraphEncoding {
            destination: encoding.destination,
            source1: encoding.source1,
            kind: encoding.kind,
            immediate: encoding.immediate,
        },
        virtual_definitions,
        virtual_uses,
    )?;
    let address = match &block.ops.get(index)?.kind {
        OpKind::Load { addr, .. } => addr,
        _ => unreachable!("shared scalar-insert graph starts with Load"),
    };
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    Some(X86JitEvexScalarInsertMemorySequence {
        consumed: graph.consumed,
        memory_size: graph.memory_size,
        encoding,
    })
}
