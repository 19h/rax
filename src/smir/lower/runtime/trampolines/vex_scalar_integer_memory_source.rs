//! Fail-closed helper-backed VEX `VMOVD`/`VMOVQ` memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{
    X86InstructionBytes, X86VexScalarIntegerMemoryEncoding, X86VexScalarIntegerMemoryKind,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact canonical decomposition consumed for one VEX.128 scalar-integer move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexScalarIntegerMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexScalarIntegerMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn is_single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&1)
        && virtual_uses.get(&register) == Some(&1)
}

fn element(width: MemWidth) -> Option<VecElementType> {
    match width {
        MemWidth::B4 => Some(VecElementType::I32),
        MemWidth::B8 => Some(VecElementType::I64),
        _ => None,
    }
}

/// Validate the exact canonical decomposition for a VEX.128 memory `VMOVD`
/// or `VMOVQ`.
///
/// Loads are the canonical four-op `Load; Mov(0); VBroadcast; VInsertLane`
/// graph that zeroes every destination bit above the scalar. Stores are the
/// canonical two-op `VExtractLane; Store` graph. Complete source-byte
/// provenance binds the alias, W/L/vvvv fields, vector operand, and transfer
/// width; canonical IR supplies an accepted architectural address shape.
/// Every intermediate must have one global definition and one global use.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space.
pub(crate) fn x86_jit_vex_scalar_integer_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexScalarIntegerMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == first.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, first.guest_pc))?;
    let encoding = instruction.vex_scalar_integer_memory_encoding()?;
    let destination = xmm(encoding.vector);
    let elem = element(encoding.memory_width)?;

    let (consumed, intermediates) = match encoding.kind {
        X86VexScalarIntegerMemoryKind::Load => {
            let loaded = match &first.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                } if *width == encoding.memory_width
                    && first.x86_hint.is_none()
                    && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };

            let zero_op = block.ops.get(index + 1)?;
            let zero = match &zero_op.kind {
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                } if zero_op.x86_hint.is_none() => *dst,
                _ => return None,
            };

            let clear = block.ops.get(index + 2)?;
            if !matches!(
                &clear.kind,
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem: clear_elem,
                    lanes: 1,
                } if *dst == destination && *scalar == zero && *clear_elem == elem
            ) || clear.x86_hint.is_some()
            {
                return None;
            }

            let insert = block.ops.get(index + 3)?;
            if !matches!(
                &insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: 0,
                    elem: insert_elem,
                } if *dst == destination
                    && *vec == destination
                    && *scalar == loaded
                    && *insert_elem == elem
            ) || insert.x86_hint.is_some()
            {
                return None;
            }

            if loaded == zero {
                return None;
            }
            (4, [Some(loaded), Some(zero)])
        }
        X86VexScalarIntegerMemoryKind::Store => {
            let extracted = match &first.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: 0,
                    elem: extract_elem,
                    sign: SignExtend::Zero,
                } if *vec == destination && *extract_elem == elem && first.x86_hint.is_none() => {
                    *dst
                }
                _ => return None,
            };

            let store = block.ops.get(index + 1)?;
            if !matches!(
                &store.kind,
                OpKind::Store {
                    src,
                    addr,
                    width,
                } if *src == extracted
                    && *width == encoding.memory_width
                    && x86_jit_mem_address_shape_valid(addr)
            ) || store.x86_hint.is_some()
            {
                return None;
            }
            (2, [Some(extracted), None])
        }
    };

    let end = index + consumed;
    if block.ops[index..end]
        .iter()
        .any(|op| op.guest_pc != first.guest_pc)
        || block
            .ops
            .get(end)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
        || !intermediates.into_iter().flatten().all(|register| {
            is_single_definition_single_use(register, virtual_definitions, virtual_uses)
        })
    {
        return None;
    }

    Some(X86JitVexScalarIntegerMemorySequence { consumed, encoding })
}

/// Return the exact length of any helper-backed VEX scalar-move memory graph.
///
/// This consolidates half-lane floating moves and scalar-integer moves at
/// their common dispatch points while retaining their independent exact
/// classifiers.
pub(crate) fn x86_jit_vex_scalar_move_memory_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    super::x86_jit_vex_half_move_memory_sequence(
        block,
        index,
        allow_mem,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )
    .map(|sequence| sequence.consumed)
    .or_else(|| {
        super::x86_jit_vex_half_move_store_sequence(
            block,
            index,
            allow_mem,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        )
        .map(|sequence| sequence.consumed)
    })
    .or_else(|| {
        x86_jit_vex_scalar_integer_memory_sequence(
            block,
            index,
            allow_mem,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        )
        .map(|sequence| sequence.consumed)
    })
}
