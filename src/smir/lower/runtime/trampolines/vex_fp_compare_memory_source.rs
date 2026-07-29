//! Fail-closed helper-backed VEX floating-point comparison memory admission.

use std::collections::HashMap;

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX floating-point comparison memory-source
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexFpCompareMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) elem: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) predicate: u8,
    pub(crate) scalar: bool,
    pub(crate) w: bool,
}

fn low_vex_vector_index(reg: VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(index),
        _ => None,
    }
}

/// Validate one complete AVX VEX floating-point comparison memory-source
/// decomposition. Packed `VCMPPS`/`VCMPPD` consume an exact two-op
/// `VLoad`/`X86VectorFpCompare` sequence. Scalar `VCMPSS`/`VCMPSD` consume an
/// exact `Load`/`VBroadcast`/`X86VectorFpCompare` sequence, optionally preceded
/// by the lifter's dead zero-initialization of the load temporary. Source-byte
/// provenance binds both architectural inputs, destination, element type,
/// vector/scalar form, width, WIG encoding, and the five-bit predicate. Every
/// loaded virtual must have the exact definition/use count implied by its
/// shape, and no same-PC tail may remain unconsumed.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_fp_compare_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexFpCompareMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    if matches!(load.kind, OpKind::Load { .. } | OpKind::Mov { .. }) {
        return x86_jit_vex_scalar_fp_compare_memory_sequence(
            block,
            index,
            instruction_bytes,
            virtual_definitions,
            virtual_uses,
        );
    }
    let (temporary, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::X86VectorFpCompare {
        dst,
        src1,
        src2,
        mask: None,
        elem,
        width: consumer_width,
        lanes,
        predicate,
        scalar: false,
        mask_destination: false,
        zero_upper: true,
        suppress_exceptions: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != temporary
        || *consumer_width != width
        || *lanes != width.lanes(*elem) as u8
        || !matches!(*elem, VecElementType::F32 | VecElementType::F64)
    {
        return None;
    }
    let destination = low_vex_vector_index(*dst, width)?;
    let source1 = low_vex_vector_index(*src1, width)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_width, encoded_predicate, w) =
        instruction.vex_memory_packed_fp_compare_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_width,
        encoded_predicate,
    ) != (destination, source1, *elem, width, *predicate)
    {
        return None;
    }
    let expected_prefix = if *elem == VecElementType::F32 {
        X86SsePrefix::None
    } else {
        X86SsePrefix::OpSize
    };
    if consumer.x86_hint
        != Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: expected_prefix,
            opcode: 0xC2,
            width,
            w,
        })
    {
        return None;
    }

    Some(X86JitVexFpCompareMemorySequence {
        consumed: 2,
        memory_size: width.bytes(),
        destination,
        source1,
        elem: *elem,
        width,
        predicate: *predicate,
        scalar: false,
        w,
    })
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

fn x86_jit_vex_scalar_fp_compare_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexFpCompareMemorySequence> {
    let first = block.ops.get(index)?;
    let (load_index, initialization) = match &first.kind {
        OpKind::Mov {
            dst,
            src: crate::smir::ir::types::SrcOperand::Imm(0),
            width: crate::smir::ir::types::OpWidth::W64,
        } if first.x86_hint.is_none() && matches!(dst, VReg::Virtual(_)) => (index + 1, Some(*dst)),
        OpKind::Load { .. } => (index, None),
        _ => return None,
    };
    let load = block.ops.get(load_index)?;
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
    let expected_definitions = if let Some(initialized) = initialization {
        if initialized != loaded_scalar || first.guest_pc != load.guest_pc {
            return None;
        }
        2
    } else {
        1
    };
    if virtual_definitions.get(&loaded_scalar) != Some(&expected_definitions)
        || virtual_uses.get(&loaded_scalar) != Some(&1)
    {
        return None;
    }

    let broadcast = block.ops.get(load_index + 1)?;
    let source_vector = match &broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: broadcast_elem,
            lanes: 1,
        } if broadcast.x86_hint.is_none()
            && broadcast.guest_pc == load.guest_pc
            && *scalar == loaded_scalar
            && *broadcast_elem == elem =>
        {
            *dst
        }
        _ => return None,
    };
    if !virtual_single_definition_single_use(source_vector, virtual_definitions, virtual_uses) {
        return None;
    }

    let consumer = block.ops.get(load_index + 2)?;
    if consumer.guest_pc != load.guest_pc
        || block
            .ops
            .get(load_index + 3)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::X86VectorFpCompare {
        dst,
        src1,
        src2,
        mask: None,
        elem: consumer_elem,
        width: VecWidth::V128,
        lanes: 1,
        predicate,
        scalar: true,
        mask_destination: false,
        zero_upper: true,
        suppress_exceptions: false,
    } = &consumer.kind
    else {
        return None;
    };
    if *src2 != source_vector || *consumer_elem != elem {
        return None;
    }
    let destination = low_vex_vector_index(*dst, VecWidth::V128)?;
    let source1 = low_vex_vector_index(*src1, VecWidth::V128)?;

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (encoded_destination, encoded_source1, encoded_elem, encoded_predicate, w) =
        instruction.vex_memory_scalar_fp_compare_fields()?;
    if (
        encoded_destination,
        encoded_source1,
        encoded_elem,
        encoded_predicate,
    ) != (destination, source1, elem, *predicate)
    {
        return None;
    }
    let expected_prefix = if elem == VecElementType::F32 {
        X86SsePrefix::Rep
    } else {
        X86SsePrefix::Repne
    };
    if consumer.x86_hint
        != Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: expected_prefix,
            opcode: 0xC2,
            width: VecWidth::V128,
            w,
        })
    {
        return None;
    }

    Some(X86JitVexFpCompareMemorySequence {
        consumed: load_index - index + 3,
        memory_size,
        destination,
        source1,
        elem,
        width: VecWidth::V128,
        predicate: *predicate,
        scalar: true,
        w,
    })
}
