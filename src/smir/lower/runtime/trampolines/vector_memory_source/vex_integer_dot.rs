//! Exact base AVX-VNNI VEX dot-product memory-source admission.

use std::collections::HashMap;

use super::{
    X86JitVexBinaryMemorySequence, low_vex_vector_index, virtual_single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecElementType, VecWidth};

/// Exact helper-backed base AVX-VNNI sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexIntegerDotMemorySequence {
    pub(crate) binary: X86JitVexBinaryMemorySequence,
}

/// Validate the complete two-op `VLoad`/`VDotProduct` decomposition for one
/// base AVX-VNNI memory source. Exact source-byte provenance binds both
/// architectural vector inputs, the destination/accumulator alias, source
/// signedness, saturation, element width, vector length, and W=0. The loaded
/// virtual must have exactly one definition and one use, and no other
/// operation may share the guest-instruction frontier.
///
/// Classification is O(1). Callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_integer_dot_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexIntegerDotMemorySequence> {
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
    let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask: None,
        src_elem,
        acc_elem: VecElementType::I32,
        width: consumer_width,
        src1_unsigned,
        saturate,
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
    let encoded = instruction.vex_memory_integer_dot_fields()?;
    if (
        encoded.destination,
        encoded.source1,
        encoded.src_elem,
        encoded.width,
        encoded.src1_unsigned,
        encoded.saturate,
    ) != (
        destination,
        source1,
        *src_elem,
        width,
        *src1_unsigned,
        *saturate,
    ) {
        return None;
    }

    Some(X86JitVexIntegerDotMemorySequence {
        binary: X86JitVexBinaryMemorySequence {
            consumed: 2,
            memory_size: width.bytes(),
            destination,
            source1,
            width,
            map: X86VecMap::Map0F38,
            prefix: X86SsePrefix::OpSize,
            opcode: encoded.opcode,
            w: false,
            needs_avx2: false,
            needs_fma: false,
        },
    })
}
