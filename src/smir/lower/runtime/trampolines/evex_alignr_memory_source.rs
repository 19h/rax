//! Fail-closed helper-backed EVEX `VPALIGNR` memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SrcOperand, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{X86EvexAlignrMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, exact_virtual_definition_use, single_definition_single_use,
    vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous EVEX `VPALIGNR` Full Mem decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexAlignrMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexAlignrMemoryEncoding,
}

fn no_following_same_pc(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    consumed: usize,
    guest_pc: GuestAddr,
) -> bool {
    !block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == guest_pc)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// `VPALIGNR` Full Mem source.
///
/// Exact provenance binds WIG, vector width, immediate, destination, high
/// source, byte writemask, and the register-source rewrite. The matcher
/// validates every per-128-bit selector, all virtual definition/use counts,
/// the unconditional E4NF.nb vector load, the complete merge/zero tail, and
/// the guest-PC frontier. Classification is O(L) time and O(1) auxiliary space
/// for L <= 64 byte lanes; callers build definition/use maps once in O(N) time
/// and O(V) space.
pub(crate) fn x86_jit_evex_alignr_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexAlignrMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_alignr_memory_encoding()?;
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
    if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
        return None;
    }

    let lanes = encoding.width.lanes(VecElementType::I8) as u8;
    let mut offset = 1usize;
    let zero_op = block.ops.get(index + offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if zero_op.guest_pc != guest_pc
        || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let indices_op = block.ops.get(index + offset)?;
    let indices = match indices_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I8,
            lanes: actual_lanes,
        } if indices_op.x86_hint.is_none() && scalar == zero && actual_lanes == lanes => dst,
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
    offset += 1;

    for lane in 0..lanes {
        let block_base = lane / 16 * 16;
        let in_block = lane % 16;
        let concatenated = u16::from(encoding.immediate) + u16::from(in_block);
        let selector = if concatenated < 16 {
            u16::from(block_base) + concatenated
        } else if concatenated < 32 {
            u16::from(lanes) + u16::from(block_base) + concatenated - 16
        } else {
            u16::from(lanes) * 2
        };
        let selector_op = block.ops.get(index + offset)?;
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
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || insert.guest_pc != guest_pc
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: actual_lane,
                    elem: VecElementType::I8,
                } if dst == indices
                    && vec == indices
                    && scalar == selector_reg
                    && actual_lane == lane
            )
        {
            return None;
        }
        offset += 1;
    }

    let shuffle = block.ops.get(index + offset)?;
    let raw = match shuffle.kind {
        OpKind::VShuffle {
            dst,
            src1,
            src2: Some(src2),
            indices: actual_indices,
            elem: VecElementType::I8,
            lanes: actual_lanes,
        } if shuffle.x86_hint.is_none()
            && src1 == loaded
            && vector_index(&src2, encoding.width) == Some(encoding.high)
            && actual_indices == indices
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if shuffle.guest_pc != guest_pc || !matches!(raw, VReg::Virtual(_)) {
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
            VecElementType::I8,
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
        if commit.x86_hint.is_some()
            || commit.guest_pc != guest_pc
            || !matches!(
                commit.kind,
                OpKind::VMov {
                    dst,
                    src,
                    width,
                } if vector_index(&dst, encoding.width) == Some(encoding.destination)
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
    Some(X86JitEvexAlignrMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}
