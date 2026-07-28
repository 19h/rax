//! Exact VEX packed low-product multiply memory-source sequence admission.

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
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
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
    if consumer.guest_pc != load.guest_pc {
        return None;
    }
    let OpKind::VMul {
        dst,
        src1,
        src2,
        elem,
        lanes,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary
        || !matches!(elem, VecElementType::I16 | VecElementType::I32)
        || *lanes != width.lanes(*elem) as u8
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;
    let (map, opcode) = match elem {
        VecElementType::I16 => (X86VecMap::Map0F, 0xD5),
        VecElementType::I32 => (X86VecMap::Map0F38, 0x40),
        _ => return None,
    };
    let Some(X86OpHint::VexOp {
        map: hinted_map,
        pp: X86SsePrefix::OpSize,
        opcode: hinted_opcode,
        width: hinted_width,
        w: hinted_w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if (hinted_map, hinted_opcode, hinted_width) != (map, opcode, width) {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_width, encoded_w) =
        instruction.vex_memory_pmul_low_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
        encoded_w,
    ) != (destination, source1, *elem, width, hinted_w)
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
        // VPMULLW/VPMULLD are WIG. Match both guest values but replay W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
