//! Fail-closed helper-backed EVEX packed-integer comparison/test admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SrcOperand, VReg, VecCmpCond, X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedIntegerMaskMemoryEncoding, X86EvexPackedIntegerMaskMemoryReplay,
    X86EvexPackedIntegerMaskOperation, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence_tail,
    exact_virtual_definition_use, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed packed EVEX
/// integer comparison/test memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedIntegerMaskMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedIntegerMaskMemoryEncoding,
}

fn exact_mask_commit(
    op: &crate::smir::ir::ops::SmirOp,
    raw_mask: VReg,
    encoding: X86EvexPackedIntegerMaskMemoryEncoding,
) -> bool {
    let destination = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.destination)));
    if op.x86_hint.is_some() {
        return false;
    }
    match encoding.writemask {
        Some(mask) => matches!(
            op.kind,
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Reg(actual_mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == destination
                && src1 == raw_mask
                && actual_mask == VReg::Arch(ArchReg::X86(X86Reg::K(mask)))
        ),
        None => matches!(
            op.kind,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W64,
            } if dst == destination && src == raw_mask
        ),
    }
}

fn exact_mov_mask(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    compared: VReg,
    encoding: X86EvexPackedIntegerMaskMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let mov_mask = block.ops.get(index)?;
    let raw_mask = match mov_mask.kind {
        OpKind::X86MovMask {
            dst,
            src,
            elem,
            lanes,
            dst_width: OpWidth::W64,
        } if mov_mask.x86_hint.is_none()
            && src == compared
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(raw_mask, 1, 1, virtual_definitions, virtual_uses)
        || !exact_mask_commit(block.ops.get(index + 1)?, raw_mask, encoding)
    {
        return None;
    }
    Some(2)
}

fn exact_compare_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    memory_source: VReg,
    encoding: X86EvexPackedIntegerMaskMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let X86EvexPackedIntegerMaskOperation::Compare {
        condition,
        constant,
        ..
    } = encoding.operation
    else {
        return None;
    };
    let architectural_source1 = VReg::Arch(ArchReg::X86(match encoding.width {
        crate::smir::ir::types::VecWidth::V128 => X86Reg::Xmm(encoding.source1),
        crate::smir::ir::types::VecWidth::V256 => X86Reg::Ymm(encoding.source1),
        crate::smir::ir::types::VecWidth::V512 => X86Reg::Zmm(encoding.source1),
        crate::smir::ir::types::VecWidth::V64 => return None,
    }));
    let (expected_source1, expected_condition) = match (condition, constant) {
        (Some(condition), None) => (architectural_source1, condition),
        (None, Some(constant)) => (
            memory_source,
            if constant {
                VecCmpCond::Eq
            } else {
                VecCmpCond::Ne
            },
        ),
        _ => return None,
    };
    let compare = block.ops.get(index)?;
    let compared = match compare.kind {
        OpKind::VCmp {
            dst,
            src1,
            src2,
            cond,
            elem,
            lanes,
        } if compare.x86_hint.is_none()
            && src1 == expected_source1
            && src2 == memory_source
            && cond == expected_condition
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(compared, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    Some(
        1 + exact_mov_mask(
            block,
            index + 1,
            compared,
            encoding,
            virtual_definitions,
            virtual_uses,
        )?,
    )
}

fn exact_test_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    memory_source: VReg,
    encoding: X86EvexPackedIntegerMaskMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let X86EvexPackedIntegerMaskOperation::Test { inverted } = encoding.operation else {
        return None;
    };
    let and_op = block.ops.get(index)?;
    let anded = match and_op.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        } if and_op.x86_hint.is_none()
            && vector_index(&src1, encoding.width) == Some(encoding.source1)
            && src2 == memory_source
            && width == encoding.width =>
        {
            dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(anded, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let zero_op = block.ops.get(index + 1)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !exact_virtual_definition_use(zero, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 2)?;
    let zero_vector = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if broadcast.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(zero_vector, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let compare = block.ops.get(index + 3)?;
    let compared = match compare.kind {
        OpKind::VCmp {
            dst,
            src1,
            src2,
            cond,
            elem,
            lanes,
        } if compare.x86_hint.is_none()
            && src1 == anded
            && src2 == zero_vector
            && cond
                == if inverted {
                    VecCmpCond::Eq
                } else {
                    VecCmpCond::Ne
                }
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(compared, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    Some(
        4 + exact_mov_mask(
            block,
            index + 4,
            compared,
            encoding,
            virtual_definitions,
            virtual_uses,
        )?,
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX packed
/// integer comparison/test memory source.
///
/// Exact provenance binds the E4/E4.nb tuple, vector width and element type,
/// source vector, K destination and writemask, compare predicate/test polarity,
/// helper address, fault-suppression graph, sign-bit reduction, and sole
/// K-register commit. Classification is O(L) time and O(1) auxiliary space for
/// L <= 64 lanes; callers build definition/use maps once in O(N) time and O(V)
/// space.
pub(crate) fn x86_jit_evex_packed_integer_mask_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedIntegerMaskMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_integer_mask_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexPackedIntegerMaskMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexPackedIntegerMaskMemoryReplay::Broadcast { .. } => {
            X86EvexE4MemoryReplayForm::Broadcast
        }
        X86EvexPackedIntegerMaskMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let memory_source_uses = usize::from(matches!(
        encoding.operation,
        X86EvexPackedIntegerMaskOperation::Compare {
            condition: None,
            constant: Some(_),
            ..
        }
    )) + 1;
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: false,
        vector_load_hint: None,
        form,
        memory_source_uses,
    };
    let exact = exact_evex_e4_memory_sequence_tail(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |block, tail_index, memory_source| match encoding.operation {
            X86EvexPackedIntegerMaskOperation::Compare { .. } => exact_compare_tail(
                block,
                tail_index,
                memory_source,
                encoding,
                virtual_definitions,
                virtual_uses,
            ),
            X86EvexPackedIntegerMaskOperation::Test { .. } => exact_test_tail(
                block,
                tail_index,
                memory_source,
                encoding,
                virtual_definitions,
                virtual_uses,
            ),
        },
    )?;
    Some(X86JitEvexPackedIntegerMaskMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
