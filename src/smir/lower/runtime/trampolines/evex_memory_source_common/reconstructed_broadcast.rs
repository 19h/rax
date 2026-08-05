//! Exact aggregate and lane-reconstructed EVEX masked-broadcast graphs.

use std::collections::HashMap;

use super::{
    X86EvexE4MemoryMatch, X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, evex_e4_memory_width,
    exact_e4_semantic_tail, exact_lane_address, exact_lane_predicate, exact_nonzero_mask_predicate,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use,
    x86_jit_mem_address_shape_valid,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, X86Reg};

/// Match the optimizer's direct `mask & applicable_bits` PredLoad condition.
/// PredLoad treats every nonzero value as true, so normalizing the value to
/// bit 0 is not semantically required.
#[allow(clippy::too_many_arguments)]
pub(super) fn exact_masked_value_predicate(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    mask: VReg,
    applicable_bits: u64,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let and = block.ops.get(index + *offset)?;
    let predicate = match and.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(actual_bits),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if and.x86_hint.is_none() && src1 == mask && actual_bits == applicable_bits as i64 => dst,
        _ => return None,
    };
    if and.guest_pc != guest_pc
        || !single_definition_single_use(predicate, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;
    Some(predicate)
}

/// Match the aggregate-gated scalar load and broadcast graph. The lifters may
/// emit either a normalized bit-0 predicate or the optimizer's direct nonzero
/// masked value, and may place the scalar zero seed on either side of the
/// predicate graph.
pub(super) fn exact_masked_e4_broadcast<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_tail: &F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    if shape.form != X86EvexE4MemoryReplayForm::Broadcast {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(shape.writemask?)));
    let lanes = shape.width.lanes(shape.elem) as u8;
    let applicable_bits = if lanes == 64 {
        u64::MAX
    } else {
        (1u64 << lanes) - 1
    };
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let leading_scalar = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => Some(dst),
        _ => None,
    };
    let mut offset = usize::from(leading_scalar.is_some());
    let predicate_offset = offset;
    let condition = match exact_nonzero_mask_predicate(
        block,
        index,
        &mut offset,
        guest_pc,
        mask,
        applicable_bits,
        virtual_definitions,
        virtual_uses,
    ) {
        Some(condition) => condition,
        None => {
            offset = predicate_offset;
            exact_masked_value_predicate(
                block,
                index,
                &mut offset,
                guest_pc,
                mask,
                applicable_bits,
                virtual_definitions,
                virtual_uses,
            )?
        }
    };

    let scalar = if let Some(scalar) = leading_scalar {
        scalar
    } else {
        let seed = block.ops.get(index + offset)?;
        let scalar = match seed.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } if seed.x86_hint.is_none() => dst,
            _ => return None,
        };
        if seed.guest_pc != guest_pc {
            return None;
        }
        offset += 1;
        scalar
    };
    if !exact_virtual_definition_use(scalar, 2, 1, virtual_definitions, virtual_uses) {
        return None;
    }

    let address_offset = offset;
    let expected_width = evex_e4_memory_width(shape.elem)?;
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
            && elem == shape.elem
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
            shape.memory_source_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    offset += 1;

    offset += exact_e4_semantic_tail(block, index + offset, guest_pc, loaded, exact_tail)?;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed: offset,
        address_offset,
        memory_size: expected_width.bytes(),
    })
}

/// Match a masked broadcast reconstructed by one zero vector, one address,
/// and an in-place predicated load/insert for each destination lane. Every
/// PredLoad must use the same scalar address; accepting a lane offset would
/// silently turn `{1toN}` into an ordinary vector access.
pub(super) fn exact_masked_e4_reconstructed_broadcast<F>(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    shape: X86EvexE4MemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
    exact_tail: &F,
) -> Option<X86EvexE4MemoryMatch>
where
    F: Fn(&crate::smir::ir::SmirBlock, usize, VReg) -> Option<usize>,
{
    if shape.form != X86EvexE4MemoryReplayForm::Broadcast {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(shape.writemask?)));
    let lanes = shape.width.lanes(shape.elem) as u8;
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
    if !single_definition_single_use(zero, virtual_definitions, virtual_uses) {
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
            && elem == shape.elem
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
            usize::from(lanes) + shape.memory_source_uses,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }

    let address_offset = 2usize;
    let lea = block.ops.get(index + address_offset)?;
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

    let expected_width = evex_e4_memory_width(shape.elem)?;
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
                && exact_lane_address(addr, base, 0)
        ) || load.guest_pc != guest_pc
        {
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
                    && dst == loaded
                    && vec == loaded
                    && actual_scalar == scalar
                    && actual_lane == lane
                    && elem == shape.elem
            )
        {
            return None;
        }
        offset += 1;
    }

    offset += exact_e4_semantic_tail(block, index + offset, guest_pc, loaded, exact_tail)?;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86EvexE4MemoryMatch {
        consumed: offset,
        address_offset,
        memory_size: expected_width.bytes(),
    })
}
