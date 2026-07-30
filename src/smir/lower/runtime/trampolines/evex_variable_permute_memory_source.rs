//! Fail-closed helper-backed EVEX variable VPERMILPS/PD memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexVariablePermuteMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::vector_index;
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed EVEX
/// variable-control VPERMILPS/PD memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexVariablePermuteMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86EvexVariablePermuteMemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
        _ => unreachable!("validated EVEX variable-permute width"),
    }))
}

fn unique_virtual(register: VReg, seen: &mut HashSet<VReg>) -> Option<VReg> {
    matches!(register, VReg::Virtual(_))
        .then_some(register)
        .filter(|candidate| seen.insert(*candidate))
}

fn local_virtual_counts_match(
    ops: &[crate::smir::ir::ops::SmirOp],
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let mut local_definitions = HashMap::new();
    let mut local_uses = HashMap::new();
    for op in ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *local_definitions.entry(register).or_insert(0usize) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *local_uses.entry(register).or_insert(0usize) += 1;
            }
        }
    }
    local_definitions
        .iter()
        .all(|(register, count)| virtual_definitions.get(register) == Some(count))
        && local_uses
            .iter()
            .all(|(register, count)| virtual_uses.get(register) == Some(count))
}

fn match_loaded_controls(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    cursor: &mut usize,
    encoding: X86EvexVariablePermuteMemoryEncoding,
    seen: &mut HashSet<VReg>,
) -> Option<VReg> {
    let memory = block.ops.get(index + *cursor)?;
    if encoding.broadcast {
        let scalar_width = match encoding.elem {
            VecElementType::F32 => MemWidth::B4,
            VecElementType::F64 => MemWidth::B8,
            _ => return None,
        };
        let scalar = match &memory.kind {
            OpKind::Load {
                dst,
                addr,
                width,
                sign: SignExtend::Zero,
            } if memory.x86_hint.is_none()
                && *width == scalar_width
                && x86_jit_mem_address_shape_valid(addr) =>
            {
                unique_virtual(*dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;
        let broadcast = block.ops.get(index + *cursor)?;
        let controls = match broadcast.kind {
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
                unique_virtual(dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;
        Some(controls)
    } else {
        let controls = match &memory.kind {
            OpKind::VLoad { dst, addr, width }
                if memory.x86_hint.is_none()
                    && *width == encoding.width
                    && x86_jit_mem_address_shape_valid(addr) =>
            {
                unique_virtual(*dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;
        Some(controls)
    }
}

fn match_indices_and_raw(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    cursor: &mut usize,
    controls: VReg,
    encoding: X86EvexVariablePermuteMemoryEncoding,
    seen: &mut HashSet<VReg>,
) -> Option<VReg> {
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let (domain_lanes, control_shift) = match encoding.elem {
        VecElementType::F32 => (4u8, 0u8),
        VecElementType::F64 => (2u8, 1u8),
        _ => return None,
    };
    let zero_op = block.ops.get(index + *cursor)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => unique_virtual(dst, seen)?,
        _ => return None,
    };
    *cursor += 1;
    let indices_op = block.ops.get(index + *cursor)?;
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
            unique_virtual(dst, seen)?
        }
        _ => return None,
    };
    *cursor += 1;

    for lane in 0..lanes {
        let extract = block.ops.get(index + *cursor)?;
        let control = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && vec == controls
                && actual_lane == lane
                && elem == encoding.elem =>
            {
                unique_virtual(dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;

        let shifted = if control_shift == 0 {
            match block.ops.get(index + *cursor).map(|op| &op.kind) {
                Some(OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(src),
                    width: OpWidth::W64,
                }) if *src == control => {
                    let shifted = unique_virtual(*dst, seen)?;
                    *cursor += 1;
                    shifted
                }
                _ => control,
            }
        } else {
            let shift = block.ops.get(index + *cursor)?;
            let shifted = match shift.kind {
                OpKind::Shr {
                    dst,
                    src,
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if shift.x86_hint.is_none()
                    && src == control
                    && amount == i64::from(control_shift) =>
                {
                    unique_virtual(dst, seen)?
                }
                _ => return None,
            };
            *cursor += 1;
            shifted
        };

        let and = block.ops.get(index + *cursor)?;
        let selected = match and.kind {
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if and.x86_hint.is_none()
                && src1 == shifted
                && mask == i64::from(domain_lanes - 1) =>
            {
                unique_virtual(dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;

        let base = i64::from(lane / domain_lanes * domain_lanes);
        let absolute_op = block.ops.get(index + *cursor)?;
        let absolute = match absolute_op.kind {
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Imm(actual_base),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if absolute_op.x86_hint.is_none() && src1 == selected && actual_base == base => {
                unique_virtual(dst, seen)?
            }
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W64,
            } if absolute_op.x86_hint.is_none() && base == 0 && src == selected => {
                unique_virtual(dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;

        let insert = block.ops.get(index + *cursor)?;
        if !matches!(
            insert.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: actual_lane,
                elem,
            } if insert.x86_hint.is_none()
                && dst == indices
                && vec == indices
                && scalar == absolute
                && actual_lane == lane
                && elem == encoding.elem
        ) {
            return None;
        }
        *cursor += 1;
    }

    let permute = block.ops.get(index + *cursor)?;
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
            && src1 == vector(encoding.source1, encoding.width)
            && actual_indices == indices
            && elem == encoding.elem
            && width == encoding.width =>
        {
            unique_virtual(dst, seen)?
        }
        _ => return None,
    };
    *cursor += 1;
    Some(raw)
}

fn match_lane_predicate(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    cursor: &mut usize,
    mask: VReg,
    lane: u8,
    seen: &mut HashSet<VReg>,
) -> Option<VReg> {
    let first = block.ops.get(index + *cursor)?;
    if lane == 0 {
        if let OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } = first.kind
        {
            if first.x86_hint.is_none() && src1 == mask {
                let condition = unique_virtual(dst, seen)?;
                *cursor += 1;
                return Some(condition);
            }
        }
    }

    let shifted = match first.kind {
        OpKind::Shr {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if first.x86_hint.is_none() && src == mask && amount == i64::from(lane) => {
            unique_virtual(dst, seen)?
        }
        _ => return None,
    };
    *cursor += 1;
    let and = block.ops.get(index + *cursor)?;
    let condition = match and.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if and.x86_hint.is_none() && src1 == shifted => unique_virtual(dst, seen)?,
        _ => return None,
    };
    *cursor += 1;
    Some(condition)
}

fn match_masked_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    cursor: &mut usize,
    raw: VReg,
    encoding: X86EvexVariablePermuteMemoryEncoding,
    seen: &mut HashSet<VReg>,
) -> Option<()> {
    let destination = vector(encoding.destination, encoding.width);
    if encoding.writemask == 0 {
        let commit = block.ops.get(index + *cursor)?;
        if !matches!(
            commit.kind,
            OpKind::VMov { dst, src, width }
                if commit.x86_hint.is_none()
                    && dst == destination
                    && src == raw
                    && width == encoding.width
        ) {
            return None;
        }
        *cursor += 1;
        return Some(());
    }

    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let old = if encoding.zeroing {
        None
    } else {
        let old_op = block.ops.get(index + *cursor)?;
        let old = match old_op.kind {
            OpKind::VMov { dst, src, width }
                if old_op.x86_hint.is_none() && src == destination && width == encoding.width =>
            {
                unique_virtual(dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;
        Some(old)
    };
    let zero_op = block.ops.get(index + *cursor)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => unique_virtual(dst, seen)?,
        _ => return None,
    };
    *cursor += 1;
    let base_op = block.ops.get(index + *cursor)?;
    let result_base = match base_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if base_op.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            unique_virtual(dst, seen)?
        }
        _ => return None,
    };
    *cursor += 1;

    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask)));
    let lane_width = match encoding.elem {
        VecElementType::F32 => OpWidth::W32,
        VecElementType::F64 => OpWidth::W64,
        _ => return None,
    };
    for lane in 0..lanes {
        let condition = match_lane_predicate(block, index, cursor, mask, lane, seen)?;
        let active_op = block.ops.get(index + *cursor)?;
        let active = match active_op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if active_op.x86_hint.is_none()
                && vec == raw
                && actual_lane == lane
                && elem == encoding.elem =>
            {
                unique_virtual(dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;
        let inactive = if let Some(old) = old {
            let inactive_op = block.ops.get(index + *cursor)?;
            let inactive = match inactive_op.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: actual_lane,
                    elem,
                    sign: SignExtend::Zero,
                } if inactive_op.x86_hint.is_none()
                    && vec == old
                    && actual_lane == lane
                    && elem == encoding.elem =>
                {
                    unique_virtual(dst, seen)?
                }
                _ => return None,
            };
            *cursor += 1;
            inactive
        } else {
            zero
        };

        let select = block.ops.get(index + *cursor)?;
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
                && src_false == inactive
                && width == lane_width =>
            {
                unique_virtual(dst, seen)?
            }
            _ => return None,
        };
        *cursor += 1;
        let insert = block.ops.get(index + *cursor)?;
        if !matches!(
            insert.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: actual_lane,
                elem,
            } if insert.x86_hint.is_none()
                && dst == destination
                && vec == if lane == 0 { result_base } else { destination }
                && scalar == selected
                && actual_lane == lane
                && elem == encoding.elem
        ) {
            return None;
        }
        *cursor += 1;
    }
    Some(())
}

/// Validate the complete O0/O1/O2 decomposition emitted for one memory-source
/// EVEX variable VPERMILPS/PD.
///
/// Source bytes bind operation, width, operands, broadcast, and mask policy.
/// Every temporary is confined to the sequence. Classification is O(L) time
/// and O(L) auxiliary space for at most 16 lanes and 184 operations; callers
/// construct global definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_variable_permute_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexVariablePermuteMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_variable_permute_memory_encoding()?;
    let mut cursor = 0usize;
    let mut seen = HashSet::new();
    let controls = match_loaded_controls(block, index, &mut cursor, encoding, &mut seen)?;
    let raw = match_indices_and_raw(block, index, &mut cursor, controls, encoding, &mut seen)?;
    match_masked_result(block, index, &mut cursor, raw, encoding, &mut seen)?;

    let sequence = block.ops.get(index..index.checked_add(cursor)?)?;
    if sequence
        .iter()
        .any(|op| op.guest_pc != guest_pc || op.x86_hint.is_some())
        || block
            .ops
            .get(index + cursor)
            .is_some_and(|op| op.guest_pc == guest_pc)
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }
    Some(X86JitEvexVariablePermuteMemorySequence {
        consumed: cursor,
        encoding,
    })
}
