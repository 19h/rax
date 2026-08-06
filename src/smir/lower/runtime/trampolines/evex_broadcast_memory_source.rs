//! Fail-closed helper-backed EVEX memory-broadcast admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand,
    VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{X86EvexBroadcastMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_evex_vector_mask_result_with_raw_counts, exact_nonzero_mask_predicate_with_uses,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed EVEX
/// memory-broadcast lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexBroadcastMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexBroadcastMemoryEncoding,
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 | VecElementType::F32 => Some(MemWidth::B4),
        VecElementType::I64 | VecElementType::F64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn same_frontier(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: usize,
    guest_pc: GuestAddr,
) -> bool {
    block
        .ops
        .get(index + offset)
        .is_some_and(|op| op.guest_pc == guest_pc && op.x86_hint.is_none())
}

fn exact_lane_address(address: &Address, base: VReg, offset: i64) -> bool {
    matches!(
        address,
        Address::BaseOffset {
            base: actual_base,
            offset: actual_offset,
            disp_size: DispSize::Auto,
        } if *actual_base == base && *actual_offset == offset
    )
}

/// Validate the complete O0/O1/O2 SMIR decomposition of one EVEX
/// `VBROADCAST*` or `VPBROADCAST*` memory source.
///
/// Source-byte provenance binds all 34 legal opcode/W/vector-length shapes,
/// destination and mask control. Every tuple load, aggregate fault-suppression
/// predicate, repeated source lane, merge/zero destination lane, virtual
/// definition/use count, APX address guard, and guest-PC frontier is checked.
/// Classification is O(S + L) time and O(1) auxiliary space for S source
/// tuple lanes and L destination lanes (S <= 8, L <= 64); callers build the
/// global definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_broadcast_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBroadcastMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_broadcast_memory_encoding()?;
    let destination_lanes = encoding.width.lanes(encoding.elem) as usize;
    let source_lanes = usize::from(encoding.source_lanes);
    let mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let mut offset = 0usize;

    let memory_condition = if let Some(mask) = mask {
        let applicable_bits = if destination_lanes == 64 {
            u64::MAX
        } else {
            (1u64 << destination_lanes) - 1
        };
        Some(exact_nonzero_mask_predicate_with_uses(
            block,
            index,
            &mut offset,
            guest_pc,
            mask,
            applicable_bits,
            source_lanes,
            virtual_definitions,
            virtual_uses,
        )?)
    } else {
        None
    };

    let address_offset = offset;
    let lea = block.ops.get(index + offset)?;
    let (base, address) = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => (*base, addr),
        _ => return None,
    };
    if lea.guest_pc != guest_pc
        || !exact_virtual_definition_use(base, 1, source_lanes, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let source_zero_op = block.ops.get(index + offset)?;
    let source_zero = match source_zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if source_zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if source_zero_op.guest_pc != guest_pc
        || !single_definition_single_use(source_zero, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let source_op = block.ops.get(index + offset)?;
    let source = match source_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if source_op.x86_hint.is_none()
            && scalar == source_zero
            && elem == encoding.elem
            && usize::from(lanes) == destination_lanes =>
        {
            dst
        }
        _ => return None,
    };
    let source_uses = source_lanes
        + if source_lanes == 1 {
            1
        } else {
            destination_lanes
        };
    if source_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            source,
            source_lanes + 1,
            source_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    offset += 1;

    let lane_width = memory_width(encoding.elem)?;
    for lane in 0..source_lanes {
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

        let load = block.ops.get(index + offset)?;
        let expected_offset = (lane as i64) * i64::from(encoding.elem.bytes());
        let load_matches = match (&load.kind, memory_condition) {
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
                    && *width == lane_width
                    && exact_lane_address(addr, base, expected_offset)
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
                    && *width == lane_width
                    && exact_lane_address(addr, base, expected_offset)
            }
            _ => false,
        };
        if !load_matches || !same_frontier(block, index, offset, guest_pc) {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.guest_pc != guest_pc
            || insert.x86_hint.is_some()
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: inserted,
                    lane: inserted_lane,
                    elem,
                } if dst == source
                    && vec == source
                    && inserted == scalar
                    && usize::from(inserted_lane) == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    let (raw, raw_definitions, raw_uses) = if source_lanes == 1 {
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none() && vec == source && elem == encoding.elem => dst,
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;
        let broadcast = block.ops.get(index + offset)?;
        let raw = match broadcast.kind {
            OpKind::VBroadcast {
                dst,
                scalar: actual_scalar,
                elem,
                lanes,
            } if broadcast.x86_hint.is_none()
                && actual_scalar == scalar
                && elem == encoding.elem
                && usize::from(lanes) == destination_lanes =>
            {
                dst
            }
            _ => return None,
        };
        if broadcast.guest_pc != guest_pc {
            return None;
        }
        offset += 1;
        (raw, 1, if mask.is_some() { destination_lanes } else { 1 })
    } else {
        let result_zero_op = block.ops.get(index + offset)?;
        let result_zero = match result_zero_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } if result_zero_op.x86_hint.is_none() => dst,
            _ => return None,
        };
        if result_zero_op.guest_pc != guest_pc
            || !single_definition_single_use(result_zero, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let zeroed_op = block.ops.get(index + offset)?;
        let zeroed = match zeroed_op.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes,
            } if zeroed_op.x86_hint.is_none()
                && scalar == result_zero
                && elem == encoding.elem
                && usize::from(lanes) == destination_lanes =>
            {
                dst
            }
            _ => return None,
        };
        if zeroed_op.guest_pc != guest_pc
            || !single_definition_single_use(zeroed, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let mut raw = None;
        for lane in 0..destination_lanes {
            let extract = block.ops.get(index + offset)?;
            let scalar = match extract.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: extracted_lane,
                    elem,
                    sign: SignExtend::Zero,
                } if extract.x86_hint.is_none()
                    && vec == source
                    && usize::from(extracted_lane) == lane % source_lanes
                    && elem == encoding.elem =>
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
            offset += 1;

            let insert = block.ops.get(index + offset)?;
            let destination = match insert.kind {
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar: inserted,
                    lane: inserted_lane,
                    elem,
                } if insert.x86_hint.is_none()
                    && vec == raw.unwrap_or(zeroed)
                    && inserted == scalar
                    && usize::from(inserted_lane) == lane
                    && elem == encoding.elem =>
                {
                    dst
                }
                _ => return None,
            };
            if insert.guest_pc != guest_pc
                || !matches!(destination, VReg::Virtual(_))
                || raw.is_some_and(|prior| prior != destination)
            {
                return None;
            }
            raw = Some(destination);
            offset += 1;
        }
        let raw = raw?;
        (
            raw,
            destination_lanes,
            if mask.is_some() {
                destination_lanes.checked_mul(2)?.checked_sub(1)?
            } else {
                destination_lanes
            },
        )
    };

    if let Some(mask) = mask {
        exact_evex_vector_mask_result_with_raw_counts(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            mask,
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            raw_definitions,
            raw_uses,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        if !exact_virtual_definition_use(
            raw,
            raw_definitions,
            raw_uses,
            virtual_definitions,
            virtual_uses,
        ) {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.guest_pc != guest_pc
            || commit.x86_hint.is_some()
            || !matches!(
                commit.kind,
                OpKind::VMov { dst, src, width }
                    if src == raw
                        && width == encoding.width
                        && vector_index(&dst, encoding.width) == Some(encoding.destination)
            )
        {
            return None;
        }
        offset += 1;
    }

    if !no_following_same_pc(block, index, offset, guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }
    Some(X86JitEvexBroadcastMemorySequence {
        consumed: offset,
        address_offset,
        encoding,
    })
}
