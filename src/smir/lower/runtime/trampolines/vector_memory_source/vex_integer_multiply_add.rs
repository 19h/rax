//! Exact VEX packed-integer multiply-add memory-source sequence admission.

use std::collections::HashMap;

use super::{
    X86JitVexBinaryMemorySequence, low_vex_vector_index, virtual_single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecElementType, VecWidth};

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
            if matches!(dst, VReg::Virtual(_))
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
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let OpKind::VDotProduct {
        dst,
        acc: VReg::Imm(0),
        src1,
        src2,
        mask: None,
        src_elem,
        acc_elem,
        width: consumer_width,
        src1_unsigned,
        saturate,
        zeroing: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary || *consumer_width != width {
        return None;
    }
    let (expected_acc_elem, expected_unsigned, expected_saturate, expected_load_hint, map, opcode) =
        match src_elem {
            VecElementType::I8 => (
                VecElementType::I16,
                true,
                true,
                None,
                X86VecMap::Map0F38,
                0x04,
            ),
            VecElementType::I16 => (
                VecElementType::I32,
                false,
                false,
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
                X86VecMap::Map0F,
                0xF5,
            ),
            _ => return None,
        };
    if *acc_elem != expected_acc_elem
        || *src1_unsigned != expected_unsigned
        || *saturate != expected_saturate
        || load.x86_hint != expected_load_hint
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_source_element, encoded_width, _encoded_w) =
        instruction.vex_memory_integer_multiply_add_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_source_element,
        encoded_width,
    ) != (destination, source1, *src_elem, width)
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map,
        prefix: X86SsePrefix::OpSize,
        opcode,
        // VPMADDUBSW/VPMADDWD are WIG. Match both guest values but replay W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
