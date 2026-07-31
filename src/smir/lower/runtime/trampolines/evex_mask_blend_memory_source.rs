//! Fail-closed helper-backed EVEX opmask-selector blend memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexMaskBlendMemoryEncoding, X86EvexMaskBlendMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_lane_address, exact_lane_predicate, exact_nonzero_mask_predicate,
    exact_virtual_definition_use, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// mask-blend memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexMaskBlendMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexMaskBlendMemoryEncoding,
}

#[derive(Clone, Copy)]
struct MatchedMemorySource {
    loaded: VReg,
    offset: usize,
    address_offset: usize,
    memory_size: u32,
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 => Some(MemWidth::B4),
        VecElementType::I64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn lane_width(elem: VecElementType) -> Option<OpWidth> {
    match elem {
        VecElementType::I8 => Some(OpWidth::W8),
        VecElementType::I16 => Some(OpWidth::W16),
        VecElementType::I32 => Some(OpWidth::W32),
        VecElementType::I64 => Some(OpWidth::W64),
        _ => None,
    }
}

fn applicable_bits(lanes: u8) -> u64 {
    if lanes == 64 {
        u64::MAX
    } else {
        (1u64 << lanes) - 1
    }
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

fn unmasked_vector_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexMaskBlendMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    if encoding.selector.is_some() || encoding.zeroing {
        return None;
    }
    let lanes = encoding.width.lanes(encoding.elem) as usize;
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && *width == encoding.width
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(loaded, 1, lanes, virtual_definitions, virtual_uses) {
        return None;
    }
    Some(MatchedMemorySource {
        loaded,
        offset: 1,
        address_offset: 0,
        memory_size: encoding.width.bytes(),
    })
}

fn unmasked_broadcast_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexMaskBlendMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    if encoding.selector.is_some() || encoding.zeroing {
        return None;
    }
    let lanes = encoding.width.lanes(encoding.elem) as usize;
    let expected_width = memory_width(encoding.elem)?;
    let load = block.ops.get(index)?;
    let scalar = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *width == expected_width
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
            elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && elem == encoding.elem
            && usize::from(actual_lanes) == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != load.guest_pc
        || !exact_virtual_definition_use(loaded, 1, lanes, virtual_definitions, virtual_uses)
    {
        return None;
    }
    Some(MatchedMemorySource {
        loaded,
        offset: 2,
        address_offset: 0,
        memory_size: expected_width.bytes(),
    })
}

fn masked_broadcast_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexMaskBlendMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let selector = encoding.selector?;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(selector)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let mut offset = 0usize;
    let condition = exact_nonzero_mask_predicate(
        block,
        index,
        &mut offset,
        guest_pc,
        mask,
        applicable_bits(lanes),
        virtual_definitions,
        virtual_uses,
    )?;

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

    let address_offset = offset;
    let expected_width = memory_width(encoding.elem)?;
    let load = block.ops.get(index + offset)?;
    if !matches!(
        &load.kind,
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width,
            signed: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *dst == scalar
            && *cond == condition
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr)
    ) || load.guest_pc != guest_pc
    {
        return None;
    }
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar: actual_scalar,
            elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            loaded,
            1,
            usize::from(lanes),
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    offset += 1;
    Some(MatchedMemorySource {
        loaded,
        offset,
        address_offset,
        memory_size: expected_width.bytes(),
    })
}

fn masked_vector_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexMaskBlendMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let selector = encoding.selector?;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(selector)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let zero = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !exact_virtual_definition_use(zero, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            loaded,
            usize::from(lanes) + 1,
            2 * usize::from(lanes),
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let address_offset = 2usize;
    let lea = block.ops.get(index + address_offset)?;
    let (base, original_address) = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => (*base, addr),
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
        || !original_address.is_x86_state_backed_shape()
    {
        return None;
    }

    let expected_width = memory_width(encoding.elem)?;
    let lane_bytes = i64::from(encoding.elem.bytes());
    let mut offset = address_offset + 1;
    for lane in 0..lanes {
        let condition = exact_lane_predicate(
            block,
            index,
            &mut offset,
            guest_pc,
            mask,
            lane,
            virtual_definitions,
            virtual_uses,
        )?;
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
        if !matches!(
            &load.kind,
            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed: SignExtend::Zero,
            } if load.x86_hint.is_none()
                && *dst == scalar
                && *cond == condition
                && *width == expected_width
                && exact_lane_address(addr, base, i64::from(lane) * lane_bytes)
        ) || load.guest_pc != guest_pc
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
                    scalar: actual_scalar,
                    lane: actual_lane,
                    elem,
                } if dst == loaded
                    && vec == loaded
                    && actual_scalar == scalar
                    && actual_lane == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    Some(MatchedMemorySource {
        loaded,
        offset,
        address_offset,
        memory_size: encoding.width.bytes(),
    })
}

#[allow(clippy::too_many_arguments)]
fn exact_blend_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    mut offset: usize,
    loaded: VReg,
    encoding: X86EvexMaskBlendMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let raw_zero_op = block.ops.get(index + offset)?;
    let raw_zero = match raw_zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if raw_zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if raw_zero_op.guest_pc != guest_pc
        || !single_definition_single_use(raw_zero, virtual_definitions, virtual_uses)
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
            lanes: actual_lanes,
        } if raw_op.x86_hint.is_none()
            && scalar == raw_zero
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if raw_op.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            raw,
            usize::from(lanes) + 1,
            usize::from(lanes) + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    offset += 1;

    let fallback_zero = if matches!(
        block.ops.get(index + offset).map(|op| &op.kind),
        Some(OpKind::Mov {
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
            ..
        })
    ) {
        let zero_op = block.ops.get(index + offset)?;
        let zero = match zero_op.kind {
            OpKind::Mov { dst, .. } if zero_op.x86_hint.is_none() => dst,
            _ => unreachable!("zero shape prevalidated"),
        };
        let expected_uses = if encoding.zeroing {
            usize::from(lanes)
        } else {
            0
        };
        if zero_op.guest_pc != guest_pc
            || !exact_virtual_definition_use(
                zero,
                1,
                expected_uses,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        offset += 1;
        Some(zero)
    } else {
        None
    };
    if encoding.zeroing && fallback_zero.is_none() {
        return None;
    }

    let selector = encoding
        .selector
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let select_width = lane_width(encoding.elem)?;
    for lane in 0..lanes {
        let active_op = block.ops.get(index + offset)?;
        let active = match active_op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if active_op.x86_hint.is_none()
                && vec == loaded
                && actual_lane == lane
                && elem == encoding.elem =>
            {
                dst
            }
            _ => return None,
        };
        if active_op.guest_pc != guest_pc
            || !single_definition_single_use(active, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let selected = if let Some(mask) = selector {
            let fallback = if encoding.zeroing {
                fallback_zero?
            } else {
                let fallback_op = block.ops.get(index + offset)?;
                let fallback = match fallback_op.kind {
                    OpKind::VExtractLane {
                        dst,
                        vec,
                        lane: actual_lane,
                        elem,
                        sign: SignExtend::Zero,
                    } if fallback_op.x86_hint.is_none()
                        && vector_index(&vec, encoding.width) == Some(encoding.source1)
                        && actual_lane == lane
                        && elem == encoding.elem =>
                    {
                        dst
                    }
                    _ => return None,
                };
                if fallback_op.guest_pc != guest_pc
                    || !single_definition_single_use(fallback, virtual_definitions, virtual_uses)
                {
                    return None;
                }
                offset += 1;
                fallback
            };
            let condition = exact_lane_predicate(
                block,
                index,
                &mut offset,
                guest_pc,
                mask,
                lane,
                virtual_definitions,
                virtual_uses,
            )?;
            let select = block.ops.get(index + offset)?;
            let selected = match select.kind {
                OpKind::Select {
                    dst,
                    cond,
                    src_true,
                    src_false,
                    width,
                } if select.x86_hint.is_none()
                    && cond == condition
                    && src_true == active
                    && src_false == fallback
                    && width == select_width =>
                {
                    dst
                }
                _ => return None,
            };
            if select.guest_pc != guest_pc
                || !single_definition_single_use(selected, virtual_definitions, virtual_uses)
            {
                return None;
            }
            offset += 1;
            selected
        } else {
            active
        };

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
                    elem,
                } if dst == raw
                    && vec == raw
                    && scalar == selected
                    && actual_lane == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        offset += 1;
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
    no_following_same_pc(block, index, offset, guest_pc).then_some(offset)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// V[P]BLENDM* memory source.
///
/// Exact provenance binds opcode, widths, architectural operands, selector
/// and zeroing policy, broadcast/full-vector tuple, Type E4 helper accesses,
/// lane selection, and the single architectural commit. Classification is
/// O(L) time and O(1) auxiliary space for L <= 64 lanes; callers build
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_mask_blend_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexMaskBlendMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_mask_blend_memory_encoding()?;
    let source = match encoding.replay {
        X86EvexMaskBlendMemoryReplay::Vector { .. } => {
            unmasked_vector_source(block, index, encoding, virtual_definitions, virtual_uses)?
        }
        X86EvexMaskBlendMemoryReplay::Broadcast { .. } if encoding.selector.is_some() => {
            masked_broadcast_source(block, index, encoding, virtual_definitions, virtual_uses)?
        }
        X86EvexMaskBlendMemoryReplay::Broadcast { .. } => {
            unmasked_broadcast_source(block, index, encoding, virtual_definitions, virtual_uses)?
        }
        X86EvexMaskBlendMemoryReplay::MaskedVector { .. } => {
            masked_vector_source(block, index, encoding, virtual_definitions, virtual_uses)?
        }
    };
    let consumed = exact_blend_result(
        block,
        index,
        source.offset,
        source.loaded,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexMaskBlendMemorySequence {
        consumed,
        address_offset: source.address_offset,
        memory_size: source.memory_size,
        encoding,
    })
}
