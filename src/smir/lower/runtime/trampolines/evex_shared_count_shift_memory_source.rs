//! Exact helper-backed EVEX packed shared-count shift memory admission.

use std::collections::HashMap;

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_evex_vector_mask_result, no_following_same_pc, single_definition_single_use,
    vector_index,
};
use super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexSharedCountShiftMemoryEncoding, X86InstructionBytes};

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// packed shared-count shift memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexSharedCountShiftMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexSharedCountShiftMemoryEncoding,
}

/// Validate the complete O0/O1/O2 decomposition emitted for one AVX-512
/// packed shift whose common count is loaded from a Mem128 operand.
///
/// Classification binds exact instruction bytes, effective-address shape,
/// the unconditional 128-bit load, low-64-bit count extraction, vector
/// operands, shift semantics, writemask reconstruction, APX guard frontier,
/// and the sole architectural destination commit. Runtime is O(L) for L
/// destination lanes and uses O(1) auxiliary space after the caller builds
/// definition/use maps.
pub(crate) fn x86_jit_evex_shared_count_shift_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexSharedCountShiftMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_shared_count_shift_memory_encoding()?;
    let (loaded, address) = match &load.kind {
        OpKind::VLoad {
            dst,
            addr,
            width: VecWidth::V128,
        } if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            && matches!(dst, VReg::Virtual(_))
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, addr)
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded, virtual_definitions, virtual_uses)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }

    let extract = block.ops.get(index + 1)?;
    let count = match extract.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane: 0,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } if extract.x86_hint.is_none() && vec == loaded && matches!(dst, VReg::Virtual(_)) => dst,
        _ => return None,
    };
    if extract.guest_pc != guest_pc
        || !single_definition_single_use(count, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let shift = block.ops.get(index + 2)?;
    let raw = match shift.kind {
        OpKind::X86PackedShift {
            dst,
            src,
            count: actual_count,
            width,
            elem,
            shift: actual_shift,
        } if shift.x86_hint.is_none()
            && actual_count == count
            && vector_index(&src, encoding.width) == Some(encoding.source)
            && width == encoding.width
            && elem == encoding.elem
            && actual_shift == encoding.shift =>
        {
            dst
        }
        _ => return None,
    };
    if shift.guest_pc != guest_pc {
        return None;
    }

    let mut consumed = 3usize;
    if let Some(mask) = encoding.writemask {
        if !matches!(raw, VReg::Virtual(_)) {
            return None;
        }
        exact_evex_vector_mask_result(
            block,
            index,
            &mut consumed,
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

    if !no_following_same_pc(block, index, consumed, guest_pc) {
        return None;
    }
    Some(X86JitEvexSharedCountShiftMemorySequence {
        consumed,
        memory_size: VecWidth::V128.bytes(),
        encoding,
    })
}
