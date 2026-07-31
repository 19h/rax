//! Fail-closed helper-backed EVEX packed binary32/binary64 memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, X86FpBinaryOp, X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedFpArithmeticMemoryEncoding, X86EvexPackedFpArithmeticMemoryReplay,
    X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, exact_lane_address, exact_lane_predicate,
    exact_nonzero_mask_predicate, exact_virtual_definition_use, single_definition_single_use,
    vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// packed binary32/binary64 arithmetic memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFpArithmeticMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFpArithmeticMemoryEncoding,
}

fn operation(opcode: u8) -> Option<X86FpBinaryOp> {
    Some(match opcode {
        0x58 => X86FpBinaryOp::Add,
        0x59 => X86FpBinaryOp::Mul,
        0x5C => X86FpBinaryOp::Sub,
        0x5D => X86FpBinaryOp::Min,
        0x5E => X86FpBinaryOp::Div,
        0x5F => X86FpBinaryOp::Max,
        _ => return None,
    })
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::F32 => Some(MemWidth::B4),
        VecElementType::F64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn exact_binary(
    op: &crate::smir::ir::ops::SmirOp,
    source2: VReg,
    encoding: X86EvexPackedFpArithmeticMemoryEncoding,
) -> Option<VReg> {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let expected_prefix = match encoding.elem {
        VecElementType::F32 => X86SsePrefix::None,
        VecElementType::F64 => X86SsePrefix::OpSize,
        _ => return None,
    };
    match op.kind {
        OpKind::X86FpBinary {
            dst,
            src1,
            src2,
            mask,
            elem,
            lanes,
            op: actual_op,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        } if vector_index(&src1, encoding.width) == Some(encoding.source1)
            && src2 == source2
            && mask == expected_mask
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8
            && actual_op == operation(encoding.opcode)?
            && op.x86_hint
                == Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: expected_prefix,
                    opcode: encoding.opcode,
                    width: encoding.width,
                    w: encoding.elem == VecElementType::F64,
                }) =>
        {
            Some(dst)
        }
        _ => None,
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

fn exact_unmasked_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: usize,
    guest_pc: GuestAddr,
    raw: VReg,
    encoding: X86EvexPackedFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    if !single_definition_single_use(raw, virtual_definitions, virtual_uses) {
        return None;
    }
    let result = block.ops.get(index + offset)?;
    if result.guest_pc != guest_pc
        || result.x86_hint.is_some()
        || !matches!(
            result.kind,
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
    let consumed = offset + 1;
    no_following_same_pc(block, index, consumed, guest_pc).then_some(consumed)
}

fn unmasked_vector_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFpArithmeticMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFpArithmeticMemoryReplay::Vector { .. }
    ) || encoding.writemask.is_some()
        || encoding.zeroing
    {
        return None;
    }
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
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
    let binary = block.ops.get(index + 1)?;
    if binary.guest_pc != load.guest_pc {
        return None;
    }
    let raw = exact_binary(binary, loaded, encoding)?;
    let consumed = exact_unmasked_result(
        block,
        index,
        2,
        load.guest_pc,
        raw,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexPackedFpArithmeticMemorySequence {
        consumed,
        address_offset: 0,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}

fn unmasked_broadcast_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFpArithmeticMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFpArithmeticMemoryReplay::Broadcast { .. }
    ) || encoding.writemask.is_some()
        || encoding.zeroing
    {
        return None;
    }
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
    if broadcast.guest_pc != load.guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    let binary = block.ops.get(index + 2)?;
    if binary.guest_pc != load.guest_pc {
        return None;
    }
    let raw = exact_binary(binary, loaded, encoding)?;
    let consumed = exact_unmasked_result(
        block,
        index,
        3,
        load.guest_pc,
        raw,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(X86JitEvexPackedFpArithmeticMemorySequence {
        consumed,
        address_offset: 0,
        memory_size: expected_width.bytes(),
        encoding,
    })
}

fn exact_masked_result(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    encoding: X86EvexPackedFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    exact_evex_vector_mask_result(
        block,
        index,
        offset,
        guest_pc,
        raw,
        mask,
        encoding.width,
        encoding.elem,
        encoding.destination,
        encoding.zeroing,
        virtual_definitions,
        virtual_uses,
    )
}

fn masked_broadcast_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFpArithmeticMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFpArithmeticMemoryReplay::Broadcast { .. }
    ) {
        return None;
    }
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let applicable_bits = (1u64 << lanes) - 1;
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let mut offset = 0usize;
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
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let binary = block.ops.get(index + offset)?;
    if binary.guest_pc != guest_pc {
        return None;
    }
    let raw = exact_binary(binary, loaded, encoding)?;
    offset += 1;
    exact_masked_result(
        block,
        index,
        &mut offset,
        guest_pc,
        raw,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexPackedFpArithmeticMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: expected_width.bytes(),
        encoding,
    })
}

fn masked_vector_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFpArithmeticMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFpArithmeticMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFpArithmeticMemoryReplay::MaskedVector { .. }
    ) {
        return None;
    }
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

    let binary = block.ops.get(index + offset)?;
    if binary.guest_pc != guest_pc {
        return None;
    }
    let raw = exact_binary(binary, loaded, encoding)?;
    offset += 1;
    exact_masked_result(
        block,
        index,
        &mut offset,
        guest_pc,
        raw,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexPackedFpArithmeticMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed
/// AVX-512 binary32/binary64 arithmetic memory source.
///
/// Exact provenance binds map/opcode, precision, vector width, architectural
/// operands, writemask policy, broadcast/full-vector tuple, helper address,
/// dynamic MXCSR behavior, and the single architectural destination commit.
/// Classification is O(L) time and O(1) auxiliary space for L <= 16 lanes;
/// callers build definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_fp_arithmetic_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFpArithmeticMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_fp_arithmetic_memory_encoding()?;
    match encoding.replay {
        X86EvexPackedFpArithmeticMemoryReplay::Vector { .. } => {
            unmasked_vector_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFpArithmeticMemoryReplay::Broadcast { .. } if encoding.writemask.is_some() => {
            masked_broadcast_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFpArithmeticMemoryReplay::Broadcast { .. } => {
            unmasked_broadcast_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFpArithmeticMemoryReplay::MaskedVector { .. } => {
            masked_vector_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
    }
}
