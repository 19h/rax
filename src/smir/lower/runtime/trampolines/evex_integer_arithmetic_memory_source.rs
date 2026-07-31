//! Fail-closed helper-backed EVEX packed integer arithmetic memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VLaneOp, VReg,
    VecElementType, X86Reg,
};
use crate::smir::ir::{
    X86EvexIntegerArithmeticMemoryEncoding, X86EvexIntegerArithmeticMemoryReplay,
    X86EvexIntegerMinMaxMemoryEncoding, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, exact_lane_address, exact_lane_predicate,
    exact_nonzero_mask_predicate, exact_virtual_definition_use, single_definition_single_use,
    vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// packed integer arithmetic memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexIntegerArithmeticMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexIntegerArithmeticMemoryEncoding,
}

#[derive(Clone, Copy)]
pub(super) struct X86EvexIntegerMemoryShape {
    pub(super) width: crate::smir::ir::types::VecWidth,
    pub(super) elem: VecElementType,
    pub(super) destination: u8,
    pub(super) writemask: Option<u8>,
    pub(super) zeroing: bool,
    pub(super) vector_load_hint: Option<X86OpHint>,
    pub(super) masked_broadcast_uses_lane_graph: bool,
}

impl From<X86EvexIntegerArithmeticMemoryEncoding> for X86EvexIntegerMemoryShape {
    fn from(encoding: X86EvexIntegerArithmeticMemoryEncoding) -> Self {
        let unaligned_vector_load = matches!(encoding.opcode, 0xE0 | 0xE3)
            || encoding.is_low_multiply()
            || encoding.is_high_word_multiply();
        Self {
            width: encoding.width,
            elem: encoding.elem,
            destination: encoding.destination,
            writemask: encoding.writemask,
            zeroing: encoding.zeroing,
            vector_load_hint: unaligned_vector_load.then_some(X86OpHint::VecAlign(
                crate::smir::ir::ops::X86VecAlign::Unaligned,
            )),
            masked_broadcast_uses_lane_graph: encoding.is_dot_product(),
        }
    }
}

impl From<X86EvexIntegerMinMaxMemoryEncoding> for X86EvexIntegerMemoryShape {
    fn from(encoding: X86EvexIntegerMinMaxMemoryEncoding) -> Self {
        Self {
            width: encoding.width,
            elem: encoding.elem,
            destination: encoding.destination,
            writemask: encoding.writemask,
            zeroing: encoding.zeroing,
            vector_load_hint: None,
            masked_broadcast_uses_lane_graph: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct MatchedMemorySource {
    pub(super) loaded: VReg,
    pub(super) offset: usize,
    pub(super) address_offset: usize,
    pub(super) memory_size: u32,
}

fn element_memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 => Some(MemWidth::B4),
        VecElementType::I64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn element_op_width(elem: VecElementType) -> Option<OpWidth> {
    match elem {
        VecElementType::I8 => Some(OpWidth::W8),
        VecElementType::I16 => Some(OpWidth::W16),
        VecElementType::I32 => Some(OpWidth::W32),
        VecElementType::I64 => Some(OpWidth::W64),
        _ => None,
    }
}

fn same_pc(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: usize,
    guest_pc: GuestAddr,
) -> bool {
    block
        .ops
        .get(index + offset)
        .is_some_and(|op| op.guest_pc == guest_pc)
}

fn unconditional_vector_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerMemoryShape,
    loaded_uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == encoding.vector_load_hint
                && *width == encoding.width
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if !exact_virtual_definition_use(loaded, 1, loaded_uses, virtual_definitions, virtual_uses) {
        return None;
    }
    Some(MatchedMemorySource {
        loaded,
        offset: 1,
        address_offset: 0,
        memory_size: encoding.width.bytes(),
    })
}

fn unconditional_broadcast_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerMemoryShape,
    loaded_uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let guest_pc = block.ops.get(index)?.guest_pc;
    let memory_width = element_memory_width(encoding.elem)?;
    let mut offset = 0usize;
    let seeded = matches!(
        block.ops.get(index).map(|op| &op.kind),
        Some(OpKind::Mov {
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
            ..
        })
    );
    let seed = if seeded {
        let seed = block.ops.get(index)?;
        if seed.x86_hint.is_some() || !same_pc(block, index, 0, guest_pc) {
            return None;
        }
        offset += 1;
        match seed.kind {
            OpKind::Mov { dst, .. } => Some(dst),
            _ => unreachable!("seeded broadcast matched Mov"),
        }
    } else {
        None
    };

    let address_offset = offset;
    let load = block.ops.get(index + offset)?;
    let scalar = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *width == memory_width
            && x86_jit_mem_address_shape_valid(addr)
            && seed.is_none_or(|seed| seed == *dst) =>
        {
            *dst
        }
        _ => return None,
    };
    let definitions = if seeded { 2 } else { 1 };
    if !same_pc(block, index, offset, guest_pc)
        || !exact_virtual_definition_use(scalar, definitions, 1, virtual_definitions, virtual_uses)
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
    if !same_pc(block, index, offset, guest_pc)
        || !exact_virtual_definition_use(loaded, 1, loaded_uses, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;
    Some(MatchedMemorySource {
        loaded,
        offset,
        address_offset,
        memory_size: memory_width.bytes(),
    })
}

fn masked_broadcast_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerMemoryShape,
    loaded_uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
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
    let condition = exact_nonzero_mask_predicate(
        block,
        index,
        &mut offset,
        guest_pc,
        mask,
        applicable_bits,
        virtual_definitions,
        virtual_uses,
    )?;
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
        if !same_pc(block, index, offset, guest_pc) {
            return None;
        }
        offset += 1;
        scalar
    };
    if !exact_virtual_definition_use(scalar, 2, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let address_offset = offset;
    let memory_width = element_memory_width(encoding.elem)?;
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
            && *width == memory_width
            && x86_jit_mem_address_shape_valid(addr)
    ) || !same_pc(block, index, offset, guest_pc)
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
    if !same_pc(block, index, offset, guest_pc)
        || !exact_virtual_definition_use(loaded, 1, loaded_uses, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;
    Some(MatchedMemorySource {
        loaded,
        offset,
        address_offset,
        memory_size: memory_width.bytes(),
    })
}

fn masked_vector_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerMemoryShape,
    loaded_uses: usize,
    broadcast: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
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
    if !single_definition_single_use(zero, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast_op = block.ops.get(index + 1)?;
    let loaded = match broadcast_op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: actual_lanes,
        } if broadcast_op.x86_hint.is_none()
            && scalar == zero
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };
    if !same_pc(block, index, 1, guest_pc)
        || !exact_virtual_definition_use(
            loaded,
            usize::from(lanes) + 1,
            usize::from(lanes) + loaded_uses,
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
        } if lea.x86_hint.is_none()
            && x86_jit_mem_address_shape_valid(addr)
            && addr.is_x86_state_backed_shape() =>
        {
            *base
        }
        _ => return None,
    };
    if !same_pc(block, index, address_offset, guest_pc)
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

    let memory_width = element_memory_width(encoding.elem)?;
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
        if !same_pc(block, index, offset, guest_pc)
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
                && *width == memory_width
                && exact_lane_address(
                    addr,
                    base,
                    if broadcast {
                        0
                    } else {
                        i64::from(lane) * i64::from(encoding.elem.bytes())
                    },
                )
        ) || !same_pc(block, index, offset, guest_pc)
        {
            return None;
        }
        offset += 1;

        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || !same_pc(block, index, offset, guest_pc)
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
        memory_size: if broadcast {
            memory_width.bytes()
        } else {
            encoding.width.bytes()
        },
    })
}

pub(super) fn exact_old_destination(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    encoding: X86EvexIntegerMemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<Option<VReg>> {
    let Some(op) = block.ops.get(index + *offset) else {
        return Some(None);
    };
    let OpKind::VMov { dst, src, width } = op.kind else {
        return Some(None);
    };
    if op.x86_hint.is_some()
        || op.guest_pc != guest_pc
        || vector_index(&src, encoding.width) != Some(encoding.destination)
        || width != encoding.width
    {
        return None;
    }
    let uses = if encoding.zeroing {
        0
    } else {
        encoding.width.lanes(encoding.elem) as usize
    };
    if !exact_virtual_definition_use(dst, 1, uses, virtual_definitions, virtual_uses) {
        return None;
    }
    *offset += 1;
    Some(Some(dst))
}

fn exact_arithmetic(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexIntegerArithmeticMemoryEncoding,
) -> Option<VReg> {
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let (dst, src1, src2, elem, actual_lanes, exact_kind) = match op.kind {
        OpKind::VAdd {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => (
            dst,
            src1,
            src2,
            elem,
            lanes,
            matches!(encoding.opcode, 0xD4 | 0xFC | 0xFD | 0xFE),
        ),
        OpKind::VSub {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => (
            dst,
            src1,
            src2,
            elem,
            lanes,
            matches!(encoding.opcode, 0xF8 | 0xF9 | 0xFA | 0xFB),
        ),
        OpKind::VAddSubSat {
            dst,
            src1,
            src2,
            elem,
            lanes,
            subtract,
            signed,
        } => {
            let exact = matches!(
                (encoding.opcode, subtract, signed),
                (0xD8 | 0xD9, true, false)
                    | (0xDC | 0xDD, false, false)
                    | (0xE8 | 0xE9, true, true)
                    | (0xEC | 0xED, false, true)
            );
            (dst, src1, src2, elem, lanes, exact)
        }
        OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op: VLaneOp::AvgRnd,
            signed: false,
            set_ovf: false,
        } => (
            dst,
            src1,
            src2,
            elem,
            lanes,
            matches!(
                (encoding.opcode, elem),
                (0xE0, VecElementType::I8) | (0xE3, VecElementType::I16)
            ),
        ),
        _ => return None,
    };
    let deferred_unmasked_commit =
        encoding.writemask.is_none() && matches!(encoding.opcode, 0xE0 | 0xE3);
    let expected_destination = if encoding.writemask.is_some() || deferred_unmasked_commit {
        matches!(dst, VReg::Virtual(_))
    } else {
        vector_index(&dst, encoding.width) == Some(encoding.destination)
    };
    let expected_hint = if matches!(encoding.opcode, 0xE0 | 0xE3) {
        None
    } else {
        Some(X86OpHint::EvexOp {
            map: encoding.map,
            pp: X86SsePrefix::OpSize,
            opcode: encoding.opcode,
            width: encoding.width,
            w: encoding.w,
        })
    };
    if !exact_kind
        || !expected_destination
        || vector_index(&src1, encoding.width) != Some(encoding.source1)
        || src2 != loaded
        || elem != encoding.elem
        || actual_lanes != lanes
        || op.x86_hint != expected_hint
    {
        return None;
    }
    Some(dst)
}

fn exact_dot_product(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexIntegerArithmeticMemoryEncoding,
) -> Option<()> {
    let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask,
        src_elem,
        acc_elem,
        width,
        src1_unsigned,
        saturate,
        zeroing,
    } = op.kind
    else {
        return None;
    };
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let expected_src_elem = if encoding.opcode < 0x52 {
        VecElementType::I8
    } else {
        VecElementType::I16
    };
    if !encoding.is_dot_product()
        || op.x86_hint.is_some()
        || dst != acc
        || vector_index(&dst, encoding.width) != Some(encoding.destination)
        || vector_index(&src1, encoding.width) != Some(encoding.source1)
        || src2 != loaded
        || mask != expected_mask
        || src_elem != expected_src_elem
        || acc_elem != VecElementType::I32
        || width != encoding.width
        || src1_unsigned != (encoding.opcode < 0x52)
        || saturate != (encoding.opcode & 1 != 0)
        || zeroing != encoding.zeroing
    {
        return None;
    }
    Some(())
}

fn exact_ifma52(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexIntegerArithmeticMemoryEncoding,
) -> Option<()> {
    let OpKind::VMultiplyAdd52 {
        dst,
        acc,
        src1,
        src2,
        mask,
        width,
        high,
        zeroing,
    } = op.kind
    else {
        return None;
    };
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    if !encoding.is_ifma52()
        || op.x86_hint.is_some()
        || dst != acc
        || vector_index(&dst, encoding.width) != Some(encoding.destination)
        || vector_index(&src1, encoding.width) != Some(encoding.source1)
        || src2 != loaded
        || mask != expected_mask
        || width != encoding.width
        || high != (encoding.opcode == 0xB5)
        || zeroing != encoding.zeroing
    {
        return None;
    }
    Some(())
}

fn exact_low_multiply(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexIntegerArithmeticMemoryEncoding,
) -> Option<(VReg, bool)> {
    let OpKind::VMul {
        dst,
        src1,
        src2,
        elem,
        lanes,
    } = op.kind
    else {
        return None;
    };
    let deferred_commit = encoding.writemask.is_some();
    let expected_destination = if deferred_commit {
        matches!(dst, VReg::Virtual(_))
    } else {
        vector_index(&dst, encoding.width) == Some(encoding.destination)
    };
    if !encoding.is_low_multiply()
        || !expected_destination
        || vector_index(&src1, encoding.width) != Some(encoding.source1)
        || src2 != loaded
        || elem != encoding.elem
        || lanes != encoding.width.lanes(encoding.elem) as u8
        || op.x86_hint
            != Some(X86OpHint::EvexOp {
                map: encoding.map,
                pp: X86SsePrefix::OpSize,
                opcode: encoding.opcode,
                width: encoding.width,
                w: encoding.w,
            })
    {
        return None;
    }
    Some((dst, deferred_commit))
}

fn exact_high_word_multiply(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexIntegerArithmeticMemoryEncoding,
) -> Option<VReg> {
    let (expected_signed, expected_round, expected_out_shift) =
        match (encoding.map, encoding.opcode) {
            (crate::smir::ir::ops::X86VecMap::Map0F, 0xE4) => (false, false, 16),
            (crate::smir::ir::ops::X86VecMap::Map0F, 0xE5) => (true, false, 16),
            (crate::smir::ir::ops::X86VecMap::Map0F38, 0x0B) => (true, true, 15),
            _ => return None,
        };
    let OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem,
        lanes,
        signed1,
        signed2,
        shift_left,
        round,
        sat_bits,
        out_shift,
    } = op.kind
    else {
        return None;
    };
    if op.x86_hint.is_some()
        || !matches!(dst, VReg::Virtual(_))
        || vector_index(&src1, encoding.width) != Some(encoding.source1)
        || src2 != loaded
        || src_elem != VecElementType::I16
        || encoding.elem != VecElementType::I16
        || lanes != encoding.width.lanes(VecElementType::I16) as u8
        || signed1 != expected_signed
        || signed2 != expected_signed
        || shift_left != 0
        || round != expected_round
        || sat_bits != 0
        || out_shift != expected_out_shift
    {
        return None;
    }
    Some(dst)
}

#[allow(clippy::too_many_arguments)]
fn exact_widening_dword_multiply(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    loaded: VReg,
    encoding: X86EvexIntegerArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let signed = match (encoding.map, encoding.opcode) {
        (crate::smir::ir::ops::X86VecMap::Map0F, 0xF4) => false,
        (crate::smir::ir::ops::X86VecMap::Map0F38, 0x28) => true,
        _ => return None,
    };
    if encoding.elem != VecElementType::I64 {
        return None;
    }
    let sign = if signed {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let qwords = encoding.width.lanes(VecElementType::I64) as usize;
    let exact_frontier = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == guest_pc && op.x86_hint.is_none())
    };
    let mut cursor = *offset;
    let mut virtuals = HashSet::from([loaded]);
    let mut products = Vec::with_capacity(qwords);
    for lane in 0..qwords {
        let source_lane = u8::try_from(lane * 2).ok()?;
        let lhs = match block.ops.get(index + cursor)?.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane,
                elem: VecElementType::I32,
                sign: actual_sign,
            } if vector_index(&vec, encoding.width) == Some(encoding.source1)
                && lane == source_lane
                && actual_sign == sign =>
            {
                dst
            }
            _ => return None,
        };
        if !exact_frontier(cursor)
            || !virtuals.insert(lhs)
            || !single_definition_single_use(lhs, virtual_definitions, virtual_uses)
        {
            return None;
        }
        cursor += 1;

        let rhs = match block.ops.get(index + cursor)?.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane,
                elem: VecElementType::I32,
                sign: actual_sign,
            } if vec == loaded && lane == source_lane && actual_sign == sign => dst,
            _ => return None,
        };
        if !exact_frontier(cursor)
            || !virtuals.insert(rhs)
            || !single_definition_single_use(rhs, virtual_definitions, virtual_uses)
        {
            return None;
        }
        cursor += 1;

        let product = match (&block.ops.get(index + cursor)?.kind, sign) {
            (
                OpKind::MulS {
                    dst_lo,
                    dst_hi: None,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                SignExtend::Sign,
            )
            | (
                OpKind::MulU {
                    dst_lo,
                    dst_hi: None,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                SignExtend::Zero,
            ) if *src1 == lhs && *src2 == rhs => *dst_lo,
            _ => return None,
        };
        if !exact_frontier(cursor)
            || !virtuals.insert(product)
            || !single_definition_single_use(product, virtual_definitions, virtual_uses)
        {
            return None;
        }
        products.push(product);
        cursor += 1;
    }

    let zero = match block.ops.get(index + cursor)?.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => dst,
        _ => return None,
    };
    if !exact_frontier(cursor)
        || !virtuals.insert(zero)
        || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }
    cursor += 1;

    let output = match block.ops.get(index + cursor)?.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I64,
            lanes,
        } if scalar == zero && usize::from(lanes) == qwords => dst,
        _ => return None,
    };
    if !exact_frontier(cursor)
        || !virtuals.insert(output)
        || !exact_virtual_definition_use(
            output,
            qwords + 1,
            qwords + 1,
            virtual_definitions,
            virtual_uses,
        )
    {
        return None;
    }
    cursor += 1;

    for (lane, product) in products.into_iter().enumerate() {
        if !matches!(
            block.ops.get(index + cursor)?.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: actual_lane,
                elem: VecElementType::I64,
            } if dst == output
                && vec == output
                && scalar == product
                && usize::from(actual_lane) == lane
        ) || !exact_frontier(cursor)
        {
            return None;
        }
        cursor += 1;
    }

    let raw = match block.ops.get(index + cursor)?.kind {
        OpKind::VMov { dst, src, width }
            if src == output && width == encoding.width && matches!(dst, VReg::Virtual(_)) =>
        {
            dst
        }
        _ => return None,
    };
    if !exact_frontier(cursor) || !virtuals.insert(raw) {
        return None;
    }
    cursor += 1;
    *offset = cursor;
    Some(raw)
}

fn exact_unmasked_result_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    encoding: X86EvexIntegerMemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    if !exact_virtual_definition_use(raw, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let commit = block.ops.get(index + *offset)?;
    if commit.x86_hint.is_some()
        || commit.guest_pc != guest_pc
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
    *offset += 1;
    Some(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn exact_mask_result_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    old: Option<VReg>,
    encoding: X86EvexIntegerMemoryShape,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    if !exact_virtual_definition_use(
        raw,
        1,
        usize::from(lanes) + 1,
        virtual_definitions,
        virtual_uses,
    ) {
        return None;
    }

    let zero = if matches!(
        block.ops.get(index + *offset).map(|op| &op.kind),
        Some(OpKind::Mov {
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
            ..
        })
    ) {
        let op = block.ops.get(index + *offset)?;
        let zero = match op.kind {
            OpKind::Mov { dst, .. } if op.x86_hint.is_none() => dst,
            _ => return None,
        };
        let uses = if encoding.zeroing {
            usize::from(lanes)
        } else {
            0
        };
        if op.guest_pc != guest_pc
            || !exact_virtual_definition_use(zero, 1, uses, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;
        Some(zero)
    } else {
        None
    };
    if encoding.zeroing && zero.is_none() {
        return None;
    }

    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lane_width = element_op_width(encoding.elem)?;
    for lane in 0..lanes {
        let inactive = if encoding.zeroing {
            zero?
        } else {
            let old = old?;
            let extract = block.ops.get(index + *offset)?;
            let scalar = match extract.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: actual_lane,
                    elem,
                    sign: SignExtend::Zero,
                } if extract.x86_hint.is_none()
                    && vec == old
                    && actual_lane == lane
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
            *offset += 1;
            scalar
        };

        let condition = exact_lane_predicate(
            block,
            index,
            offset,
            guest_pc,
            mask,
            lane,
            virtual_definitions,
            virtual_uses,
        )?;
        let active_op = block.ops.get(index + *offset)?;
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
                dst
            }
            _ => return None,
        };
        if active_op.guest_pc != guest_pc
            || !single_definition_single_use(active, virtual_definitions, virtual_uses)
        {
            return None;
        }
        *offset += 1;

        let select = block.ops.get(index + *offset)?;
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
                dst
            }
            _ => return None,
        };
        if select.guest_pc != guest_pc
            || !single_definition_single_use(selected, virtual_definitions, virtual_uses)
        {
            return None;
        }
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
                    lane: actual_lane,
                    elem,
                } if vector_index(&dst, encoding.width) == Some(encoding.destination)
                    && vec == if lane == 0 { raw } else { dst }
                    && scalar == selected
                    && actual_lane == lane
                    && elem == encoding.elem
            )
        {
            return None;
        }
        *offset += 1;
    }
    Some(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn matched_integer_memory_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerMemoryShape,
    replay: X86EvexIntegerArithmeticMemoryReplay,
    loaded_uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    match replay {
        X86EvexIntegerArithmeticMemoryReplay::Vector { .. } => unconditional_vector_source(
            block,
            index,
            encoding,
            loaded_uses,
            virtual_definitions,
            virtual_uses,
        ),
        X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. }
            if encoding.writemask.is_some() && encoding.masked_broadcast_uses_lane_graph =>
        {
            masked_vector_source(
                block,
                index,
                encoding,
                loaded_uses,
                true,
                virtual_definitions,
                virtual_uses,
            )
        }
        X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. } if encoding.writemask.is_some() => {
            masked_broadcast_source(
                block,
                index,
                encoding,
                loaded_uses,
                virtual_definitions,
                virtual_uses,
            )
        }
        X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. } => unconditional_broadcast_source(
            block,
            index,
            encoding,
            loaded_uses,
            virtual_definitions,
            virtual_uses,
        ),
        X86EvexIntegerArithmeticMemoryReplay::MaskedVector { .. } => masked_vector_source(
            block,
            index,
            encoding,
            loaded_uses,
            false,
            virtual_definitions,
            virtual_uses,
        ),
    }
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX packed
/// integer wrapping/saturating add/subtract, rounded-average, VNNI dot-product,
/// IFMA52 multiply-add, or low/high/widening multiply memory source.
///
/// Exact provenance binds the opcode, W/WIG interpretation, vector/element
/// width, architectural operands, mask policy, tuple kind, address, every
/// active-lane predicate, arithmetic semantics, and final commit. Runtime is
/// O(L) with O(L) auxiliary space for L <= 64 lanes; callers build global
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_integer_arithmetic_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexIntegerArithmeticMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_integer_arithmetic_memory_encoding()?;
    let shape = X86EvexIntegerMemoryShape::from(encoding);
    let loaded_uses = if encoding.is_widening_dword_multiply() {
        encoding.width.lanes(VecElementType::I64) as usize
    } else {
        1
    };
    let source = matched_integer_memory_source(
        block,
        index,
        shape,
        encoding.replay,
        loaded_uses,
        virtual_definitions,
        virtual_uses,
    )?;

    let mut offset = source.offset;
    let average = matches!(encoding.opcode, 0xE0 | 0xE3);
    if encoding.is_integer_multiply() {
        let (raw, deferred_commit) = if encoding.is_widening_dword_multiply() {
            (
                exact_widening_dword_multiply(
                    block,
                    index,
                    &mut offset,
                    guest_pc,
                    source.loaded,
                    encoding,
                    virtual_definitions,
                    virtual_uses,
                )?,
                true,
            )
        } else {
            let multiply = block.ops.get(index + offset)?;
            if multiply.guest_pc != guest_pc {
                return None;
            }
            let matched = if encoding.is_low_multiply() {
                exact_low_multiply(multiply, source.loaded, encoding)?
            } else {
                (
                    exact_high_word_multiply(multiply, source.loaded, encoding)?,
                    true,
                )
            };
            offset += 1;
            matched
        };
        if let Some(mask) = encoding.writemask {
            exact_evex_vector_mask_result(
                block,
                index,
                &mut offset,
                guest_pc,
                raw,
                VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
                encoding.width,
                encoding.elem,
                encoding.destination,
                encoding.zeroing,
                virtual_definitions,
                virtual_uses,
            )?;
        } else if deferred_commit {
            exact_unmasked_result_tail(
                block,
                index,
                &mut offset,
                guest_pc,
                raw,
                shape,
                virtual_definitions,
                virtual_uses,
            )?;
        }
    } else if encoding.is_ifma52() {
        let ifma52 = block.ops.get(index + offset)?;
        if ifma52.guest_pc != guest_pc {
            return None;
        }
        exact_ifma52(ifma52, source.loaded, encoding)?;
        offset += 1;
    } else if encoding.is_dot_product() {
        let dot_product = block.ops.get(index + offset)?;
        if dot_product.guest_pc != guest_pc {
            return None;
        }
        exact_dot_product(dot_product, source.loaded, encoding)?;
        offset += 1;
    } else if average {
        let arithmetic = block.ops.get(index + offset)?;
        let raw = exact_arithmetic(arithmetic, source.loaded, encoding)?;
        if arithmetic.guest_pc != guest_pc {
            return None;
        }
        offset += 1;
        if let Some(mask) = encoding.writemask {
            exact_evex_vector_mask_result(
                block,
                index,
                &mut offset,
                guest_pc,
                raw,
                VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
                encoding.width,
                encoding.elem,
                encoding.destination,
                encoding.zeroing,
                virtual_definitions,
                virtual_uses,
            )?;
        } else {
            exact_unmasked_result_tail(
                block,
                index,
                &mut offset,
                guest_pc,
                raw,
                shape,
                virtual_definitions,
                virtual_uses,
            )?;
        }
    } else {
        let old = if encoding.writemask.is_some() {
            exact_old_destination(
                block,
                index,
                &mut offset,
                guest_pc,
                shape,
                virtual_definitions,
                virtual_uses,
            )?
        } else {
            None
        };
        if encoding.writemask.is_some() && !encoding.zeroing && old.is_none() {
            return None;
        }

        let arithmetic = block.ops.get(index + offset)?;
        let raw = exact_arithmetic(arithmetic, source.loaded, encoding)?;
        if arithmetic.guest_pc != guest_pc {
            return None;
        }
        offset += 1;
        if encoding.writemask.is_some() {
            exact_mask_result_tail(
                block,
                index,
                &mut offset,
                guest_pc,
                raw,
                old,
                shape,
                virtual_definitions,
                virtual_uses,
            )?;
        }
    }
    if block
        .ops
        .get(index + offset)
        .is_some_and(|op| op.guest_pc == guest_pc)
    {
        return None;
    }

    Some(X86JitEvexIntegerArithmeticMemorySequence {
        consumed: offset,
        address_offset: source.address_offset,
        memory_size: source.memory_size,
        encoding,
    })
}
