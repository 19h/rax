//! Fail-closed helper-backed EVEX packed binary16-complex memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedFp16ComplexMemoryEncoding, X86EvexPackedFp16ComplexMemoryReplay,
    X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_lane_address, exact_lane_predicate, exact_nonzero_mask_predicate,
    exact_virtual_definition_use, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// packed binary16-complex memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFp16ComplexMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFp16ComplexMemoryEncoding,
}

fn exact_complex(
    op: &crate::smir::ir::ops::SmirOp,
    source2: VReg,
    encoding: X86EvexPackedFp16ComplexMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let expected_pp = if encoding.conjugate {
        X86SsePrefix::Repne
    } else {
        X86SsePrefix::Rep
    };
    let expected_opcode = if encoding.accumulate { 0x56 } else { 0xD6 };
    matches!(
        &op.kind,
        OpKind::X86FP16Complex {
            dst,
            src1,
            src2,
            mask,
            width,
            pairs,
            scalar,
            mask_zeroing,
            accumulate,
            conjugate,
            round,
        } if vector_index(dst, encoding.width) == Some(encoding.destination)
            && vector_index(src1, encoding.width) == Some(encoding.source1)
            && *src2 == source2
            && *mask == expected_mask
            && *width == encoding.width
            && *pairs == (encoding.width.bytes() / 4) as u8
            && !*scalar
            && *mask_zeroing == encoding.zeroing
            && *accumulate == encoding.accumulate
            && *conjugate == encoding.conjugate
            && *round == FpRoundMode::Dynamic
    ) && matches!(
        op.x86_hint,
        Some(X86OpHint::EvexOp {
            map: X86VecMap::Map6,
            pp,
            opcode,
            width,
            w: false,
        }) if pp == expected_pp && opcode == expected_opcode && width == encoding.width
    )
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

fn unmasked_vector_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFp16ComplexMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFp16ComplexMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFp16ComplexMemoryReplay::Vector { .. }
    ) || encoding.writemask.is_some()
        || encoding.zeroing
    {
        return None;
    }
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
    let complex = block.ops.get(index + 1)?;
    let consumed = 2;
    if complex.guest_pc != load.guest_pc
        || !exact_complex(complex, loaded, encoding)
        || !no_following_same_pc(block, index, consumed, load.guest_pc)
    {
        return None;
    }
    Some(X86JitEvexPackedFp16ComplexMemorySequence {
        consumed,
        address_offset: 0,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}

fn unmasked_broadcast_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFp16ComplexMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFp16ComplexMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFp16ComplexMemoryReplay::Broadcast { .. }
    ) || encoding.writemask.is_some()
        || encoding.zeroing
    {
        return None;
    }
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
            elem: VecElementType::I32,
            lanes,
        } if broadcast.x86_hint.is_none()
            && actual_scalar == scalar
            && lanes == (encoding.width.bytes() / 4) as u8 =>
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
    let complex = block.ops.get(index + 2)?;
    let consumed = 3;
    if complex.guest_pc != load.guest_pc
        || !exact_complex(complex, loaded, encoding)
        || !no_following_same_pc(block, index, consumed, load.guest_pc)
    {
        return None;
    }
    Some(X86JitEvexPackedFp16ComplexMemorySequence {
        consumed,
        address_offset: 0,
        memory_size: MemWidth::B4.bytes(),
        encoding,
    })
}

fn masked_broadcast_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFp16ComplexMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFp16ComplexMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFp16ComplexMemoryReplay::Broadcast { .. }
    ) {
        return None;
    }
    let mask_index = encoding.writemask?;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(mask_index)));
    let lanes = (encoding.width.bytes() / 4) as u8;
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
            elem: VecElementType::I32,
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

    let complex = block.ops.get(index + offset)?;
    if complex.guest_pc != guest_pc || !exact_complex(complex, loaded, encoding) {
        return None;
    }
    offset += 1;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexPackedFp16ComplexMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: MemWidth::B4.bytes(),
        encoding,
    })
}

fn masked_vector_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexPackedFp16ComplexMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFp16ComplexMemorySequence> {
    if !matches!(
        encoding.replay,
        X86EvexPackedFp16ComplexMemoryReplay::MaskedVector { .. }
    ) {
        return None;
    }
    let mask_index = encoding.writemask?;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(mask_index)));
    let lanes = (encoding.width.bytes() / 4) as u8;
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
            elem: VecElementType::I32,
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
                    elem: VecElementType::I32,
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

    let complex = block.ops.get(index + offset)?;
    if complex.guest_pc != guest_pc || !exact_complex(complex, loaded, encoding) {
        return None;
    }
    offset += 1;
    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexPackedFp16ComplexMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed
/// AVX-512-FP16 complex memory source.
///
/// Exact provenance binds MAP/opcode, vector width, architectural operands,
/// writemask policy, broadcast/full-vector tuple, helper address, and the
/// single architectural destination commit. Classification is O(L) time and
/// O(1) auxiliary space for L <= 16 complex pairs; callers build definition/use maps
/// once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_fp16_complex_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFp16ComplexMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_fp16_complex_memory_encoding()?;
    match encoding.replay {
        X86EvexPackedFp16ComplexMemoryReplay::Vector { .. } => {
            unmasked_vector_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFp16ComplexMemoryReplay::Broadcast { .. } if encoding.writemask.is_some() => {
            masked_broadcast_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFp16ComplexMemoryReplay::Broadcast { .. } => {
            unmasked_broadcast_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedFp16ComplexMemoryReplay::MaskedVector { .. } => {
            masked_vector_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
    }
}
