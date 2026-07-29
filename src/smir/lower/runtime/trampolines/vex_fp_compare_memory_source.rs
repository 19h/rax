//! Fail-closed helper-backed VEX VCMPPS/VCMPPD memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecElementType, VecWidth, X86Reg};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous packed VEX floating-point comparison memory-source
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexFpCompareMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) elem: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) predicate: u8,
    pub(crate) w: bool,
}

fn low_vex_vector_index(reg: VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(index),
        _ => None,
    }
}

/// Validate the complete two-op `VLoad`/`X86VectorFpCompare` decomposition for
/// one AVX VEX `VCMPPS`/`VCMPPD` memory source. Source-byte provenance binds
/// both architectural inputs, destination, element type, vector width, WIG
/// encoding, and the five-bit predicate. The loaded virtual must have exactly
/// one definition and one use, and no same-PC tail may remain unconsumed.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_fp_compare_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexFpCompareMemorySequence> {
    if !allow_mem {
        return None;
    }
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
    let OpKind::X86VectorFpCompare {
        dst,
        src1,
        src2,
        mask: None,
        elem,
        width: consumer_width,
        lanes,
        predicate,
        scalar: false,
        mask_destination: false,
        zero_upper: true,
        suppress_exceptions: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary
        || *consumer_width != width
        || *lanes != width.lanes(*elem) as u8
        || !matches!(*elem, VecElementType::F32 | VecElementType::F64)
    {
        return None;
    }
    let destination = low_vex_vector_index(*dst, width)?;
    let source1 = low_vex_vector_index(*src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_width, encoded_predicate, w) =
        instruction.vex_memory_packed_fp_compare_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
        encoded_predicate,
    ) != (destination, source1, *elem, width, *predicate)
    {
        return None;
    }
    let expected_prefix = if *elem == VecElementType::F32 {
        X86SsePrefix::None
    } else {
        X86SsePrefix::OpSize
    };
    if consumer.x86_hint
        != Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: expected_prefix,
            opcode: 0xC2,
            width,
            w,
        })
    {
        return None;
    }

    Some(X86JitVexFpCompareMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        elem: *elem,
        width,
        predicate: *predicate,
        w,
    })
}
