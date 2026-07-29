//! Exact VEX packed floating-point binary memory-source admission.

use std::collections::HashMap;

use super::{
    X86JitVexBinaryMemorySequence, low_vex_vector_index, virtual_single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    BlockId, FpRoundMode, GuestAddr, VReg, VecElementType, VecWidth, X86FpBinaryOp,
};

pub(super) fn packed_arithmetic_encoding_valid(
    kind: &OpKind,
    map: X86VecMap,
    prefix: X86SsePrefix,
    opcode: u8,
) -> bool {
    let OpKind::X86FpBinary {
        mask,
        elem,
        lanes,
        op,
        round,
        suppress_exceptions,
        ..
    } = kind
    else {
        return false;
    };
    let expected_op = match opcode {
        0x58 => X86FpBinaryOp::Add,
        0x59 => X86FpBinaryOp::Mul,
        0x5C => X86FpBinaryOp::Sub,
        0x5D => X86FpBinaryOp::Min,
        0x5E => X86FpBinaryOp::Div,
        0x5F => X86FpBinaryOp::Max,
        _ => return false,
    };
    let expected_prefix = match elem {
        VecElementType::F32 => X86SsePrefix::None,
        VecElementType::F64 => X86SsePrefix::OpSize,
        _ => return false,
    };
    map == X86VecMap::Map0F
        && prefix == expected_prefix
        && *op == expected_op
        && mask.is_none()
        && *round == FpRoundMode::Dynamic
        && !*suppress_exceptions
        && matches!(
            (elem, lanes),
            (VecElementType::F32, 4 | 8) | (VecElementType::F64, 2 | 4)
        )
}

pub(super) fn horizontal_addsub_sequence(
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
    let OpKind::X86FpBinary {
        dst,
        src1,
        src2,
        mask,
        elem,
        lanes,
        op,
        round,
        suppress_exceptions,
    } = &consumer.kind
    else {
        return None;
    };
    let opcode = match op {
        X86FpBinaryOp::AddSub => 0xD0,
        X86FpBinaryOp::HorizontalAdd => 0x7C,
        X86FpBinaryOp::HorizontalSub => 0x7D,
        _ => return None,
    };
    let prefix = match elem {
        VecElementType::F32 => X86SsePrefix::Repne,
        VecElementType::F64 => X86SsePrefix::OpSize,
        _ => return None,
    };
    if *src2 != temporary
        || *lanes != width.lanes(*elem) as u8
        || mask.is_some()
        || *round != FpRoundMode::Dynamic
        || *suppress_exceptions
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let source1 = low_vex_vector_index(src1, width)?;
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: hinted_prefix,
        opcode: hinted_opcode,
        width: hinted_width,
        w: hinted_w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if (hinted_prefix, hinted_opcode, hinted_width) != (prefix, opcode, width) {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_operation,
        encoded_width,
        encoded_opcode,
        encoded_w,
    ) = instruction.vex_memory_fp_horizontal_addsub_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_operation,
        encoded_width,
        encoded_opcode,
        encoded_w,
    ) != (destination, source1, *elem, *op, width, opcode, hinted_w)
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
        // VADDSUB*/VHADD*/VHSUB* are WIG. Replay one canonical W=0 encoding.
        w: false,
        needs_avx2: false,
        needs_fma: false,
    })
}
