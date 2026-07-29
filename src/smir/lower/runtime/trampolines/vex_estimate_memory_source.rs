//! Fail-closed helper-backed VEX reciprocal-estimate memory admission.

use std::collections::HashMap;

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecUnaryOp, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VRCPPS`/`VRCPSS`/`VRSQRTPS`/`VRSQRTSS`
/// memory-source decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexEstimateMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source1: Option<u8>,
    pub(crate) width: VecWidth,
    pub(crate) encoded_width: VecWidth,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
}

fn virtual_single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&1)
        && virtual_uses.get(&register) == Some(&1)
}

fn low_vex_vector_index(register: VReg, width: VecWidth) -> Option<u8> {
    match (register, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(index),
        _ => None,
    }
}

fn opcode_for_operation(operation: VecUnaryOp) -> Option<u8> {
    match operation {
        VecUnaryOp::FRecipEstimate => Some(0x53),
        VecUnaryOp::FRsqrtEstimate => Some(0x52),
        _ => None,
    }
}

fn packed_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexEstimateMemorySequence> {
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if !virtual_single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
        return None;
    }

    let unary = block.ops.get(index + 1)?;
    let (raw, opcode) = match unary.kind {
        OpKind::VUnary {
            dst,
            src,
            elem: VecElementType::F32,
            lanes,
            op,
        } if unary.x86_hint.is_none()
            && src == loaded
            && u32::from(lanes) == width.lanes(VecElementType::F32) =>
        {
            (dst, opcode_for_operation(op)?)
        }
        _ => return None,
    };
    if unary.guest_pc != load.guest_pc
        || !virtual_single_definition_single_use(raw, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let mov = block.ops.get(index + 2)?;
    let destination = match mov.kind {
        OpKind::VMov {
            dst,
            src,
            width: mov_width,
        } if mov.x86_hint.is_none() && src == raw && mov_width == width => {
            low_vex_vector_index(dst, width)?
        }
        _ => return None,
    };
    if mov.guest_pc != load.guest_pc
        || block
            .ops
            .get(index + 3)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        logical_width,
        encoded_width,
        memory_size,
        encoded_opcode,
        w,
    ) = instruction.vex_memory_fp_estimate_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        logical_width,
        encoded_width,
        memory_size,
        encoded_opcode,
    ) != (destination, None, width, width, width.bytes(), opcode)
    {
        return None;
    }

    Some(X86JitVexEstimateMemorySequence {
        consumed: 3,
        memory_size,
        destination,
        source1: None,
        width,
        encoded_width,
        opcode,
        w,
    })
}

fn scalar_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexEstimateMemorySequence> {
    let load = block.ops.get(index)?;
    let loaded_scalar = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *dst,
        _ => return None,
    };
    if !virtual_single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses) {
        return None;
    }
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    };

    let source_vector = match block.ops.get(index + 1)? {
        op if op.x86_hint.is_none() => match op.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: VecElementType::F32,
                lanes: 1,
            } if scalar == loaded_scalar => dst,
            _ => return None,
        },
        _ => return None,
    };
    if !same_pc(1)
        || !virtual_single_definition_single_use(source_vector, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let unary = block.ops.get(index + 2)?;
    let (raw, opcode) = match unary.kind {
        OpKind::VUnary {
            dst,
            src,
            elem: VecElementType::F32,
            lanes: 1,
            op,
        } if unary.x86_hint.is_none() && src == source_vector => (dst, opcode_for_operation(op)?),
        _ => return None,
    };
    if !same_pc(2) || !virtual_single_definition_single_use(raw, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let scalar_result = match block.ops.get(index + 3)? {
        op if op.x86_hint.is_none() => match op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem: VecElementType::F32,
                sign: SignExtend::Zero,
            } if vec == raw => dst,
            _ => return None,
        },
        _ => return None,
    };
    if !same_pc(3)
        || !virtual_single_definition_single_use(scalar_result, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let mut source1 = None;
    let mut upper_scalars = Vec::with_capacity(3);
    for lane in 1..4u8 {
        let offset = 3 + usize::from(lane);
        let upper_scalar = match block.ops.get(index + offset)? {
            op if op.x86_hint.is_none() => match op.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: extract_lane,
                    elem: VecElementType::F32,
                    sign: SignExtend::Zero,
                } if extract_lane == lane => {
                    if source1.is_some_and(|existing| existing != vec) {
                        return None;
                    }
                    source1 = Some(vec);
                    dst
                }
                _ => return None,
            },
            _ => return None,
        };
        if !same_pc(offset)
            || !virtual_single_definition_single_use(
                upper_scalar,
                virtual_definitions,
                virtual_uses,
            )
        {
            return None;
        }
        upper_scalars.push(upper_scalar);
    }
    let source1 = source1?;
    let source1_index = low_vex_vector_index(source1, VecWidth::V128)?;

    let zero_offset = 7;
    let zero = match block.ops.get(index + zero_offset)? {
        op if op.x86_hint.is_none() => match op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } => dst,
            _ => return None,
        },
        _ => return None,
    };
    if !same_pc(zero_offset)
        || !virtual_single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let clear_offset = 8;
    let destination = match block.ops.get(index + clear_offset)? {
        op if op.x86_hint.is_none() => match op.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: VecElementType::F32,
                lanes: 1,
            } if scalar == zero => dst,
            _ => return None,
        },
        _ => return None,
    };
    if !same_pc(clear_offset) {
        return None;
    }
    let destination_index = low_vex_vector_index(destination, VecWidth::V128)?;

    if !matches!(
        block.ops.get(index + 9),
        Some(op) if op.x86_hint.is_none()
            && matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: 0,
                    elem: VecElementType::F32,
                } if dst == destination && vec == destination && scalar == scalar_result
            )
    ) || !same_pc(9)
    {
        return None;
    }
    for (lane, upper_scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        let offset = 9 + lane;
        if !matches!(
            block.ops.get(index + offset),
            Some(op) if op.x86_hint.is_none()
                && matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst,
                        vec,
                        scalar,
                        lane: insert_lane,
                        elem: VecElementType::F32,
                    } if dst == destination
                        && vec == destination
                        && scalar == upper_scalar
                        && usize::from(insert_lane) == lane
                )
        ) || !same_pc(offset)
        {
            return None;
        }
    }

    const CONSUMED: usize = 13;
    if block
        .ops
        .get(index + CONSUMED)
        .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        logical_width,
        encoded_width,
        memory_size,
        encoded_opcode,
        w,
    ) = instruction.vex_memory_fp_estimate_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        logical_width,
        memory_size,
        encoded_opcode,
    ) != (
        destination_index,
        Some(source1_index),
        VecWidth::V128,
        4,
        opcode,
    ) {
        return None;
    }

    Some(X86JitVexEstimateMemorySequence {
        consumed: CONSUMED,
        memory_size,
        destination: destination_index,
        source1: Some(source1_index),
        width: VecWidth::V128,
        encoded_width,
        opcode,
        w,
    })
}

/// Validate one complete VEX reciprocal-estimate memory decomposition.
///
/// Packed forms are exact `VLoad`/`VUnary`/`VMov` triples. Scalar forms
/// include the complete low-lane estimate, VEX.vvvv upper-lane merge, and VEX
/// destination clearing chain. Every hidden virtual is single-definition and
/// single-use, every operation is contiguous at one guest PC, and exact source
/// bytes bind all architectural operands, widths, ignored L/W fields,
/// reserved fields, and memory footprints.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_estimate_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexEstimateMemorySequence> {
    if !allow_mem {
        return None;
    }
    packed_sequence(
        block,
        index,
        instruction_bytes,
        virtual_definitions,
        virtual_uses,
    )
    .or_else(|| {
        scalar_sequence(
            block,
            index,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        )
    })
}
