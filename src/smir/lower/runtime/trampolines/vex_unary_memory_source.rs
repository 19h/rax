//! Fail-closed helper-backed VEX unary memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, VReg, VecElementType, VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexPhminposuwMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VPABSB`/`VPABSW`/`VPABSD` memory-source sequence
/// consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexPackedAbsMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) elem: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
}

/// Exact contiguous VEX.128 `VPHMINPOSUW` memory-source sequence consumed by
/// the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexPhminposuwMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexPhminposuwMemoryEncoding,
}

fn low_vex_vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(*index),
        _ => None,
    }
}

/// Validate one exact `VLoad`/`X86Phminposuw` pair derived from a complete
/// VEX.128 memory-source instruction.
///
/// The loaded virtual must have exactly one definition and use. Both
/// operations must form the complete same-PC instruction frontier. Exact byte
/// provenance binds destination, ignored W value, vector length, mandatory
/// prefix, opcode, and complete memory-address encoding.
///
/// Classification is O(1). Callers construct definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_phminposuw_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexPhminposuwMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == load.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_phminposuw_memory_encoding()?;

    let temporary = match &load.kind {
        OpKind::VLoad {
            dst,
            addr,
            width: VecWidth::V128,
        } if matches!(dst, VReg::Virtual(_))
            && matches!(
                load.x86_hint,
                Some(X86OpHint::VecAlign(
                    X86VecAlign::Unaligned | X86VecAlign::Aligned
                ))
            )
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::X86Phminposuw { dst, src } = consumer.kind else {
        return None;
    };
    let destination = match dst {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))) => index,
        _ => return None,
    };
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0x41,
        width: VecWidth::V128,
        w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if src != temporary || (encoding.destination, encoding.w) != (destination, w) {
        return None;
    }

    Some(X86JitVexPhminposuwMemorySequence {
        consumed: 2,
        encoding,
    })
}

/// Validate one exact `VLoad`/`VUnary(Abs)` pair derived from a complete VEX
/// memory-source instruction. The virtual memory value must have exactly one
/// definition and one use, and the consumer must be adjacent at the same guest
/// PC. Exact byte provenance prevents a fabricated or mismatched SMIR hint from
/// bypassing the native admission frontier.
///
/// The classifier is O(1); callers construct definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_packed_abs_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexPackedAbsMemorySequence> {
    if !allow_mem {
        return None;
    }
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
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc {
        return None;
    }
    let OpKind::VUnary {
        dst,
        src,
        elem,
        lanes,
        op: VecUnaryOp::Abs,
    } = &consumer.kind
    else {
        return None;
    };
    if *src != temporary
        || *lanes != width.lanes(*elem) as u8
        || !matches!(
            elem,
            VecElementType::I8 | VecElementType::I16 | VecElementType::I32
        )
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let expected_opcode = match elem {
        VecElementType::I8 => 0x1C,
        VecElementType::I16 => 0x1D,
        VecElementType::I32 => 0x1E,
        _ => unreachable!("filtered packed-absolute-value element type"),
    };
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width: hint_width,
        w,
    }) = consumer.x86_hint
    else {
        return None;
    };
    if opcode != expected_opcode || hint_width != width {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_elem, encoded_width, encoded_w) =
        instruction.vex_memory_packed_abs_fields()?;
    if (encoded_destination, encoded_elem, encoded_width, encoded_w)
        != (destination, *elem, width, w)
    {
        return None;
    }

    Some(X86JitVexPackedAbsMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        elem: *elem,
        width,
        opcode,
        w,
    })
}
