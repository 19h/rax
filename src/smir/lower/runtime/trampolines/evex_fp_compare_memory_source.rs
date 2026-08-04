//! Fail-closed helper-backed packed EVEX floating-point comparison admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecElementType, X86Reg};
use crate::smir::ir::{
    X86EvexPackedFpCompareMemoryEncoding, X86EvexPackedFpCompareMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed packed EVEX
/// floating-point comparison memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFpCompareMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFpCompareMemoryEncoding,
}

fn exact_compare(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexPackedFpCompareMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let (map, prefix, w) = match encoding.elem {
        VecElementType::F16 => (X86VecMap::Map0F3A, X86SsePrefix::None, false),
        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, false),
        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, true),
        _ => return false,
    };
    matches!(
        op.kind,
        OpKind::X86VectorFpCompare {
            dst: VReg::Arch(ArchReg::X86(X86Reg::K(destination))),
            src1,
            src2,
            mask,
            elem,
            width,
            lanes,
            predicate,
            scalar: false,
            mask_destination: true,
            zero_upper: false,
            suppress_exceptions: false,
        } if destination == encoding.destination
            && vector_index(&src1, encoding.width) == Some(encoding.source1)
            && src2 == memory_source
            && mask == expected_mask
            && elem == encoding.elem
            && width == encoding.width
            && lanes == encoding.width.lanes(encoding.elem) as u8
            && predicate == encoding.predicate
            && op.x86_hint == Some(X86OpHint::EvexOp {
                map,
                pp: prefix,
                opcode: 0xC2,
                width: encoding.width,
                w,
            })
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed EVEX
/// floating-point comparison memory source.
///
/// Exact provenance binds the Type-E2 precision and tuple, vector width,
/// source vector, K destination and writemask, five-bit predicate, helper
/// address, dynamic MXCSR behavior, and the sole K-register commit.
/// Classification is O(L) time and O(1) auxiliary space for L <= 32 lanes;
/// callers build definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_fp_compare_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFpCompareMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_fp_compare_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexPackedFpCompareMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexPackedFpCompareMemoryReplay::Broadcast { .. } => {
            X86EvexE4MemoryReplayForm::Broadcast
        }
        X86EvexPackedFpCompareMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: false,
        vector_load_hint: Some(X86OpHint::VecAlign(X86VecAlign::Unaligned)),
        form,
    };
    let exact = exact_evex_e4_memory_sequence(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |op, memory_source| exact_compare(op, memory_source, encoding),
    )?;
    Some(X86JitEvexPackedFpCompareMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
