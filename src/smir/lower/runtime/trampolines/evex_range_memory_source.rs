//! Fail-closed helper-backed EVEX VRANGE memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, X86Reg};
use crate::smir::ir::{X86EvexRangeMemoryEncoding, X86EvexRangeMemoryReplay, X86InstructionBytes};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// VRANGEPD/PS/SD/SS memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexRangeMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexRangeMemoryEncoding,
}

fn exact_range(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexRangeMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    matches!(
        op.kind,
        OpKind::X86Range {
            dst,
            src1,
            src2,
            mask,
            elem,
            width,
            lanes,
            imm,
            scalar,
            mask_zeroing,
            suppress_exceptions,
        } if vector_index(&dst, encoding.width) == Some(encoding.destination)
            && vector_index(&src1, encoding.width) == Some(encoding.source1)
            && src2 == memory_source
            && mask == expected_mask
            && elem == encoding.elem
            && width == encoding.width
            && lanes == if encoding.scalar {
                1
            } else {
                encoding.width.lanes(encoding.elem) as u8
            }
            && imm == encoding.immediate
            && scalar == encoding.scalar
            && mask_zeroing == encoding.zeroing
            && !suppress_exceptions
            && op.x86_hint == Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: if encoding.scalar { 0x51 } else { 0x50 },
                width: encoding.width,
                w: encoding.elem == crate::smir::ir::types::VecElementType::F64,
            })
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed or
/// scalar AVX-512 VRANGE memory source.
///
/// Exact provenance binds scalar/packed class, precision, vector width,
/// architectural operands, writemask policy, broadcast/full-vector tuple,
/// helper address, the reserved-zero imm8 high nibble, MXCSR behavior, and the
/// sole architectural destination commit. Classification is O(L) time and
/// O(1) auxiliary space for L <= 16 lanes; callers build definition/use maps
/// once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_range_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexRangeMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_range_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexRangeMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexRangeMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexRangeMemoryReplay::MaskedVector { .. } => X86EvexE4MemoryReplayForm::MaskedVector,
        X86EvexRangeMemoryReplay::Scalar { .. } => X86EvexE4MemoryReplayForm::Scalar,
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: encoding.zeroing,
        vector_load_hint: None,
        form,
    };
    let exact = exact_evex_e4_memory_sequence(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |op, memory_source| exact_range(op, memory_source, encoding),
    )?;
    Some(X86JitEvexRangeMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
