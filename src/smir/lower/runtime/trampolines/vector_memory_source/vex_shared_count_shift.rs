//! Exact VEX packed shared-count shift memory-source sequence admission.

use std::collections::HashMap;

use super::{X86JitVexBinaryMemorySequence, low_vex_vector_index, x86_jit_mem_address_shape_valid};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, SignExtend, VReg, VecElementType, VecWidth};

pub(super) fn sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexBinaryMemorySequence> {
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad {
            dst,
            addr,
            width: VecWidth::V128,
        } if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            && matches!(dst, VReg::Virtual(_))
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if virtual_definitions.get(&loaded) != Some(&1) || virtual_uses.get(&loaded) != Some(&1) {
        return None;
    }

    let extract = block.ops.get(index + 1)?;
    if extract.guest_pc != load.guest_pc || extract.x86_hint.is_some() {
        return None;
    }
    let count = match &extract.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } if *vec == loaded && matches!(dst, VReg::Virtual(_)) => *dst,
        _ => return None,
    };
    if virtual_definitions.get(&count) != Some(&1) || virtual_uses.get(&count) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 2)?;
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let OpKind::X86PackedShift {
        dst,
        src,
        count: consumer_count,
        width,
        elem,
        shift,
    } = &consumer.kind
    else {
        return None;
    };
    if *consumer_count != count || !matches!(width, VecWidth::V128 | VecWidth::V256) {
        return None;
    }
    let destination = low_vex_vector_index(dst, *width)?;
    let source1 = low_vex_vector_index(src, *width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source,
        encoded_elem,
        encoded_shift,
        encoded_width,
        opcode,
        _encoded_w,
    ) = instruction.vex_memory_shared_count_shift_fields()?;
    if (
        encoded_destination,
        encoded_source,
        encoded_elem,
        encoded_shift,
        encoded_width,
    ) != (destination, source1, *elem, *shift, *width)
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed: 3,
        memory_size: VecWidth::V128.bytes(),
        destination,
        source1,
        width: *width,
        map: X86VecMap::Map0F,
        prefix: X86SsePrefix::OpSize,
        opcode,
        // VEX VPSLL*/VPSRL*/VPSRA* shared-count forms are WIG.
        w: false,
        needs_avx2: *width == VecWidth::V256,
        needs_fma: false,
    })
}
