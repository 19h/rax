//! Fail-closed helper-backed EVEX VPUNPCK*DQ/QDQ broadcast-memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexBroadcastInterleaveMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, no_following_same_pc, single_definition_single_use,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// packed D/Q interleave scalar-broadcast memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexBroadcastInterleaveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_offset: usize,
    pub(crate) encoding: X86EvexBroadcastInterleaveMemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> Option<VReg> {
    Some(match width {
        VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))),
        VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(index))),
        VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(index))),
        _ => return None,
    })
}

fn exact_interleave(
    op: &SmirOp,
    dst: VReg,
    src1: VReg,
    src2: VReg,
    encoding: X86EvexBroadcastInterleaveMemoryEncoding,
) -> bool {
    matches!(
        op.kind,
        OpKind::VInterleave {
            dst: actual_dst,
            src1: actual_src1,
            src2: actual_src2,
            elem,
            lanes,
            block_lanes,
            high,
        } if actual_dst == dst
            && actual_src1 == src1
            && actual_src2 == src2
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8
            && block_lanes == (16 / encoding.elem.bytes()) as u8
            && high == encoding.high
    ) && op.x86_hint
        == Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: encoding.opcode,
            width: encoding.width,
            w: encoding.elem == VecElementType::I64,
        })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VPUNPCKLDQ/LQDQ/HDQ/HQDQ scalar-broadcast memory source. Exact provenance
/// binds width, element type, interleave half, operands, mask, unconditional
/// E4NF memory semantics, and helper memory width.
///
/// Classification is O(L) time and O(1) auxiliary space for L <= 16 lanes;
/// callers build global definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_broadcast_interleave_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBroadcastInterleaveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_broadcast_interleave_memory_encoding()?;

    let loaded_scalar = match first.kind {
        OpKind::Load {
            dst,
            ref addr,
            width,
            sign: SignExtend::Zero,
        } if first.x86_hint.is_none()
            && width == encoding.memory_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            dst
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses) {
        return None;
    }

    let broadcast = block.ops.get(index + 1)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes,
        } if broadcast.x86_hint.is_none()
            && scalar == loaded_scalar
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if broadcast.guest_pc != first.guest_pc
        || !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let interleave = block.ops.get(index + 2)?;
    let raw = match interleave.kind {
        OpKind::VInterleave { dst, .. } => dst,
        _ => return None,
    };
    if interleave.guest_pc != first.guest_pc
        || !exact_interleave(
            interleave,
            raw,
            vector(encoding.source1, encoding.width)?,
            loaded,
            encoding,
        )
    {
        return None;
    }
    let mut offset = 3;
    if let Some(mask) = encoding.writemask {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            first.guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else if raw != vector(encoding.destination, encoding.width)? || encoding.zeroing {
        return None;
    }
    if !no_following_same_pc(block, index, offset, first.guest_pc) {
        return None;
    }

    Some(X86JitEvexBroadcastInterleaveMemorySequence {
        consumed: offset,
        memory_offset: 0,
        encoding,
    })
}
