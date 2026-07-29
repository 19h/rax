//! Exact VEX packed interleave memory-source sequence admission.

use std::collections::HashMap;

use super::{X86JitVexBinaryMemorySequence, low_vex_vector_index, x86_jit_mem_address_shape_valid};
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
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(
                load.x86_hint,
                None | Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            ) && matches!(dst, VReg::Virtual(_))
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
    let OpKind::VInterleave {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        high,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != loaded
        || *lanes != width.lanes(*elem) as u8
        || *block_lanes != (16 / elem.bytes()) as u8
        || !matches!(
            elem,
            VecElementType::F32
                | VecElementType::F64
                | VecElementType::I8
                | VecElementType::I16
                | VecElementType::I32
                | VecElementType::I64
        )
    {
        return None;
    }
    let floating = matches!(elem, VecElementType::F32 | VecElementType::F64);
    if load.x86_hint
        != if floating {
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
        } else {
            None
        }
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_high,
        encoded_width,
        opcode,
        encoded_w,
    ) = instruction.vex_memory_interleave_fields()?;
    let prefix = match encoded_elem {
        VecElementType::F32 => X86SsePrefix::None,
        VecElementType::F64
        | VecElementType::I8
        | VecElementType::I16
        | VecElementType::I32
        | VecElementType::I64 => X86SsePrefix::OpSize,
        _ => return None,
    };
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_high,
        encoded_width,
    ) != (destination, source1, *elem, *high, width)
        || consumer.x86_hint
            != Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: prefix,
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
        map: X86VecMap::Map0F,
        prefix,
        opcode,
        // V(P)UNPCK* is WIG. Match both guest values but replay W=0.
        w: false,
        needs_avx2: !floating && width == VecWidth::V256,
        needs_fma: false,
    })
}
