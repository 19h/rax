//! Fail-closed helper-backed EVEX AVX512_BF16 memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexBf16MemoryEncoding, X86EvexBf16MemoryKind, X86EvexBf16MemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_lane_address, exact_lane_predicate, exact_nonzero_mask_predicate,
    exact_virtual_definition_use, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// AVX512_BF16 memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexBf16MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexBf16MemoryEncoding,
}

#[derive(Clone, Copy)]
struct MatchedMemorySource {
    loaded: VReg,
    offset: usize,
    address_offset: usize,
    memory_size: u32,
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

fn unconditional_vector_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexBf16MemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
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
    if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
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
    encoding: X86EvexBf16MemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let load = block.ops.get(index)?;
    let scalar = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *dst,
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
            elem: VecElementType::F32,
            lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && lanes == encoding.width.lanes(VecElementType::F32) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != load.guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    Some(MatchedMemorySource {
        loaded,
        offset: 2,
        address_offset: 0,
        memory_size: MemWidth::B4.bytes(),
    })
}

fn masked_broadcast_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexBf16MemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    if encoding.kind == X86EvexBf16MemoryKind::ConvertTwo {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(VecElementType::F32) as u8;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let mut offset = 0usize;
    let condition = exact_nonzero_mask_predicate(
        block,
        index,
        &mut offset,
        guest_pc,
        mask,
        (1u64 << lanes) - 1,
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
    let load = block.ops.get(index + offset)?;
    if !matches!(
        &load.kind,
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width: MemWidth::B4,
            signed: SignExtend::Zero,
        } if load.x86_hint.is_none()
            && *dst == scalar
            && *cond == condition
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
            elem: VecElementType::F32,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none() && actual_scalar == scalar && actual_lanes == lanes => {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;
    Some(MatchedMemorySource {
        loaded,
        offset,
        address_offset,
        memory_size: MemWidth::B4.bytes(),
    })
}

fn masked_vector_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexBf16MemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    if encoding.kind == X86EvexBf16MemoryKind::ConvertTwo {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(VecElementType::F32) as u8;
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
            elem: VecElementType::F32,
            lanes: actual_lanes,
        } if broadcast.x86_hint.is_none() && scalar == zero && actual_lanes == lanes => dst,
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(
            loaded,
            usize::from(lanes) + 1,
            usize::from(lanes) + 1,
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
                width: MemWidth::B4,
                signed: SignExtend::Zero,
            } if load.x86_hint.is_none()
                && *dst == scalar
                && *cond == condition
                && exact_lane_address(addr, base, i64::from(lane) * 4)
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
                    elem: VecElementType::F32,
                } if dst == loaded
                    && vec == loaded
                    && actual_scalar == scalar
                    && actual_lane == lane
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

fn exact_bf16_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    source: MatchedMemorySource,
    encoding: X86EvexBf16MemoryEncoding,
) -> Option<usize> {
    let first = block.ops.get(index)?;
    let result = block.ops.get(index + source.offset)?;
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    let exact = match result.kind {
        OpKind::VCvtFP32ToBF16 {
            dst,
            src1,
            src2: None,
            mask,
            width,
            zeroing,
        } => {
            let output_width = match encoding.width {
                VecWidth::V128 | VecWidth::V256 => VecWidth::V128,
                VecWidth::V512 => VecWidth::V256,
                VecWidth::V64 => return None,
            };
            encoding.kind == X86EvexBf16MemoryKind::ConvertOne
                && vector_index(&dst, output_width) == Some(encoding.destination)
                && src1 == source.loaded
                && mask == expected_mask
                && width == encoding.width
                && zeroing == encoding.zeroing
        }
        OpKind::VCvtFP32ToBF16 {
            dst,
            src1,
            src2: Some(src2),
            mask,
            width,
            zeroing,
        } => {
            encoding.kind == X86EvexBf16MemoryKind::ConvertTwo
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && vector_index(&src1, encoding.width) == Some(encoding.source1)
                && src2 == source.loaded
                && mask == expected_mask
                && width == encoding.width
                && zeroing == encoding.zeroing
        }
        OpKind::VDotProductBF16 {
            dst,
            acc,
            src1,
            src2,
            mask,
            width,
            zeroing,
        } => {
            encoding.kind == X86EvexBf16MemoryKind::DotProduct
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && acc == dst
                && vector_index(&src1, encoding.width) == Some(encoding.source1)
                && src2 == source.loaded
                && mask == expected_mask
                && width == encoding.width
                && zeroing == encoding.zeroing
        }
        _ => false,
    };
    if !exact || result.x86_hint.is_some() || result.guest_pc != first.guest_pc {
        return None;
    }
    let consumed = source.offset + 1;
    no_following_same_pc(block, index, consumed, first.guest_pc).then_some(consumed)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VCVTNEPS2BF16, VCVTNE2PS2BF16, or VDPBF16PS memory source.
///
/// Exact provenance binds the operation, widths, architectural operands,
/// writemask and zeroing policy, broadcast/full-vector tuple, Type E4/E4NF
/// helper-access graph, and the single architectural operation. Classification
/// is O(L) time and O(1) auxiliary space for L <= 16 result lanes; callers
/// build definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_bf16_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBf16MemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_bf16_memory_encoding()?;
    let source = match encoding.replay {
        X86EvexBf16MemoryReplay::Vector { .. } => {
            unconditional_vector_source(block, index, encoding, virtual_definitions, virtual_uses)?
        }
        X86EvexBf16MemoryReplay::Broadcast { .. }
            if encoding.kind != X86EvexBf16MemoryKind::ConvertTwo
                && encoding.writemask.is_some() =>
        {
            masked_broadcast_source(block, index, encoding, virtual_definitions, virtual_uses)?
        }
        X86EvexBf16MemoryReplay::Broadcast { .. } => unconditional_broadcast_source(
            block,
            index,
            encoding,
            virtual_definitions,
            virtual_uses,
        )?,
        X86EvexBf16MemoryReplay::MaskedVector { .. } => {
            masked_vector_source(block, index, encoding, virtual_definitions, virtual_uses)?
        }
    };
    let consumed = exact_bf16_result(block, index, source, encoding)?;
    Some(X86JitEvexBf16MemorySequence {
        consumed,
        address_offset: source.address_offset,
        memory_size: source.memory_size,
        encoding,
    })
}
