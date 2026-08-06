//! Fail-closed helper-backed EVEX integer-narrowing memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86NarrowMode, X86Reg,
};
use crate::smir::ir::{SmirBlock, X86EvexIntegerNarrowMemoryEncoding, X86InstructionBytes};

use super::evex_expand_memory_source::{
    exact_local_virtual_counts, exact_mask_condition, insert_fresh, memory_width,
};
use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, exact_lane_address,
    no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact VPMOV*/VPMOVS*/VPMOVUS* fixed-position scalar store sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexIntegerNarrowMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexIntegerNarrowMemoryEncoding,
}

fn source(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated integer-narrow source width"),
    }))
}

fn exact_truncate(
    block: &SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    dst_elem: VecElementType,
    owned: &mut HashSet<VReg>,
) -> Option<VReg> {
    let op = block.ops.get(index + *offset)?;
    let dst_bits = dst_elem.bytes() * 8;
    let expected_mask = (1u64 << dst_bits) - 1;
    let narrowed = match op.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(mask),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if op.x86_hint.is_none()
            && op.guest_pc == guest_pc
            && src1 == raw
            && mask == expected_mask as i64
            && insert_fresh(owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    *offset += 1;
    Some(narrowed)
}

#[allow(clippy::too_many_arguments)]
fn exact_saturate(
    block: &SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    src_elem: VecElementType,
    dst_elem: VecElementType,
    mode: X86NarrowMode,
    owned: &mut HashSet<VReg>,
) -> Option<VReg> {
    let zero_op = block.ops.get(index + *offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none()
            && zero_op.guest_pc == guest_pc
            && insert_fresh(owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    *offset += 1;

    let broadcast = block.ops.get(index + *offset)?;
    let wide = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if broadcast.x86_hint.is_none()
            && broadcast.guest_pc == guest_pc
            && scalar == zero
            && elem == src_elem
            && u32::from(lanes) == VecWidth::V128.lanes(src_elem)
            && insert_fresh(owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    *offset += 1;

    let insert = block.ops.get(index + *offset)?;
    if insert.x86_hint.is_some()
        || insert.guest_pc != guest_pc
        || !matches!(
            insert.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: 0,
                elem,
            } if dst == wide && vec == wide && scalar == raw && elem == src_elem
        )
    {
        return None;
    }
    *offset += 1;

    let narrow = block.ops.get(index + *offset)?;
    let packed = match narrow.kind {
        OpKind::X86NarrowInt {
            dst,
            src,
            mask: None,
            src_elem: actual_src,
            dst_elem: actual_dst,
            width,
            mode: actual_mode,
            zeroing: true,
        } if narrow.x86_hint.is_none()
            && narrow.guest_pc == guest_pc
            && src == wide
            && actual_src == src_elem
            && actual_dst == dst_elem
            && width
                == if src_elem == VecElementType::I64 {
                    VecWidth::V64
                } else {
                    VecWidth::V128
                }
            && actual_mode == mode
            && insert_fresh(owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    *offset += 1;

    let extract = block.ops.get(index + *offset)?;
    let narrowed = match extract.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem,
            sign: SignExtend::Zero,
        } if extract.x86_hint.is_none()
            && extract.guest_pc == guest_pc
            && vec == packed
            && elem == dst_elem
            && insert_fresh(owned, dst) =>
        {
            dst
        }
        _ => return None,
    };
    *offset += 1;
    Some(narrowed)
}

/// Validate the complete optimizer-stable scalar store decomposition emitted
/// for one Type-E6 integer-narrowing memory destination.
///
/// O0/O1 retain the lane-zero shift in a masked predicate; O2 may fold that
/// shift into the following AND. Every conversion op, virtual dataflow edge,
/// fixed destination position, source lane, helper width, APX address
/// frontier, and byte-provenance boundary is otherwise exact. Matching is
/// O(L) time and O(V) auxiliary space for at most 32 lanes and V local virtual
/// registers.
pub(crate) fn x86_jit_evex_integer_narrow_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexIntegerNarrowMemorySequence> {
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
        .evex_integer_narrow_memory_encoding()?;
    let expected_source = source(encoding.source, encoding.width);
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let lanes = encoding.width.lanes(encoding.src_elem) as u8;
    let expected_memory_width = memory_width(encoding.dst_elem)?;
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
        let raw = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Sign,
            } if extract.x86_hint.is_none()
                && extract.guest_pc == guest_pc
                && vec == expected_source
                && actual_lane == lane
                && elem == encoding.src_elem
                && insert_fresh(&mut owned, dst) =>
            {
                dst
            }
            _ => return None,
        };
        offset += 1;

        let narrowed = match encoding.mode {
            X86NarrowMode::Truncate => exact_truncate(
                block,
                index,
                &mut offset,
                guest_pc,
                raw,
                encoding.dst_elem,
                &mut owned,
            )?,
            X86NarrowMode::SignedSaturate | X86NarrowMode::UnsignedSaturate => exact_saturate(
                block,
                index,
                &mut offset,
                guest_pc,
                raw,
                encoding.src_elem,
                encoding.dst_elem,
                encoding.mode,
                &mut owned,
            )?,
        };

        let store = block.ops.get(index + offset)?;
        if store.x86_hint.is_some()
            || store.guest_pc != guest_pc
            || !matches!(
                &store.kind,
                OpKind::PredStore {
                    src: SrcOperand::Reg(src),
                    cond,
                    addr,
                    width,
                } if *src == narrowed
                    && *cond == condition
                    && exact_lane_address(
                        addr,
                        base,
                        i64::from(lane) * i64::from(encoding.dst_elem.bytes()),
                    )
                    && *width == expected_memory_width
            )
        {
            return None;
        }
        offset += 1;
    }

    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    let sequence = block.ops.get(index..index + offset)?;
    if !exact_local_virtual_counts(sequence, virtual_definitions, virtual_uses) {
        return None;
    }

    Some(X86JitEvexIntegerNarrowMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
