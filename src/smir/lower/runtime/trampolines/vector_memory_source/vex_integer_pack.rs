//! Exact VEX saturating integer-pack memory-source sequence admission.

use std::collections::HashMap;

use super::{X86JitVexBinaryMemorySequence, low_vex_vector_index, x86_jit_mem_address_shape_valid};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecElementType, VecWidth};

pub(super) fn sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
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
    if virtual_definitions.get(&loaded) != Some(&1) || virtual_uses.get(&loaded) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc {
        return None;
    }
    let OpKind::VPackSat {
        dst,
        src1,
        src2,
        src_elem,
        to_unsigned,
        src_lanes,
        block_lanes,
    } = &consumer.kind
    else {
        return None;
    };
    if *src1 != loaded
        || *src_lanes != width.lanes(*src_elem) as u8
        || *block_lanes != (16 / src_elem.bytes()) as u8
        || !matches!(src_elem, VecElementType::I16 | VecElementType::I32)
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src2, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        encoded_src_elem,
        encoded_to_unsigned,
        encoded_width,
        encoded_map,
        opcode,
        encoded_w,
    ) = instruction.vex_memory_integer_pack_fields()?;
    let map = match encoded_map {
        1 => X86VecMap::Map0F,
        2 => X86VecMap::Map0F38,
        _ => return None,
    };
    if (
        encoded_destination,
        encoded_source1,
        encoded_src_elem,
        encoded_to_unsigned,
        encoded_width,
    ) != (destination, source1, *src_elem, *to_unsigned, width)
        || consumer.x86_hint
            != Some(X86OpHint::VexOp {
                map,
                pp: X86SsePrefix::OpSize,
                opcode,
                width,
                w: encoded_w,
            })
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
        // VPACKSS*/VPACKUS* are WIG. Match both guest values but replay W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
