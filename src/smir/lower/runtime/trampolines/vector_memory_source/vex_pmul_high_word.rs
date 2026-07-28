//! Exact VEX packed high-word multiply memory-source sequence admission.

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
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem: VecElementType::I16,
        lanes,
        signed1,
        signed2,
        shift_left: 0,
        round: false,
        sat_bits: 0,
        out_shift: 16,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary
        || *signed1 != *signed2
        || *lanes != width.lanes(VecElementType::I16) as u8
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_signed, encoded_width, _encoded_w) =
        instruction.vex_memory_pmul_high_word_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_signed,
        encoded_width,
    ) != (destination, source1, *signed1, width)
    {
        return None;
    }
    let opcode = if *signed1 { 0xE5 } else { 0xE4 };

    Some(X86JitVexBinaryMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: X86VecMap::Map0F,
        prefix: X86SsePrefix::OpSize,
        opcode,
        // VPMULHUW/VPMULHW are WIG. Match both guest values but replay W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
