//! Exact AVX-IFMA VEX `VPMADD52LUQ`/`VPMADD52HUQ` memory-source admission.

use std::collections::HashMap;

use super::{
    X86JitVexBinaryMemorySequence, low_vex_vector_index, virtual_single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecWidth};

/// Exact helper-backed AVX-IFMA sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexIfma52MemorySequence {
    pub(crate) binary: X86JitVexBinaryMemorySequence,
}

/// Validate the complete two-op `VLoad`/`VMultiplyAdd52` decomposition for one
/// AVX-IFMA memory source. Exact source-byte provenance binds both
/// architectural vector inputs, the accumulator/destination alias, high/low
/// product selection, vector length, mandatory prefix, and W=1. The loaded
/// virtual must have exactly one definition and one use, and no other
/// operation may share the guest-instruction frontier.
///
/// Classification is O(1). Callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_ifma52_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexIfma52MemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if !virtual_single_definition_single_use(temporary, virtual_definitions, virtual_uses) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if (index != 0 && block.ops[index - 1].guest_pc == load.guest_pc)
        || consumer.guest_pc != load.guest_pc
        || consumer.x86_hint.is_some()
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::VMultiplyAdd52 {
        dst,
        acc,
        src1,
        src2,
        mask: None,
        width: consumer_width,
        high,
        zeroing: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *acc != *dst || *src2 != temporary || *consumer_width != width {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_width, encoded_high, opcode) =
        instruction.vex_memory_ifma52_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_width,
        encoded_high,
    ) != (destination, source1, width, *high)
    {
        return None;
    }

    Some(X86JitVexIfma52MemorySequence {
        binary: X86JitVexBinaryMemorySequence {
            consumed: 2,
            memory_size: width.bytes(),
            destination,
            source1,
            width,
            map: X86VecMap::Map0F38,
            prefix: X86SsePrefix::OpSize,
            opcode,
            w: true,
            needs_avx2: false,
            needs_fma: false,
        },
    })
}
