//! Fail-closed helper-backed EVEX scalar floating-point comparison admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecElementType, VecWidth, X86Reg};
use crate::smir::ir::{X86EvexScalarFpCompareMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence, vector_index,
};

/// Exact contiguous EVEX scalar comparison memory decomposition consumed by
/// the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarFpCompareMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) load_offset: usize,
    pub(crate) encoding: X86EvexScalarFpCompareMemoryEncoding,
}

fn exact_compare(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexScalarFpCompareMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let (map, prefix, w) = match encoding.elem {
        VecElementType::F16 => (X86VecMap::Map0F3A, X86SsePrefix::Rep, false),
        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::Rep, false),
        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::Repne, true),
        _ => return false,
    };
    let hint_width = match encoding.ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
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
            width: VecWidth::V128,
            lanes: 1,
            predicate,
            scalar: true,
            mask_destination: true,
            zero_upper: false,
            suppress_exceptions: false,
        } if destination == encoding.destination
            && vector_index(&src1, VecWidth::V128) == Some(encoding.source1)
            && src2 == memory_source
            && mask == expected_mask
            && elem == encoding.elem
            && predicate == encoding.predicate
            && op.x86_hint == Some(X86OpHint::EvexOp {
                map,
                pp: prefix,
                opcode: 0xC2,
                width: hint_width,
                w,
            })
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX scalar
/// floating-point comparison memory source.
///
/// Exact byte provenance binds precision, source vector, K destination and
/// writemask, five-bit predicate, LLIG provenance image, dynamic MXCSR behavior, helper
/// address, Type E3 fault suppression, APX address guard, and sole K-register
/// commit. Classification is O(1) time and auxiliary space; callers build
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_scalar_fp_compare_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpCompareMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_scalar_fp_compare_memory_encoding()?;
    let shape = X86EvexE4MemoryShape {
        width: VecWidth::V128,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: false,
        vector_load_hint: None,
        form: X86EvexE4MemoryReplayForm::Scalar,
        memory_source_uses: 1,
    };
    let exact = exact_evex_e4_memory_sequence(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |op, memory_source| exact_compare(op, memory_source, encoding),
    )?;
    Some(X86JitEvexScalarFpCompareMemorySequence {
        consumed: exact.consumed,
        load_offset: exact.address_offset,
        encoding,
    })
}
