//! Fail-closed helper-backed EVEX one-table full-permute memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexFullPermuteControl, X86EvexFullPermuteMemoryEncoding, X86EvexFullPermuteMemoryReplay,
    X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_vector_mask_result,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed EVEX
/// VPERM*/VPERMIL* one-table memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexFullPermuteMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexFullPermuteMemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX full-permute width"),
    }))
}

#[allow(clippy::too_many_arguments)]
fn exact_immediate_indices(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    encoding: X86EvexFullPermuteMemoryEncoding,
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
        let selector_op = block.ops.get(index + *offset)?;
        let selector = match selector_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(actual),
                width: OpWidth::W64,
            } if selector_op.x86_hint.is_none()
                && Some(actual) == encoding.control.source_lane(lane).map(i64::from) =>
            {
                dst
            }
            _ => return None,
        };
        if selector_op.guest_pc != guest_pc
            || !single_definition_single_use(selector, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let insert = block.ops.get(index + *offset)?;
        if insert.guest_pc != guest_pc
            || insert.x86_hint.is_some()
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: actual_lane,
                    elem,
                } if dst == indices
                    && vec == indices
                    && scalar == selector
                    && actual_lane == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        *offset += 1;
    }
    Some(indices)
}

fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    encoding: X86EvexFullPermuteMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(VReg, usize)> {
    let address_offset = *offset;
    match encoding.replay {
        X86EvexFullPermuteMemoryReplay::Vector { .. } => {
            let load = block.ops.get(index + *offset)?;
            let loaded = match &load.kind {
                OpKind::VLoad { dst, addr, width }
                    if matches!(
                        load.x86_hint,
                        None | Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                    ) && *width == encoding.width
                        && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            if load.guest_pc != guest_pc
                || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
            {
                return None;
            }
            *offset += 1;
            Some((loaded, address_offset))
        }
        X86EvexFullPermuteMemoryReplay::Broadcast { memory_width, .. } => {
            let load = block.ops.get(index + *offset)?;
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
            if load.guest_pc != guest_pc
                || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
            {
                return None;
            }
            *offset += 1;
            let broadcast = block.ops.get(index + *offset)?;
            let loaded = match broadcast.kind {
                OpKind::VBroadcast {
                    dst,
                    scalar: actual_scalar,
                    elem,
                    lanes,
                } if broadcast.x86_hint.is_none()
                    && actual_scalar == scalar
                    && elem == encoding.elem
                    && lanes == encoding.width.lanes(encoding.elem) as u8 =>
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
            *offset += 1;
            Some((loaded, address_offset))
        }
    }
}

/// Validate the complete O0/O1/O2 decomposition for one Type-E4NF EVEX
/// one-table permutation with a memory source.
///
/// Provenance binds the opcode/control class, all vector operands, immediate,
/// tuple shape, writemask policy, APX frontier, and exact sole destination
/// commit. Classification is O(L) time and O(1) auxiliary space for at most
/// 64 lanes; definition/use maps are built once by the caller in O(N) time
/// and O(V) space.
pub(crate) fn x86_jit_evex_full_permute_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFullPermuteMemorySequence> {
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
        .evex_full_permute_memory_encoding()?;
    let mut offset = 0usize;
    let indices = match encoding.control {
        X86EvexFullPermuteControl::Variable { indices } => vector(indices, encoding.width),
        X86EvexFullPermuteControl::Immediate { .. } => exact_immediate_indices(
            block,
            index,
            &mut offset,
            guest_pc,
            encoding,
            virtual_definitions,
            virtual_uses,
        )?,
    };
    let (loaded, address_offset) = exact_memory_source(
        block,
        index,
        &mut offset,
        guest_pc,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    let permute = block.ops.get(index + offset)?;
    let raw = match permute.kind {
        OpKind::VPermute {
            dst,
            src1,
            src2: None,
            indices: actual_indices,
            elem,
            width,
            overwrite_table: false,
        } if permute.x86_hint.is_none()
            && src1 == loaded
            && actual_indices == indices
            && elem == encoding.elem
            && width == encoding.width
            && matches!(dst, VReg::Virtual(_)) =>
        {
            dst
        }
        _ => return None,
    };
    if permute.guest_pc != guest_pc {
        return None;
    }
    offset += 1;

    if let Some(mask_index) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask_index))),
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if encoding.zeroing || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
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
    let address = exact_evex_memory_sequence_address(block, index, address_offset)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }
    Some(X86JitEvexFullPermuteMemorySequence {
        consumed: offset,
        address_offset,
        encoding,
    })
}
