//! Fail-closed helper-backed VEX floating-point square-root memory admission.

use std::collections::HashMap;

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VSQRTPS`/`VSQRTPD`/`VSQRTSS`/`VSQRTSD`
/// memory-source decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexSqrtMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source1: Option<u8>,
    pub(crate) elem: VecElementType,
    pub(crate) width: VecWidth,
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

fn packed_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexSqrtMemorySequence> {
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

    let sqrt = block.ops.get(index + 1)?;
    if sqrt.guest_pc != load.guest_pc
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::X86Sqrt {
        dst,
        src,
        elem,
        lanes,
        round: FpRoundMode::Dynamic,
        suppress_exceptions: false,
    } = sqrt.kind
    else {
        return None;
    };
    if src != loaded
        || !matches!(elem, VecElementType::F32 | VecElementType::F64)
        || u32::from(lanes) != width.lanes(elem)
    {
        return None;
    }
    let destination = low_vex_vector_index(dst, width)?;
    let prefix = if elem == VecElementType::F32 {
        X86SsePrefix::None
    } else {
        X86SsePrefix::OpSize
    };
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: hinted_prefix,
        opcode: 0x51,
        width: hinted_width,
        w,
    }) = sqrt.x86_hint
    else {
        return None;
    };
    if (hinted_prefix, hinted_width) != (prefix, width) {
        return None;
    }

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_width, memory_size, encoded_w) =
        instruction.vex_memory_fp_sqrt_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
        memory_size,
        encoded_w,
    ) != (destination, None, elem, width, width.bytes(), w)
    {
        return None;
    }

    Some(X86JitVexSqrtMemorySequence {
        consumed: 2,
        memory_size,
        destination,
        source1: None,
        elem,
        width,
        w,
    })
}

fn scalar_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexSqrtMemorySequence> {
    let load = block.ops.get(index)?;
    let (loaded_scalar, memory_size, elem) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => {
            (*dst, 4, VecElementType::F32)
        }
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => {
            (*dst, 8, VecElementType::F64)
        }
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
                elem: broadcast_elem,
                lanes: 1,
            } if scalar == loaded_scalar && broadcast_elem == elem => dst,
            _ => return None,
        },
        _ => return None,
    };
    if !same_pc(1)
        || !virtual_single_definition_single_use(source_vector, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let sqrt = block.ops.get(index + 2)?;
    let sqrt_result = match sqrt.kind {
        OpKind::X86Sqrt {
            dst,
            src,
            elem: sqrt_elem,
            lanes: 1,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        } if src == source_vector && sqrt_elem == elem => dst,
        _ => return None,
    };
    let prefix = if elem == VecElementType::F32 {
        X86SsePrefix::Rep
    } else {
        X86SsePrefix::Repne
    };
    let Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: hinted_prefix,
        opcode: 0x51,
        width: VecWidth::V128,
        w,
    }) = sqrt.x86_hint
    else {
        return None;
    };
    if !same_pc(2)
        || hinted_prefix != prefix
        || !virtual_single_definition_single_use(sqrt_result, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let scalar_result = match block.ops.get(index + 3)? {
        op if op.x86_hint.is_none() => match op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } if vec == sqrt_result && extract_elem == elem => dst,
            _ => return None,
        },
        _ => return None,
    };
    if !same_pc(3)
        || !virtual_single_definition_single_use(scalar_result, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let xmm_lanes = u8::try_from(VecWidth::V128.lanes(elem)).ok()?;
    let mut source1 = None;
    let mut upper_scalars = Vec::with_capacity(usize::from(xmm_lanes - 1));
    for lane in 1..xmm_lanes {
        let offset = 3 + usize::from(lane);
        let upper_scalar = match block.ops.get(index + offset)? {
            op if op.x86_hint.is_none() => match op.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: extract_lane,
                    elem: extract_elem,
                    sign: SignExtend::Zero,
                } if extract_lane == lane && extract_elem == elem => {
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

    let zero_offset = 3 + usize::from(xmm_lanes);
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

    let clear_offset = zero_offset + 1;
    let destination = match block.ops.get(index + clear_offset)? {
        op if op.x86_hint.is_none() => match op.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: broadcast_elem,
                lanes: 1,
            } if scalar == zero && broadcast_elem == elem => dst,
            _ => return None,
        },
        _ => return None,
    };
    if !same_pc(clear_offset) {
        return None;
    }
    let destination_index = low_vex_vector_index(destination, VecWidth::V128)?;

    let low_insert_offset = clear_offset + 1;
    if !matches!(
        block.ops.get(index + low_insert_offset),
        Some(op) if op.x86_hint.is_none()
            && matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: 0,
                    elem: insert_elem,
                } if dst == destination
                    && vec == destination
                    && scalar == scalar_result
                    && insert_elem == elem
            )
    ) || !same_pc(low_insert_offset)
    {
        return None;
    }
    for (lane, upper_scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        let offset = low_insert_offset + lane;
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
                        elem: insert_elem,
                    } if dst == destination
                        && vec == destination
                        && scalar == upper_scalar
                        && usize::from(insert_lane) == lane
                        && insert_elem == elem
                )
        ) || !same_pc(offset)
        {
            return None;
        }
    }

    let consumed = low_insert_offset + usize::from(xmm_lanes);
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
        encoded_memory_size,
        encoded_w,
    ) = instruction.vex_memory_fp_sqrt_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
        encoded_memory_size,
        encoded_w,
    ) != (
        destination_index,
        Some(source1_index),
        elem,
        VecWidth::V128,
        memory_size,
        w,
    ) {
        return None;
    }

    Some(X86JitVexSqrtMemorySequence {
        consumed,
        memory_size,
        destination: destination_index,
        source1: Some(source1_index),
        elem,
        width: VecWidth::V128,
        w,
    })
}

/// Validate one complete VEX square-root memory-source decomposition.
///
/// Packed forms are exact two-op `VLoad`/`X86Sqrt` pairs. Scalar forms include
/// the complete low-lane computation, VEX.vvvv upper-lane merge, and VEX
/// destination clearing chain. Every hidden virtual is single-definition and
/// single-use, every operation is contiguous at one guest PC, and exact source
/// bytes bind all architectural operands, widths, prefixes, WIG, reserved
/// fields, and the deterministic scalar VEX.L=0 frontier.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_sqrt_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexSqrtMemorySequence> {
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
