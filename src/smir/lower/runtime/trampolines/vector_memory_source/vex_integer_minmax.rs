//! Exact VEX packed-integer minimum/maximum memory-source sequence admission.

use std::collections::HashMap;

use super::{X86JitVexBinaryMemorySequence, low_vex_vector_index, x86_jit_mem_address_shape_valid};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecCmpCond, VecElementType, VecWidth};

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
    if virtual_definitions.get(&loaded) != Some(&1) || virtual_uses.get(&loaded) != Some(&2) {
        return None;
    }

    let compare = block.ops.get(index + 1)?;
    if compare.guest_pc != load.guest_pc || compare.x86_hint.is_some() {
        return None;
    }
    let OpKind::VCmp {
        dst: select,
        src1,
        src2,
        cond,
        elem,
        lanes,
    } = &compare.kind
    else {
        return None;
    };
    if !matches!(select, VReg::Virtual(_))
        || *select == loaded
        || *src2 != loaded
        || *lanes != width.lanes(*elem) as u8
        || virtual_definitions.get(select) != Some(&1)
        || virtual_uses.get(select) != Some(&1)
    {
        return None;
    }

    let select_op = block.ops.get(index + 2)?;
    if select_op.guest_pc != load.guest_pc || select_op.x86_hint.is_some() {
        return None;
    }
    let OpKind::VBitSelect {
        dst,
        mask,
        src_true,
        src_false,
        width: select_width,
    } = &select_op.kind
    else {
        return None;
    };
    if *mask != *select || *src_true != *src1 || *src_false != loaded || *select_width != width {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        minimum,
        signed,
        encoded_width,
        map,
        opcode,
        _encoded_w,
    ) = instruction.vex_memory_integer_minmax_fields()?;
    let expected_cond = match (minimum, signed) {
        (true, true) => VecCmpCond::Lt,
        (true, false) => VecCmpCond::Ltu,
        (false, true) => VecCmpCond::Gt,
        (false, false) => VecCmpCond::Gtu,
    };
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
        expected_cond,
    ) != (destination, source1, *elem, width, *cond)
    {
        return None;
    }

    Some(X86JitVexBinaryMemorySequence {
        consumed: 3,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        map: match map {
            1 => X86VecMap::Map0F,
            2 => X86VecMap::Map0F38,
            _ => unreachable!("classified packed-integer min/max VEX map"),
        },
        prefix: X86SsePrefix::OpSize,
        opcode,
        // VPMIN*/VPMAX* are WIG. Match both guest values but replay W=0.
        w: false,
        needs_avx2: width == VecWidth::V256,
        needs_fma: false,
    })
}
