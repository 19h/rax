//! Exact AVX2 per-element variable-shift memory-source admission.

use std::collections::HashMap;

use super::{
    X86JitVexBinaryMemorySequence, low_vex_vector_index, virtual_single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, ShiftOp, VReg, VecElementType, VecWidth};

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
    let OpKind::X86PackedShiftVariable {
        dst,
        src,
        count,
        mask: None,
        width: consumer_width,
        elem,
        shift,
        zeroing: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *count != temporary || *consumer_width != width {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_shift, encoded_width) =
        instruction.vex_memory_variable_shift_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_shift,
        encoded_width,
    ) != (destination, source1, *elem, *shift, width)
    {
        return None;
    }

    let (opcode, w) = match (elem, shift) {
        (VecElementType::I32, ShiftOp::Lsr) => (0x45, false),
        (VecElementType::I64, ShiftOp::Lsr) => (0x45, true),
        (VecElementType::I32, ShiftOp::Asr) => (0x46, false),
        (VecElementType::I32, ShiftOp::Lsl) => (0x47, false),
        (VecElementType::I64, ShiftOp::Lsl) => (0x47, true),
        _ => return None,
    };

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: X86VecMap::Map0F38,
        prefix: X86SsePrefix::OpSize,
        opcode,
        w,
        // Every VEX encoding in this family requires AVX2 at both widths.
        needs_avx2: true,
        needs_fma: false,
    })
}
