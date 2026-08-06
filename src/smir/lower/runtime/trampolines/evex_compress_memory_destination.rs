//! Fail-closed helper-backed EVEX packed compress memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, GuestAddr, SignExtend, SrcOperand, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{SmirBlock, X86EvexCompressMemoryEncoding, X86InstructionBytes};

use super::evex_expand_memory_source::{
    exact_count_update, exact_local_virtual_counts, exact_mask_condition, insert_fresh,
    memory_width,
};
use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact VCOMPRESS*/VPCOMPRESS* dense memory-write sequence consumed by x86-64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexCompressMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexCompressMemoryEncoding,
}

fn source(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated packed compress width"),
    }))
}

/// Validate the complete optimizer-stable scalar store decomposition emitted
/// for one Type-E4 packed compress memory destination.
///
/// The matcher accepts the O0 terminal count update and the O1/O2 form where
/// that dead pair is removed; O2 may also remove the lane-zero shift. Every
/// other operation, virtual value, source lane, address, memory width, and
/// byte-provenance boundary is exact. Matching is O(L) time and O(V)
/// auxiliary space for at most 64 lanes and the V virtual registers in this
/// one graph.
pub(crate) fn x86_jit_evex_compress_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexCompressMemorySequence> {
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
        .evex_compress_memory_encoding()?;
    let expected_source = source(encoding.source, encoding.width);
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let expected_memory_width = memory_width(encoding.elem)?;
    let mut owned = HashSet::new();
    let mut offset = 0usize;

    let (base, address) = match &first.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if first.x86_hint.is_none()
            && x86_jit_mem_address_shape_valid(addr)
            && insert_fresh(&mut owned, *base) =>
        {
            (*base, addr)
        }
        _ => return None,
    };
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }
    offset += 1;

    let count_op = block.ops.get(index + offset)?;
    let mut count = match count_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: crate::smir::ir::types::OpWidth::W64,
        } if count_op.x86_hint.is_none()
            && count_op.guest_pc == guest_pc
            && insert_fresh(&mut owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    offset += 1;

    for lane in 0..lanes {
        let condition = exact_mask_condition(
            block,
            index,
            &mut offset,
            guest_pc,
            expected_mask,
            lane,
            &mut owned,
        )?;
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && extract.guest_pc == guest_pc
                && vec == expected_source
                && actual_lane == lane
                && elem == encoding.elem
                && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;
        let store = block.ops.get(index + offset)?;
        if !matches!(
            &store.kind,
            OpKind::PredStore {
                src: SrcOperand::Reg(actual_scalar),
                cond,
                addr: Address::BaseIndexScale {
                    base: Some(actual_base),
                    index: actual_index,
                    scale,
                    disp: 0,
                    disp_size: DispSize::Auto,
                },
                width,
            } if *actual_scalar == scalar
                && *cond == condition
                && *actual_base == base
                && *actual_index == count
                && *scale == encoding.elem.bytes() as u8
                && *width == expected_memory_width
        ) || store.x86_hint.is_some()
            || store.guest_pc != guest_pc
        {
            return None;
        }
        offset += 1;

        if lane + 1 < lanes {
            count = exact_count_update(
                block,
                index,
                &mut offset,
                guest_pc,
                count,
                condition,
                &mut owned,
            )?;
        } else {
            let saved_offset = offset;
            let mut saved_owned = owned.clone();
            if exact_count_update(
                block,
                index,
                &mut offset,
                guest_pc,
                count,
                condition,
                &mut saved_owned,
            )
            .is_some()
            {
                owned = saved_owned;
            } else {
                offset = saved_offset;
            }
        }
    }

    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    let sequence = block.ops.get(index..index + offset)?;
    if !exact_local_virtual_counts(sequence, virtual_definitions, virtual_uses) {
        return None;
    }

    Some(X86JitEvexCompressMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
