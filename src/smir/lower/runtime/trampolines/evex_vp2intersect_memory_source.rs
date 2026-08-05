//! Fail-closed helper-backed EVEX `VP2INTERSECTD/Q` memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecCmpCond, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexVp2IntersectMemoryEncoding, X86EvexVp2IntersectMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, no_following_same_pc,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// `VP2INTERSECTD/Q` memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexVp2IntersectMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexVp2IntersectMemoryEncoding,
}

fn encoded_vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated VP2INTERSECT width"),
    }))
}

fn exact_op(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: usize,
    guest_pc: GuestAddr,
) -> Option<&SmirOp> {
    let op = block.ops.get(index + offset)?;
    (op.guest_pc == guest_pc && op.x86_hint.is_none()).then_some(op)
}

fn fresh_virtual(register: VReg, fresh: &mut HashSet<VReg>) -> bool {
    matches!(register, VReg::Virtual(_)) && fresh.insert(register)
}

fn exact_zero(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    let op = exact_op(block, index, *offset, guest_pc)?;
    let zero = match op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => dst,
        _ => return None,
    };
    if !fresh_virtual(zero, fresh) {
        return None;
    }
    *offset += 1;
    Some(zero)
}

#[allow(clippy::too_many_arguments)]
fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    encoding: X86EvexVp2IntersectMemoryEncoding,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    match encoding.replay {
        X86EvexVp2IntersectMemoryReplay::Vector { .. } => {
            let load = exact_op(block, index, *offset, guest_pc)?;
            let loaded = match &load.kind {
                OpKind::VLoad { dst, addr, width }
                    if *width == encoding.width && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            if !fresh_virtual(loaded, fresh) {
                return None;
            }
            *offset += 1;
            Some(loaded)
        }
        X86EvexVp2IntersectMemoryReplay::Broadcast { memory_width, .. } => {
            let load = exact_op(block, index, *offset, guest_pc)?;
            let scalar = match &load.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                } if *width == memory_width && x86_jit_mem_address_shape_valid(addr) => *dst,
                _ => return None,
            };
            if !fresh_virtual(scalar, fresh) {
                return None;
            }
            *offset += 1;

            let broadcast = exact_op(block, index, *offset, guest_pc)?;
            let source = match broadcast.kind {
                OpKind::VBroadcast {
                    dst,
                    scalar: actual_scalar,
                    elem,
                    lanes,
                } if actual_scalar == scalar
                    && elem == encoding.elem
                    && lanes == encoding.width.lanes(encoding.elem) as u8 =>
                {
                    dst
                }
                _ => return None,
            };
            if !fresh_virtual(source, fresh) {
                return None;
            }
            *offset += 1;
            Some(source)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn exact_movemask(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    compared: VReg,
    elem: VecElementType,
    lanes: u8,
    fresh: &mut HashSet<VReg>,
) -> Option<VReg> {
    let accumulated = exact_zero(block, index, offset, guest_pc, fresh)?;
    for lane in 0..lanes {
        let extract = exact_op(block, index, *offset, guest_pc)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem: actual_elem,
                sign: SignExtend::Zero,
            } if vec == compared && actual_lane == lane && actual_elem == elem => dst,
            _ => return None,
        };
        if !fresh_virtual(scalar, fresh) {
            return None;
        }
        *offset += 1;

        let shift = exact_op(block, index, *offset, guest_pc)?;
        let sign = match shift.kind {
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Imm(amount),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if src == scalar && amount == i64::from(elem.bytes() * 8 - 1) => dst,
            _ => return None,
        };
        if !fresh_virtual(sign, fresh) {
            return None;
        }
        *offset += 1;

        let positioned = if lane == 0 {
            sign
        } else {
            let shift = exact_op(block, index, *offset, guest_pc)?;
            let shifted = match shift.kind {
                OpKind::Shl {
                    dst,
                    src,
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if src == sign && amount == i64::from(lane) => dst,
                _ => return None,
            };
            if !fresh_virtual(shifted, fresh) {
                return None;
            }
            *offset += 1;
            shifted
        };

        let combine = exact_op(block, index, *offset, guest_pc)?;
        if !matches!(
            combine.kind,
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == accumulated && src1 == accumulated && src2 == positioned
        ) {
            return None;
        }
        *offset += 1;
    }

    // O0 retains append_sse_movmask's final copy; O1/O2 propagate it into
    // both consumers. No other graph variation is admitted.
    let Some(candidate) = block.ops.get(index + *offset) else {
        return Some(accumulated);
    };
    let matches = match candidate.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } if candidate.guest_pc == guest_pc
            && candidate.x86_hint.is_none()
            && src == accumulated =>
        {
            if !fresh_virtual(dst, fresh) {
                return None;
            }
            *offset += 1;
            dst
        }
        _ => accumulated,
    };
    Some(matches)
}

#[allow(clippy::too_many_arguments)]
fn exact_intersection_graph(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    source2: VReg,
    encoding: X86EvexVp2IntersectMemoryEncoding,
    fresh: &mut HashSet<VReg>,
) -> Option<()> {
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let source1 = encoded_vector(encoding.source1, encoding.width);
    let mask1 = exact_zero(block, index, offset, guest_pc, fresh)?;
    let mask2 = exact_zero(block, index, offset, guest_pc, fresh)?;
    let zero = exact_zero(block, index, offset, guest_pc, fresh)?;

    for lane in 0..lanes {
        let extract = exact_op(block, index, *offset, guest_pc)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if vec == source1 && actual_lane == lane && elem == encoding.elem => dst,
            _ => return None,
        };
        if !fresh_virtual(scalar, fresh) {
            return None;
        }
        *offset += 1;

        let broadcast = exact_op(block, index, *offset, guest_pc)?;
        let splat = match broadcast.kind {
            OpKind::VBroadcast {
                dst,
                scalar: actual_scalar,
                elem,
                lanes: actual_lanes,
            } if actual_scalar == scalar && elem == encoding.elem && actual_lanes == lanes => dst,
            _ => return None,
        };
        if !fresh_virtual(splat, fresh) {
            return None;
        }
        *offset += 1;

        let compare = exact_op(block, index, *offset, guest_pc)?;
        let compared = match compare.kind {
            OpKind::VCmp {
                dst,
                src1: actual_source1,
                src2: actual_source2,
                cond: VecCmpCond::Eq,
                elem,
                lanes: actual_lanes,
            } if actual_source1 == splat
                && actual_source2 == source2
                && elem == encoding.elem
                && actual_lanes == lanes =>
            {
                dst
            }
            _ => return None,
        };
        if !fresh_virtual(compared, fresh) {
            return None;
        }
        *offset += 1;

        let matches = exact_movemask(
            block,
            index,
            offset,
            guest_pc,
            compared,
            encoding.elem,
            lanes,
            fresh,
        )?;
        let bit_op = exact_op(block, index, *offset, guest_pc)?;
        let bit = match bit_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(value),
                width: OpWidth::W64,
            } if value == 1i64 << lane => dst,
            _ => return None,
        };
        if !fresh_virtual(bit, fresh) {
            return None;
        }
        *offset += 1;

        let select = exact_op(block, index, *offset, guest_pc)?;
        let selected = match select.kind {
            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width: OpWidth::W64,
            } if cond == matches && src_true == bit && src_false == zero => dst,
            _ => return None,
        };
        if !fresh_virtual(selected, fresh) {
            return None;
        }
        *offset += 1;

        let first_mask = exact_op(block, index, *offset, guest_pc)?;
        if !matches!(
            first_mask.kind,
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == mask1 && src1 == mask1 && src2 == selected
        ) {
            return None;
        }
        *offset += 1;

        let second_mask = exact_op(block, index, *offset, guest_pc)?;
        if !matches!(
            second_mask.kind,
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == mask2 && src1 == mask2 && src2 == matches
        ) {
            return None;
        }
        *offset += 1;
    }

    for (destination, source) in [
        (encoding.destination_base, mask1),
        (encoding.destination_base + 1, mask2),
    ] {
        let commit = exact_op(block, index, *offset, guest_pc)?;
        if !matches!(
            commit.kind,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::K(actual_destination))),
                src: SrcOperand::Reg(actual_source),
                width: OpWidth::W64,
            } if actual_destination == destination && actual_source == source
        ) {
            return None;
        }
        *offset += 1;
    }
    Some(())
}

fn exact_virtual_span_closure(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    consumed: usize,
    fresh: &HashSet<VReg>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let mut local_definitions = HashMap::<VReg, usize>::new();
    let mut local_uses = HashMap::<VReg, usize>::new();
    for op in &block.ops[index..index + consumed] {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *local_definitions.entry(register).or_default() += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *local_uses.entry(register).or_default() += 1;
            }
        }
    }
    fresh.iter().all(|register| {
        local_definitions.contains_key(register)
            && local_uses.contains_key(register)
            && virtual_definitions.get(register) == local_definitions.get(register)
            && virtual_uses.get(register) == local_uses.get(register)
    }) && local_definitions
        .keys()
        .all(|register| fresh.contains(register))
        && local_uses.keys().all(|register| fresh.contains(register))
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// `VP2INTERSECTD/Q` memory source.
///
/// Exact provenance binds both K destinations, D/Q element selection, vector
/// width, source1, full versus broadcast tuple, all L-by-L equality tests,
/// both movemask reductions, one unconditional memory access, APX address
/// guard, virtual-value closure, and the sole same-PC frontier. Runtime is
/// O(L^2) and auxiliary space is O(V), where L <= 16 and V is the matched
/// virtual-value count; callers build global definition/use maps once.
pub(crate) fn x86_jit_evex_vp2intersect_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexVp2IntersectMemorySequence> {
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
        .evex_vp2intersect_memory_encoding()?;
    let mut fresh = HashSet::new();
    let mut offset = 0usize;
    let source2 = exact_memory_source(block, index, &mut offset, guest_pc, encoding, &mut fresh)?;
    exact_intersection_graph(
        block,
        index,
        &mut offset,
        guest_pc,
        source2,
        encoding,
        &mut fresh,
    )?;
    if !no_following_same_pc(block, index, offset, guest_pc)
        || !exact_virtual_span_closure(
            block,
            index,
            offset,
            &fresh,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    let address = exact_evex_memory_sequence_address(block, index, 0)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }
    Some(X86JitEvexVp2IntersectMemorySequence {
        consumed: offset,
        address_offset: 0,
        encoding,
    })
}
