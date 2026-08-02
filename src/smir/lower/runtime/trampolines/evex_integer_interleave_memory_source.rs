//! Fail-closed helper-backed EVEX VPUNPCK* Full Mem admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, X86Reg};
use crate::smir::ir::{X86EvexIntegerInterleaveMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// VPUNPCK* full-vector memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexIntegerInterleaveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexIntegerInterleaveMemoryEncoding,
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX
/// VPUNPCKLBW/LWD/LDQ/LQDQ/HBW/HWD/HDQ/HQDQ Full Mem source.
///
/// Exact provenance binds opcode, W/WIG, vector and element widths, low/high
/// half selection, operands, destination mask policy, one unconditional E4NF
/// complete-tuple read, every merge/zero lane, and the guest-PC frontier.
/// Runtime is O(L) and auxiliary space is O(1) for L <= 64 lanes; callers
/// construct definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_integer_interleave_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexIntegerInterleaveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_integer_interleave_memory_encoding()?;
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

    let interleave = block.ops.get(index + 1)?;
    let raw = match interleave.kind {
        OpKind::VInterleave {
            dst,
            src1,
            src2,
            elem,
            lanes,
            block_lanes,
            high,
        } if vector_index(&src1, encoding.width) == Some(encoding.source1)
            && src2 == loaded
            && elem == encoding.elem
            && u32::from(lanes) == encoding.width.lanes(encoding.elem)
            && u32::from(block_lanes) == 16 / encoding.elem.bytes()
            && high == encoding.high
            && interleave.x86_hint
                == Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: encoding.opcode,
                    width: encoding.width,
                    w: encoding.w,
                }) =>
        {
            dst
        }
        _ => return None,
    };
    if interleave.guest_pc != guest_pc {
        return None;
    }
    let mut offset = 2;

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
    } else if encoding.zeroing || vector_index(&raw, encoding.width) != Some(encoding.destination) {
        return None;
    }

    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexIntegerInterleaveMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size: encoding.memory_size,
        encoding,
    })
}
