//! Fail-closed helper-backed EVEX packed widening-move admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedExtendMemoryEncoding, X86EvexPackedExtendMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_address,
    exact_evex_memory_sequence_frontier, exact_evex_reconstructed_vector_mask_result,
    exact_lane_address, exact_lane_predicate, exact_virtual_definition_use, no_following_same_pc,
    single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact EVEX packed widening-move decomposition consumed by x86-64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedExtendMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedExtendMemoryEncoding,
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 => Some(MemWidth::B4),
        _ => None,
    }
}

fn destination_register(encoding: X86EvexPackedExtendMemoryEncoding) -> VReg {
    VReg::Arch(ArchReg::X86(match encoding.width {
        VecWidth::V128 => X86Reg::Xmm(encoding.destination),
        VecWidth::V256 => X86Reg::Ymm(encoding.destination),
        VecWidth::V512 => X86Reg::Zmm(encoding.destination),
        VecWidth::V64 => unreachable!("EVEX packed-extension destination width"),
    }))
}

#[allow(clippy::too_many_arguments)]
fn exact_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedExtendMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<(usize, VReg)> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let lanes = encoding.lanes;
    let source_elem = encoding.source_elem;

    let zero_op = block.ops.get(index)?;
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

    let source_op = block.ops.get(index + 1)?;
    let source = match source_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: container_lanes,
        } if source_op.x86_hint.is_none()
            && scalar == zero
            && elem == source_elem
            && container_lanes == encoding.source_width.lanes(source_elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if source_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            source,
            usize::from(lanes) + 1,
            usize::from(lanes) * 2,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let lea = block.ops.get(index + 2)?;
    let base = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *base,
        _ => return None,
    };
    if lea.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            base,
            1,
            usize::from(lanes),
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let expected_width = memory_width(source_elem)?;
    let mut offset = 3usize;
    for lane in 0..lanes {
        let seed = block.ops.get(index + offset)?;
        let scalar = match seed.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } if seed.x86_hint.is_none() => dst,
            _ => return None,
        };
        if seed.guest_pc != guest_pc
            || !exact_virtual_definition_use(scalar, 2, 1, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let condition = if let Some(mask) = mask {
            Some(exact_lane_predicate(
                block,
                index,
                &mut offset,
                guest_pc,
                mask,
                lane,
                virtual_definitions,
                virtual_uses,
            )?)
        } else {
            None
        };
        let load = block.ops.get(index + offset)?;
        let lane_offset = i64::from(lane) * i64::from(source_elem.bytes());
        let exact_load = match (&load.kind, condition) {
            (
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                },
                None,
            ) => {
                *dst == scalar
                    && *width == expected_width
                    && load.x86_hint.is_none()
                    && exact_lane_address(addr, base, lane_offset)
            }
            (
                OpKind::PredLoad {
                    dst,
                    cond,
                    addr,
                    width,
                    signed: SignExtend::Zero,
                },
                Some(expected_condition),
            ) => {
                *dst == scalar
                    && *cond == expected_condition
                    && *width == expected_width
                    && load.x86_hint.is_none()
                    && exact_lane_address(addr, base, lane_offset)
            }
            _ => false,
        };
        if !exact_load || load.guest_pc != guest_pc {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.guest_pc != guest_pc
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if insert.x86_hint.is_none()
                    && dst == source
                    && vec == source
                    && actual_scalar == scalar
                    && actual_lane == lane
                    && elem == source_elem
            )
        {
            return None;
        }
        offset += 1;
    }
    Some((offset, source))
}

#[allow(clippy::too_many_arguments)]
fn exact_extend_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    mut offset: usize,
    source: VReg,
    encoding: X86EvexPackedExtendMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let expected_sign = if encoding.signed {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let mut scalars = Vec::with_capacity(usize::from(encoding.lanes));
    for lane in 0..encoding.lanes {
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign,
            } if extract.x86_hint.is_none()
                && vec == source
                && actual_lane == lane
                && elem == encoding.source_elem
                && sign == expected_sign =>
            {
                dst
            }
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        scalars.push(scalar);
        offset += 1;
    }

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

    let raw_op = block.ops.get(index + offset)?;
    let raw = match raw_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if raw_op.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.destination_elem
            && lanes == encoding.lanes =>
        {
            dst
        }
        _ => return None,
    };
    if raw_op.guest_pc != guest_pc {
        return None;
    }
    offset += 1;

    for (lane, scalar) in scalars.into_iter().enumerate() {
        let insert = block.ops.get(index + offset)?;
        if insert.guest_pc != guest_pc
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if insert.x86_hint.is_none()
                    && dst == raw
                    && vec == raw
                    && actual_scalar == scalar
                    && usize::from(actual_lane) == lane
                    && elem == encoding.destination_elem
            )
        {
            return None;
        }
        offset += 1;
    }

    if let Some(mask) = encoding.writemask {
        exact_evex_reconstructed_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            encoding.destination_elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        let lanes = usize::from(encoding.lanes);
        if !exact_virtual_definition_use(
            raw,
            lanes + 1,
            lanes + 1,
            virtual_definitions,
            virtual_uses,
        ) {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.guest_pc != guest_pc
            || !matches!(
                commit.kind,
                OpKind::VMov { dst, src, width }
                    if commit.x86_hint.is_none()
                        && dst == destination_register(encoding)
                        && vector_index(&dst, encoding.width) == Some(encoding.destination)
                        && src == raw
                        && width == encoding.width
            )
        {
            return None;
        }
        offset += 1;
    }
    Some(offset)
}

/// Validate the complete O0/O1/O2 decomposition for all twelve EVEX packed
/// sign/zero-extension memory-source mnemonics.
///
/// Exact provenance binds opcode/W, destination and source widths, tuple
/// extent, signedness, writemask, APX address frontier, every SSA use, and the
/// terminal guest-PC boundary. Matching is O(L) time and O(L) temporary space
/// for at most 32 destination lanes; callers build definition/use maps once in
/// O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_extend_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedExtendMemorySequence> {
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
        .evex_packed_extend_memory_encoding()?;
    let (source_consumed, source) =
        exact_memory_source(block, index, encoding, virtual_definitions, virtual_uses)?;
    let consumed = exact_extend_result(
        block,
        index,
        source_consumed,
        source,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    if !no_following_same_pc(block, index, consumed, guest_pc) {
        return None;
    }
    let address_offset = 2;
    let address = exact_evex_memory_sequence_address(block, index, address_offset)?;
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }
    Some(X86JitEvexPackedExtendMemorySequence {
        consumed,
        address_offset,
        memory_size: encoding.memory_size(),
        encoding,
    })
}
