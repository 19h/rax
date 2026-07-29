//! Exact VEX `VPSHUFB` memory-source sequence admission.

use std::collections::HashMap;

use super::{
    X86JitVexBinaryMemorySequence, low_vex_vector_index, virtual_single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecElementType, VecWidth};

/// Validate the complete two-op `VLoad`/`VByteShuffle` decomposition for one
/// VEX `VPSHUFB` memory source. Exact source-byte provenance binds both
/// architectural inputs, destination, vector width, and WIG encoding.
///
/// The classifier is O(1); callers construct definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(super) fn sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
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
    if consumer.guest_pc != load.guest_pc
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        block_lanes: 16,
    } = &consumer.kind
    else {
        return None;
    };
    if *control != temporary || u32::from(*lanes) != width.lanes(VecElementType::I8) {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src, width)?;
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0x00,
        width: hinted_width,
        w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if hinted_width != width {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_width, encoded_w) =
        instruction.vex_memory_byte_shuffle_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_width,
        encoded_w,
    ) != (destination, source1, width, w)
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: X86VecMap::Map0F38,
        prefix: X86SsePrefix::OpSize,
        opcode: 0x00,
        w,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
