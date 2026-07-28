//! Exact VEX packed-horizontal integer memory-source sequence admission.

use std::collections::HashMap;

use super::{
    X86JitVexBinaryMemorySequence, low_vex_vector_index, virtual_single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecWidth};

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
    if consumer.guest_pc != load.guest_pc {
        return None;
    }
    let OpKind::VHorizontalBin {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        subtract,
        saturating,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary
        || *lanes != width.lanes(*elem) as u8
        || *block_lanes != (16 / elem.bytes()) as u8
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width: hinted_width,
        w: hinted_w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if hinted_width != width {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_subtract,
        encoded_saturating,
        encoded_width,
        encoded_opcode,
        encoded_w,
    ) = instruction.vex_memory_horizontal_integer_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_subtract,
        encoded_saturating,
        encoded_width,
        encoded_opcode,
        encoded_w,
    ) != (
        destination,
        source1,
        *elem,
        *subtract,
        *saturating,
        width,
        opcode,
        hinted_w,
    ) {
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
        opcode,
        // VPHADD*/VPHSUB* are WIG. Replay one canonical W=0 encoding.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
