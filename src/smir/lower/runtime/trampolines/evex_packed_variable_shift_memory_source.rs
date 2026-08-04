//! Fail-closed helper-backed EVEX per-element variable-shift memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, X86Reg};
use crate::smir::ir::{
    X86EvexPackedVariableShiftMemoryEncoding, X86EvexPackedVariableShiftMemoryReplay,
    X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// packed per-element variable-shift memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedVariableShiftMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedVariableShiftMemoryEncoding,
}

fn exact_variable_shift(
    op: &crate::smir::ir::ops::SmirOp,
    memory_count: VReg,
    encoding: X86EvexPackedVariableShiftMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    matches!(
        op.kind,
        OpKind::X86PackedShiftVariable {
            dst,
            src,
            count,
            mask,
            width,
            elem,
            shift,
            zeroing,
        } if op.x86_hint.is_none()
            && vector_index(&dst, encoding.width) == Some(encoding.destination)
            && vector_index(&src, encoding.width) == Some(encoding.source)
            && count == memory_count
            && mask == expected_mask
            && width == encoding.width
            && elem == encoding.elem
            && shift == encoding.shift
            && zeroing == encoding.zeroing
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed
/// AVX-512 per-element variable-shift memory source.
///
/// Exact provenance binds the instruction family, direction, element and
/// vector widths, architectural operands, writemask policy,
/// broadcast/full-vector tuple, helper address, and the sole architectural
/// destination commit.
pub(crate) fn x86_jit_evex_packed_variable_shift_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedVariableShiftMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_variable_shift_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexPackedVariableShiftMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexPackedVariableShiftMemoryReplay::Broadcast { .. } => {
            X86EvexE4MemoryReplayForm::Broadcast
        }
        X86EvexPackedVariableShiftMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
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
        |op, memory_count| exact_variable_shift(op, memory_count, encoding),
    )?;
    Some(X86JitEvexPackedVariableShiftMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
