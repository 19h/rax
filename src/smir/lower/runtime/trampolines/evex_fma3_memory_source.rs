//! Fail-closed helper-backed EVEX packed/scalar FMA3 memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedFma3MemoryEncoding, X86EvexScalarFma3MemoryEncoding, X86InstructionBytes,
};

use super::vector_memory_source::{vex_fma3_kind, vex_fma3_order};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous unmasked, non-broadcast EVEX packed FMA3 memory-source
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFma3MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFma3MemoryEncoding,
}

/// Exact contiguous EVEX scalar FMA3 memory-source decomposition consumed by
/// the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarFma3MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) load_offset: usize,
    pub(crate) memory_width: MemWidth,
    pub(crate) encoding: X86EvexScalarFma3MemoryEncoding,
}

fn vector_index(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=31))), VecWidth::V256)
        | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(index @ 0..=31))), VecWidth::V512) => Some(*index),
        _ => None,
    }
}

fn xmm_index(reg: &VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=31))) => Some(*index),
        _ => None,
    }
}

fn single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&1)
        && virtual_uses.get(&register) == Some(&1)
}

fn exact_virtual_definition_use(
    register: VReg,
    definitions: usize,
    uses: usize,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&definitions)
        && virtual_uses.get(&register) == Some(&uses)
}

/// Validate the complete three-op decomposition emitted for one unmasked,
/// non-broadcast EVEX packed FMA3 memory source. Exact instruction provenance
/// binds vector width, element type, architectural operands, opcode semantics,
/// and the native register-source rewrite. Both virtual results must have
/// exactly one definition and one use in the complete block.
///
/// Classification is O(1) time and O(1) auxiliary space. Callers build the
/// global definition/use maps once in O(N) time and O(V) space for N operations
/// and V virtual registers.
pub(crate) fn x86_jit_evex_packed_fma3_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFma3MemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
        return None;
    }

    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .evex_packed_fma3_memory_encoding()?;
    if encoding.width != width {
        return None;
    }
    let elem = encoding.elem;

    let fma = block.ops.get(index + 1)?;
    let (raw, src1, src2, src3, mask, kind, order, round, lanes) = match &fma.kind {
        OpKind::X86Fma(fma_op) if elem != VecElementType::F16 => (
            fma_op.dst,
            fma_op.src1,
            fma_op.src2,
            fma_op.src3,
            fma_op.mask,
            fma_op.kind,
            fma_op.order,
            fma_op.round,
            fma_op.lanes,
        ),
        OpKind::X86FP16Fma {
            dst,
            src1,
            src2,
            src3,
            mask,
            kind,
            order,
            round,
            lanes,
        } if elem == VecElementType::F16 => (
            *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
        ),
        _ => return None,
    };
    if fma.guest_pc != load.guest_pc
        || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        || vector_index(&src1, width) != Some(encoding.destination)
        || vector_index(&src2, width) != Some(encoding.source1)
        || src3 != loaded
        || mask.is_some()
        || kind != vex_fma3_kind(encoding.opcode)?
        || order != vex_fma3_order(encoding.opcode)?
        || round != FpRoundMode::Dynamic
        || lanes != width.lanes(elem) as u8
        || fma.x86_hint
            != Some(X86OpHint::EvexOp {
                map: if elem == VecElementType::F16 {
                    X86VecMap::Map6
                } else {
                    X86VecMap::Map0F38
                },
                pp: X86SsePrefix::OpSize,
                opcode: encoding.opcode,
                width,
                w: encoding.w,
            })
    {
        return None;
    }

    let result = block.ops.get(index + 2)?;
    if result.guest_pc != load.guest_pc
        || result.x86_hint.is_some()
        || !matches!(
            result.kind,
            OpKind::VMov {
                dst,
                src,
                width: result_width,
            } if vector_index(&dst, width) == Some(encoding.destination)
                && src == raw
                && result_width == width
        )
        || block
            .ops
            .get(index + 3)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    Some(X86JitEvexPackedFma3MemorySequence {
        consumed: 3,
        memory_size: width.bytes(),
        encoding,
    })
}

fn x86_jit_evex_scalar_fma3_result_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    guest_pc: GuestAddr,
    tail_offset: usize,
    scalar_result: VReg,
    upper_source: VReg,
    elem: VecElementType,
    encoding: X86EvexScalarFma3MemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == guest_pc)
    };
    let xmm_lanes = VecWidth::V128.lanes(elem) as usize;
    let mut upper_scalars = Vec::with_capacity(xmm_lanes - 1);
    for lane in 1..xmm_lanes {
        let offset = tail_offset + lane - 1;
        let extract = block.ops.get(index + offset)?;
        let upper_scalar = match &extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extract_lane,
                elem: extract_elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && *vec == upper_source
                && usize::from(*extract_lane) == lane
                && *extract_elem == elem =>
            {
                *dst
            }
            _ => return None,
        };
        if !same_pc(offset)
            || !single_definition_single_use(upper_scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        upper_scalars.push(upper_scalar);
    }

    let zero_offset = tail_offset + xmm_lanes - 1;
    let zero_op = block.ops.get(index + zero_offset)?;
    let zero = match &zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => *dst,
        _ => return None,
    };
    if !same_pc(zero_offset)
        || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let clear_offset = zero_offset + 1;
    let clear = block.ops.get(index + clear_offset)?;
    if clear.x86_hint.is_some()
        || !matches!(
            &clear.kind,
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: broadcast_elem,
                lanes: 1,
            } if xmm_index(dst) == Some(encoding.destination)
                && *scalar == zero
                && *broadcast_elem == elem
        )
        || !same_pc(clear_offset)
    {
        return None;
    }

    let low_insert_offset = clear_offset + 1;
    let low_insert = block.ops.get(index + low_insert_offset)?;
    if low_insert.x86_hint.is_some()
        || !matches!(
            &low_insert.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: 0,
                elem: insert_elem,
            } if xmm_index(dst) == Some(encoding.destination)
                && dst == vec
                && *scalar == scalar_result
                && *insert_elem == elem
        )
        || !same_pc(low_insert_offset)
    {
        return None;
    }
    for (lane, upper_scalar) in upper_scalars.into_iter().enumerate() {
        let lane = lane + 1;
        let offset = low_insert_offset + lane;
        let insert = block.ops.get(index + offset)?;
        if insert.x86_hint.is_some()
            || !matches!(
                &insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane: insert_lane,
                    elem: insert_elem,
                } if xmm_index(dst) == Some(encoding.destination)
                    && dst == vec
                    && *scalar == upper_scalar
                    && usize::from(*insert_lane) == lane
                    && *insert_elem == elem
            )
            || !same_pc(offset)
        {
            return None;
        }
    }

    let consumed = low_insert_offset + xmm_lanes;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == guest_pc)
    {
        return None;
    }
    Some(consumed)
}

fn x86_jit_unmasked_evex_scalar_fma3_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarFma3MemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFma3MemorySequence> {
    if encoding.writemask.is_some() || encoding.zeroing {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded_scalar, memory_width, elem) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B2,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => {
            (*dst, MemWidth::B2, VecElementType::F16)
        }
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => {
            (*dst, MemWidth::B4, VecElementType::F32)
        }
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => {
            (*dst, MemWidth::B8, VecElementType::F64)
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses) {
        return None;
    }
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    };

    let broadcast = block.ops.get(index + 1)?;
    let source_vector = match &broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } if broadcast.x86_hint.is_none()
            && *scalar == loaded_scalar
            && *broadcast_elem == elem =>
        {
            *dst
        }
        _ => return None,
    };
    if !same_pc(1)
        || !single_definition_single_use(source_vector, virtual_definitions, virtual_uses)
    {
        return None;
    }

    if encoding.elem != elem {
        return None;
    }

    let fma = block.ops.get(index + 2)?;
    let (raw, src1, src2, src3, mask, kind, order, round, lanes) = match &fma.kind {
        OpKind::X86Fma(fma_op) if elem != VecElementType::F16 && fma_op.elem == elem => (
            fma_op.dst,
            fma_op.src1,
            fma_op.src2,
            fma_op.src3,
            fma_op.mask,
            fma_op.kind,
            fma_op.order,
            fma_op.round,
            fma_op.lanes,
        ),
        OpKind::X86FP16Fma {
            dst,
            src1,
            src2,
            src3,
            mask,
            kind,
            order,
            round,
            lanes,
        } if elem == VecElementType::F16 => (
            *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
        ),
        _ => return None,
    };
    if !same_pc(2)
        || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        || xmm_index(&src1) != Some(encoding.destination)
        || xmm_index(&src2) != Some(encoding.source1)
        || src3 != source_vector
        || mask.is_some()
        || kind != vex_fma3_kind(encoding.opcode)?
        || order != vex_fma3_order(encoding.opcode)?
        || round != FpRoundMode::Dynamic
        || lanes != 1
        || fma.x86_hint
            != Some(X86OpHint::EvexOp {
                map: if elem == VecElementType::F16 {
                    X86VecMap::Map6
                } else {
                    X86VecMap::Map0F38
                },
                pp: X86SsePrefix::OpSize,
                opcode: encoding.opcode,
                width: encoding.hint_width,
                w: encoding.w,
            })
    {
        return None;
    }

    let result_extract = block.ops.get(index + 3)?;
    let scalar_result = match &result_extract.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: extract_elem,
            sign: SignExtend::Zero,
        } if result_extract.x86_hint.is_none() && *vec == raw && *extract_elem == elem => *dst,
        _ => return None,
    };
    if !same_pc(3)
        || !single_definition_single_use(scalar_result, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let consumed = x86_jit_evex_scalar_fma3_result_tail(
        block,
        index,
        load.guest_pc,
        4,
        scalar_result,
        src1,
        elem,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;

    Some(X86JitEvexScalarFma3MemorySequence {
        consumed,
        load_offset: 0,
        memory_width,
        encoding,
    })
}

fn x86_jit_masked_evex_scalar_fma3_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarFma3MemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFma3MemorySequence> {
    let mask_index = encoding.writemask?;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(mask_index)));
    let condition_op = block.ops.get(index)?;
    let condition = match &condition_op.kind {
        OpKind::And {
            dst,
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if condition_op.x86_hint.is_none() && *src1 == mask => *dst,
        _ => return None,
    };
    if !exact_virtual_definition_use(condition, 1, 2, virtual_definitions, virtual_uses) {
        return None;
    }
    let same_pc = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .is_some_and(|op| op.guest_pc == condition_op.guest_pc)
    };

    let seed = block.ops.get(index + 1)?;
    let loaded_scalar = match &seed.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if seed.x86_hint.is_none() => *dst,
        _ => return None,
    };
    if !same_pc(1)
        || !exact_virtual_definition_use(loaded_scalar, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let pred_load = block.ops.get(index + 2)?;
    let (memory_width, elem) = match &pred_load.kind {
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width: MemWidth::B2,
            signed: SignExtend::Zero,
        } if pred_load.x86_hint.is_none()
            && *dst == loaded_scalar
            && *cond == condition
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            (MemWidth::B2, VecElementType::F16)
        }
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width: MemWidth::B4,
            signed: SignExtend::Zero,
        } if pred_load.x86_hint.is_none()
            && *dst == loaded_scalar
            && *cond == condition
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            (MemWidth::B4, VecElementType::F32)
        }
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        } if pred_load.x86_hint.is_none()
            && *dst == loaded_scalar
            && *cond == condition
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            (MemWidth::B8, VecElementType::F64)
        }
        _ => return None,
    };
    if !same_pc(2) || encoding.elem != elem {
        return None;
    }

    let broadcast = block.ops.get(index + 3)?;
    let source_vector = match &broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } if broadcast.x86_hint.is_none()
            && *scalar == loaded_scalar
            && *broadcast_elem == elem =>
        {
            *dst
        }
        _ => return None,
    };
    if !same_pc(3)
        || !single_definition_single_use(source_vector, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let fma = block.ops.get(index + 4)?;
    let (raw, src1, src2, src3, fma_mask, kind, order, round, lanes) = match &fma.kind {
        OpKind::X86Fma(fma_op) if elem != VecElementType::F16 && fma_op.elem == elem => (
            fma_op.dst,
            fma_op.src1,
            fma_op.src2,
            fma_op.src3,
            fma_op.mask,
            fma_op.kind,
            fma_op.order,
            fma_op.round,
            fma_op.lanes,
        ),
        OpKind::X86FP16Fma {
            dst,
            src1,
            src2,
            src3,
            mask,
            kind,
            order,
            round,
            lanes,
        } if elem == VecElementType::F16 => (
            *dst, *src1, *src2, *src3, *mask, *kind, *order, *round, *lanes,
        ),
        _ => return None,
    };
    if !same_pc(4)
        || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        || xmm_index(&src1) != Some(encoding.destination)
        || xmm_index(&src2) != Some(encoding.source1)
        || src3 != source_vector
        || fma_mask != Some(mask)
        || kind != vex_fma3_kind(encoding.opcode)?
        || order != vex_fma3_order(encoding.opcode)?
        || round != FpRoundMode::Dynamic
        || lanes != 1
        || fma.x86_hint
            != Some(X86OpHint::EvexOp {
                map: if elem == VecElementType::F16 {
                    X86VecMap::Map6
                } else {
                    X86VecMap::Map0F38
                },
                pp: X86SsePrefix::OpSize,
                opcode: encoding.opcode,
                width: encoding.hint_width,
                w: encoding.w,
            })
    {
        return None;
    }

    let result_extract = block.ops.get(index + 5)?;
    let scalar_result = match &result_extract.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: extract_elem,
            sign: SignExtend::Zero,
        } if result_extract.x86_hint.is_none() && *vec == raw && *extract_elem == elem => *dst,
        _ => return None,
    };
    if !same_pc(5)
        || !single_definition_single_use(scalar_result, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let fallback_op = block.ops.get(index + 6)?;
    let fallback = if encoding.zeroing {
        let width = match elem {
            VecElementType::F16 => OpWidth::W16,
            VecElementType::F32 => OpWidth::W32,
            VecElementType::F64 => OpWidth::W64,
            _ => unreachable!("validated scalar EVEX FMA3 element"),
        };
        match &fallback_op.kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0),
                width: fallback_width,
            } if fallback_op.x86_hint.is_none() && *fallback_width == width => *dst,
            _ => return None,
        }
    } else {
        match &fallback_op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: 0,
                elem: fallback_elem,
                sign: SignExtend::Zero,
            } if fallback_op.x86_hint.is_none()
                && xmm_index(vec) == Some(encoding.destination)
                && *fallback_elem == elem =>
            {
                *dst
            }
            _ => return None,
        }
    };
    if !same_pc(6) || !single_definition_single_use(fallback, virtual_definitions, virtual_uses) {
        return None;
    }

    let select = block.ops.get(index + 7)?;
    let selected = match &select.kind {
        OpKind::Select {
            dst,
            cond,
            src_true,
            src_false,
            width,
        } if select.x86_hint.is_none()
            && *cond == condition
            && *src_true == scalar_result
            && *src_false == fallback
            && *width
                == match elem {
                    VecElementType::F16 => OpWidth::W16,
                    VecElementType::F32 => OpWidth::W32,
                    VecElementType::F64 => OpWidth::W64,
                    _ => unreachable!("validated scalar EVEX FMA3 element"),
                } =>
        {
            *dst
        }
        _ => return None,
    };
    if !same_pc(7) || !single_definition_single_use(selected, virtual_definitions, virtual_uses) {
        return None;
    }

    let consumed = x86_jit_evex_scalar_fma3_result_tail(
        block,
        index,
        condition_op.guest_pc,
        8,
        selected,
        src1,
        elem,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexScalarFma3MemorySequence {
        consumed,
        load_offset: 2,
        memory_width,
        encoding,
    })
}

/// Validate the complete scalar FMA3 decomposition emitted for one unmasked
/// or writemasked EVEX memory source. Exact instruction provenance binds the
/// LLIG hint, element type, architectural operands, opcode and mask semantics,
/// and a normalized host-stack memory rewrite. Virtual definition/use counts
/// are checked for the complete block, including the two uses of the scalar
/// mask condition and the seed/conditional definitions of the loaded scalar.
///
/// Classification is O(1) time and O(1) auxiliary space because a scalar XMM
/// has at most eight binary16 lanes. Callers build the global definition/use
/// maps once in O(N) time and O(V) space for N operations and V virtual
/// registers.
pub(crate) fn x86_jit_evex_scalar_fma3_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFma3MemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_scalar_fma3_memory_encoding()?;
    if encoding.writemask.is_some() {
        x86_jit_masked_evex_scalar_fma3_memory_sequence(
            block,
            index,
            encoding,
            virtual_definitions,
            virtual_uses,
        )
    } else {
        x86_jit_unmasked_evex_scalar_fma3_memory_sequence(
            block,
            index,
            encoding,
            virtual_definitions,
            virtual_uses,
        )
    }
}
