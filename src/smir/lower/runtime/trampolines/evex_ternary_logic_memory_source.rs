//! Fail-closed helper-backed EVEX VPTERNLOGD/Q memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, X86Reg};
use crate::smir::ir::{
    X86EvexTernaryLogicMemoryEncoding, X86EvexTernaryLogicMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// VPTERNLOGD/Q memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexTernaryLogicMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexTernaryLogicMemoryEncoding,
}

fn exact_ternary_logic(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexTernaryLogicMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    matches!(
        op.kind,
        OpKind::X86TernaryLogic {
            dst,
            src1,
            src2,
            src3,
            mask,
            imm,
            width,
            elem,
            zeroing,
        } if op.x86_hint.is_none()
            && vector_index(&dst, encoding.width) == Some(encoding.destination)
            && src1 == dst
            && vector_index(&src2, encoding.width) == Some(encoding.source2)
            && src3 == memory_source
            && mask == expected_mask
            && imm == encoding.immediate
            && width == encoding.width
            && elem == encoding.elem
            && zeroing == encoding.zeroing
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one
/// VPTERNLOGD/Q memory source.
///
/// Exact provenance binds the instruction family, element and vector widths,
/// all architectural operands, imm8 truth table, writemask policy,
/// broadcast/full-vector tuple, helper address, and sole destination commit.
pub(crate) fn x86_jit_evex_ternary_logic_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexTernaryLogicMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_ternary_logic_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexTernaryLogicMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexTernaryLogicMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexTernaryLogicMemoryReplay::MaskedVector { .. } => {
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
        |op, memory_source| exact_ternary_logic(op, memory_source, encoding),
    )?;
    Some(X86JitEvexTernaryLogicMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
