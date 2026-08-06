//! Fail-closed helper-backed EVEX duplicate-move memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexDuplicateMoveMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_vector_mask_result,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// duplicate-move memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexDuplicateMoveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexDuplicateMoveMemoryEncoding,
}

fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexDuplicateMoveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(VReg, usize)> {
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    if encoding.memory_size == 8 {
        let scalar = match &load.kind {
            OpKind::Load {
                dst,
                addr,
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            } if load.x86_hint.is_none()
                && encoding.width == VecWidth::V128
                && encoding.elem == VecElementType::F64
                && x86_jit_mem_address_shape_valid(addr) =>
            {
                *dst
            }
            _ => return None,
        };
        if !single_definition_single_use(scalar, virtual_definitions, virtual_uses) {
            return None;
        }
        let broadcast = block.ops.get(index + 1)?;
        let source = match broadcast.kind {
            OpKind::VBroadcast {
                dst,
                scalar: actual_scalar,
                elem: VecElementType::F64,
                lanes: 2,
            } if broadcast.guest_pc == guest_pc
                && broadcast.x86_hint.is_none()
                && actual_scalar == scalar =>
            {
                dst
            }
            _ => return None,
        };
        single_definition_single_use(source, virtual_definitions, virtual_uses)
            .then_some((source, 2))
    } else {
        let source = match &load.kind {
            OpKind::VLoad { dst, addr, width }
                if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                    && *width == encoding.width
                    && encoding.memory_size == encoding.width.bytes()
                    && x86_jit_mem_address_shape_valid(addr) =>
            {
                *dst
            }
            _ => return None,
        };
        single_definition_single_use(source, virtual_definitions, virtual_uses)
            .then_some((source, 1))
    }
}

fn encoded_vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX duplicate-move width"),
    }))
}

/// Match the exact selector graph emitted for one duplicate move and return
/// its raw virtual result. Runtime is O(L) and auxiliary space O(1), where L
/// is at most 16 architectural destination lanes.
#[allow(clippy::too_many_arguments)]
fn exact_duplicate_graph(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source: VReg,
    encoding: X86EvexDuplicateMoveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let zero_op = block.ops.get(index + *offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if zero_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(zero, 1, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let indices_op = block.ops.get(index + *offset)?;
    let indices = match indices_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if indices_op.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if indices_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            indices,
            usize::from(lanes) + 1,
            usize::from(lanes) + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    *offset += 1;

    for lane in 0..lanes {
        let selector = lane / 2 * 2 + u8::from(encoding.high);
        let selector_op = block.ops.get(index + *offset)?;
        let selector_reg = match selector_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(actual_selector),
                width: OpWidth::W64,
            } if selector_op.x86_hint.is_none() && actual_selector == i64::from(selector) => dst,
            _ => return None,
        };
        if selector_op.guest_pc != guest_pc
            || !single_definition_single_use(selector_reg, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let insert_op = block.ops.get(index + *offset)?;
        if insert_op.guest_pc != guest_pc
            || insert_op.x86_hint.is_some()
            || !matches!(
                insert_op.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: actual_lane,
                    elem,
                } if dst == indices
                    && vec == indices
                    && scalar == selector_reg
                    && actual_lane == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        *offset += 1;
    }

    let shuffle = block.ops.get(index + *offset)?;
    let raw = match shuffle.kind {
        OpKind::VShuffle {
            dst,
            src1,
            src2: None,
            indices: actual_indices,
            elem,
            lanes: actual_lanes,
        } if shuffle.x86_hint.is_none()
            && src1 == source
            && actual_indices == indices
            && elem == encoding.elem
            && actual_lanes == lanes
            && matches!(dst, VReg::Virtual(_)) =>
        {
            dst
        }
        _ => return None,
    };
    if shuffle.guest_pc != guest_pc {
        return None;
    }
    *offset += 1;
    Some(raw)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VMOVSLDUP/VMOVSHDUP/VMOVDDUP memory source.
///
/// Exact byte provenance binds fixed W/pp, vector and element widths,
/// destination, mask policy, one unconditional E4NF/E5NF tuple read, every
/// generated selector, APX address ownership, and the terminal guest-PC
/// frontier. Runtime is O(L) and auxiliary space O(1) for L <= 16 lanes;
/// callers construct definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_duplicate_move_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexDuplicateMoveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_duplicate_move_memory_encoding()?;
    let (source, mut offset) =
        exact_memory_source(block, index, encoding, virtual_definitions, virtual_uses)?;
    let address = exact_evex_memory_sequence_address(block, index, 0)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    let raw = exact_duplicate_graph(
        block,
        index,
        &mut offset,
        guest_pc,
        source,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if encoding.zeroing
            || !exact_virtual_definition_use(raw, 1, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.guest_pc != guest_pc
            || commit.x86_hint.is_some()
            || !matches!(
                commit.kind,
                OpKind::VMov { dst, src, width }
                    if vector_index(&dst, encoding.width) == Some(encoding.destination)
                        && src == raw
                        && width == encoding.width
            )
        {
            return None;
        }
        offset += 1;
    }
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexDuplicateMoveMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
