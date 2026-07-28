//! Fail-closed helper-backed EVEX packed FMA3 memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexPackedFma3MemoryEncoding, X86InstructionBytes};

use super::vector_memory_source::{vex_fma3_kind, vex_fma3_order};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous unmasked, non-broadcast EVEX packed FMA3 memory-source
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFma3MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFma3MemoryEncoding,
}

fn vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=31))), VecWidth::V256)
        | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(index @ 0..=31))), VecWidth::V512) => Some(*index),
        _ => None,
    }
}

fn single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&1)
        && virtual_uses.get(&register) == Some(&1)
}

/// Validate the complete three-op decomposition emitted for one unmasked,
/// non-broadcast EVEX packed FMA3 memory source. Exact instruction provenance
/// binds vector width, element type, architectural operands, opcode semantics,
/// and the native register-source rewrite. Both virtual results must have
/// exactly one definition and one use in the complete block.
///
/// Classification is O(1) time and O(1) auxiliary space. Callers build the
/// global definition/use maps once in O(N) time and O(V) space for N operations
/// and V virtual registers.
pub(crate) fn x86_jit_evex_packed_fma3_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFma3MemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
        return None;
    }

    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .evex_packed_fma3_memory_encoding()?;
    if encoding.width != width {
        return None;
    }
    let elem = encoding.elem;

    let fma = block.ops.get(index + 1)?;
    let (raw, src1, src2, src3, mask, kind, order, round, lanes) = match &fma.kind {
        OpKind::X86Fma(fma_op) if elem != VecElementType::F16 => (
            fma_op.dst,
            fma_op.src1,
            fma_op.src2,
            fma_op.src3,
            fma_op.mask,
            fma_op.kind,
            fma_op.order,
            fma_op.round,
            fma_op.lanes,
        ),
        OpKind::X86FP16Fma {
            dst,
            src1,
            src2,
            src3,
            mask,
            kind,
            order,
            round,
            lanes,
        } if elem == VecElementType::F16 => (
            *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
        ),
        _ => return None,
    };
    if fma.guest_pc != load.guest_pc
        || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        || vector_index(&src1, width) != Some(encoding.destination)
        || vector_index(&src2, width) != Some(encoding.source1)
        || src3 != loaded
        || mask.is_some()
        || kind != vex_fma3_kind(encoding.opcode)?
        || order != vex_fma3_order(encoding.opcode)?
        || round != FpRoundMode::Dynamic
        || lanes != width.lanes(elem) as u8
        || fma.x86_hint
            != Some(X86OpHint::EvexOp {
                map: if elem == VecElementType::F16 {
                    X86VecMap::Map6
                } else {
                    X86VecMap::Map0F38
                },
                pp: X86SsePrefix::OpSize,
                opcode: encoding.opcode,
                width,
                w: encoding.w,
            })
    {
        return None;
    }

    let result = block.ops.get(index + 2)?;
    if result.guest_pc != load.guest_pc
        || result.x86_hint.is_some()
        || !matches!(
            result.kind,
            OpKind::VMov {
                dst,
                src,
                width: result_width,
            } if vector_index(&dst, width) == Some(encoding.destination)
                && src == raw
                && result_width == width
        )
        || block
            .ops
            .get(index + 3)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    Some(X86JitEvexPackedFma3MemorySequence {
        consumed: 3,
        memory_size: width.bytes(),
        encoding,
    })
}
