//! Fail-closed helper-backed EVEX VPSHUFD/VPSHUFHW/VPSHUFLW memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexLaneShuffleMemoryEncoding, X86EvexLaneShuffleMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_vector_mask_result,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// packed lane-shuffle memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexLaneShuffleMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexLaneShuffleMemoryEncoding,
}

fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexLaneShuffleMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(VReg, usize)> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    match encoding.replay {
        X86EvexLaneShuffleMemoryReplay::Vector { .. } => {
            let load = block.ops.get(index)?;
            let loaded = match &load.kind {
                OpKind::VLoad { dst, addr, width }
                    if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                        && *width == encoding.width
                        && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            single_definition_single_use(loaded, virtual_definitions, virtual_uses)
                .then_some((loaded, 1))
        }
        X86EvexLaneShuffleMemoryReplay::Broadcast { memory_width, .. } => {
            let load = block.ops.get(index)?;
            let scalar = match &load.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                } if load.x86_hint.is_none()
                    && *width == memory_width
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
            let loaded = match broadcast.kind {
                OpKind::VBroadcast {
                    dst,
                    scalar: actual_scalar,
                    elem: VecElementType::I32,
                    lanes,
                } if broadcast.x86_hint.is_none()
                    && actual_scalar == scalar
                    && u32::from(lanes) == encoding.width.lanes(VecElementType::I32) =>
                {
                    dst
                }
                _ => return None,
            };
            if broadcast.guest_pc != guest_pc
                || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
            {
                return None;
            }
            Some((loaded, 2))
        }
    }
}

fn encoded_vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX packed lane-shuffle width"),
    }))
}

/// Match the exact selector-vector graph emitted for one packed immediate
/// lane shuffle. Advances `offset` past the zero seed, every lane selector,
/// and the final `VShuffle`, and returns its raw virtual result.
///
/// Runtime is O(L) and auxiliary space is O(1), where L is at most 32 I16
/// lanes. Global definition/use maps bind every internal virtual to this graph.
#[allow(clippy::too_many_arguments)]
fn exact_packed_shuffle_imm_graph(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source: VReg,
    width: VecWidth,
    elem: VecElementType,
    high_words: Option<bool>,
    immediate: u8,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let lanes = width.lanes(elem) as u8;
    let block_lanes = match elem {
        VecElementType::I32 if high_words.is_none() => 4,
        VecElementType::I16 if high_words.is_some() => 8,
        _ => return None,
    };

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
            elem: actual_elem,
            lanes: actual_lanes,
        } if indices_op.x86_hint.is_none()
            && scalar == zero
            && actual_elem == elem
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
        let within = lane % block_lanes;
        let lane_block = lane - within;
        let shuffled = match high_words {
            None => true,
            Some(true) => within >= 4,
            Some(false) => within < 4,
        };
        let selector = if shuffled {
            let output = within % 4;
            lane_block
                + if high_words == Some(true) { 4 } else { 0 }
                + ((immediate >> (output * 2)) & 3)
        } else {
            lane
        };

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
                    elem: actual_elem,
                } if dst == indices
                    && vec == indices
                    && scalar == selector_reg
                    && actual_lane == lane
                    && actual_elem == elem
            )
        {
            return None;
        }
        *offset += 1;
    }

    let shuffle_op = block.ops.get(index + *offset)?;
    let raw = match shuffle_op.kind {
        OpKind::VShuffle {
            dst,
            src1: actual_source,
            src2: None,
            indices: actual_indices,
            elem: actual_elem,
            lanes: actual_lanes,
        } if shuffle_op.x86_hint.is_none()
            && actual_source == source
            && actual_indices == indices
            && actual_elem == elem
            && actual_lanes == lanes
            && matches!(dst, VReg::Virtual(_)) =>
        {
            dst
        }
        _ => return None,
    };
    if shuffle_op.guest_pc != guest_pc {
        return None;
    }
    *offset += 1;
    Some(raw)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VPSHUFD/VPSHUFHW/VPSHUFLW memory source.
///
/// Exact provenance validates W/pp and retains the architecturally ignored W
/// bit for word forms. It binds vector and element widths, destination, imm8,
/// every generated selector, destination mask policy, one unconditional E4NF
/// tuple read, the APX address guard, and the guest-PC frontier. Runtime is
/// O(L) and auxiliary space is O(1) for L <= 32 lanes; callers construct
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_lane_shuffle_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexLaneShuffleMemorySequence> {
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
        .evex_lane_shuffle_memory_encoding()?;
    let (loaded, mut offset) =
        exact_memory_source(block, index, encoding, virtual_definitions, virtual_uses)?;
    let address = exact_evex_memory_sequence_address(block, index, 0)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    let elem = encoding.kind.element();
    let raw = exact_packed_shuffle_imm_graph(
        block,
        index,
        &mut offset,
        guest_pc,
        loaded,
        encoding.width,
        elem,
        encoding.kind.high_words(),
        encoding.immediate,
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
            elem,
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
    Some(X86JitEvexLaneShuffleMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
