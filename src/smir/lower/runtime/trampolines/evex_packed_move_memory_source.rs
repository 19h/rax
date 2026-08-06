//! Fail-closed helper-backed writemasked EVEX packed-move memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, SrcOperand, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, X86EvexPackedMoveMemoryEncoding, X86EvexPackedMoveMemoryKind, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape,
    exact_evex_e4_memory_sequence_tail_after_prefix, exact_evex_memory_apx_frontier,
    exact_evex_memory_sequence_address, exact_evex_memory_sequence_frontier,
    exact_evex_reconstructed_vector_mask_result, exact_lane_address, exact_lane_predicate,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// writemasked packed-move lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedMoveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedMoveMemoryEncoding,
}

fn memory_width(elem: VecElementType) -> Option<MemWidth> {
    match elem {
        VecElementType::I8 => Some(MemWidth::B1),
        VecElementType::I16 => Some(MemWidth::B2),
        VecElementType::I32 | VecElementType::F32 => Some(MemWidth::B4),
        VecElementType::I64 | VecElementType::F64 => Some(MemWidth::B8),
        _ => None,
    }
}

fn exact_alignment_prefix(
    block: &SmirBlock,
    index: usize,
    encoding: X86EvexPackedMoveMemoryEncoding,
) -> Option<usize> {
    let Some(expected) = encoding.alignment else {
        return Some(0);
    };
    let guard = block.ops.get(index)?;
    match &guard.kind {
        OpKind::X86CheckAlignment { addr, alignment }
            if guard.x86_hint.is_none()
                && *alignment == expected
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            Some(1)
        }
        _ => None,
    }
}

fn alignment_address_matches(
    block: &SmirBlock,
    index: usize,
    encoding: X86EvexPackedMoveMemoryEncoding,
    address: &crate::smir::ir::types::Address,
) -> bool {
    match encoding.alignment {
        None => true,
        Some(_) => matches!(
            &block.ops[index].kind,
            OpKind::X86CheckAlignment { addr, .. } if addr == address
        ),
    }
}

fn exact_load_sequence(
    block: &SmirBlock,
    index: usize,
    encoding: X86EvexPackedMoveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedMoveMemorySequence> {
    if encoding.kind != X86EvexPackedMoveMemoryKind::Load {
        return None;
    }
    let prefix_ops = exact_alignment_prefix(block, index, encoding)?;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask)));
    let lanes = encoding.width.lanes(encoding.elem) as usize;
    let exact = exact_evex_e4_memory_sequence_tail_after_prefix(
        block,
        index,
        prefix_ops,
        X86EvexE4MemoryShape {
            width: encoding.width,
            elem: encoding.elem,
            writemask: Some(encoding.writemask),
            zeroing: encoding.zeroing,
            vector_load_hint: None,
            form: X86EvexE4MemoryReplayForm::MaskedVector,
            memory_source_uses: lanes,
        },
        virtual_definitions,
        virtual_uses,
        |block, tail_index, raw| {
            let mut offset = 0usize;
            exact_evex_reconstructed_vector_mask_result(
                block,
                tail_index,
                &mut offset,
                block.ops.get(tail_index)?.guest_pc,
                raw,
                mask,
                encoding.width,
                encoding.elem,
                encoding.vector,
                encoding.zeroing,
                virtual_definitions,
                virtual_uses,
            )?;
            Some(offset)
        },
    )?;
    let address = exact_evex_memory_sequence_address(block, index, exact.address_offset)?;
    if !alignment_address_matches(block, index, encoding, address) {
        return None;
    }
    Some(X86JitEvexPackedMoveMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}

fn exact_store_sequence(
    block: &SmirBlock,
    index: usize,
    encoding: X86EvexPackedMoveMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedMoveMemorySequence> {
    if encoding.kind != X86EvexPackedMoveMemoryKind::Store || encoding.zeroing {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let prefix_ops = exact_alignment_prefix(block, index, encoding)?;
    let address_offset = prefix_ops;
    let lea = block.ops.get(index + address_offset)?;
    let (base, address) = match &lea.kind {
        OpKind::Lea {
            dst: base @ VReg::Virtual(_),
            addr,
        } if lea.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => (*base, addr),
        _ => return None,
    };
    let lanes = encoding.width.lanes(encoding.elem) as usize;
    if lea.guest_pc != guest_pc
        || !exact_virtual_definition_use(base, 1, lanes, virtual_definitions, virtual_uses)
        || !alignment_address_matches(block, index, encoding, address)
    {
        return None;
    }

    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask)));
    let expected_width = memory_width(encoding.elem)?;
    let lane_bytes = i64::from(encoding.elem.bytes());
    let mut offset = address_offset + 1;
    for lane in 0..u8::try_from(lanes).ok()? {
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
        let extract = block.ops.get(index + offset)?;
        let scalar = match extract.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: actual_lane,
                elem,
                sign: SignExtend::Zero,
            } if extract.x86_hint.is_none()
                && vector_index(&vec, encoding.width) == Some(encoding.vector)
                && actual_lane == lane
                && elem == encoding.elem =>
            {
                dst
            }
            _ => return None,
        };
        if extract.guest_pc != guest_pc
            || !single_definition_single_use(scalar, virtual_definitions, virtual_uses)
        {
            return None;
        }
        offset += 1;

        let store = block.ops.get(index + offset)?;
        if store.guest_pc != guest_pc
            || store.x86_hint.is_some()
            || !matches!(
                &store.kind,
                OpKind::PredStore {
                    src: SrcOperand::Reg(actual_scalar),
                    cond,
                    addr,
                    width,
                } if *actual_scalar == scalar
                    && *cond == condition
                    && *width == expected_width
                    && exact_lane_address(addr, base, i64::from(lane) * lane_bytes)
            )
        {
            return None;
        }
        offset += 1;
    }
    if !no_following_same_pc(block, index, offset, guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }
    Some(X86JitEvexPackedMoveMemorySequence {
        consumed: offset,
        address_offset,
        memory_size: encoding.width.bytes(),
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition for one writemasked EVEX
/// packed move with a memory operand.
///
/// Exact byte provenance binds all ten mnemonics, direction, width, element
/// granularity, vector register, mask policy, alignment class, APX address
/// frontier, and complete instruction length. Loads bind every predicated lane
/// read plus the single deferred architectural vector commit. Stores bind each
/// ascending predicated lane write, including partial completion. Runtime is
/// O(L) and auxiliary space is O(1) for 2 <= L <= 64 lanes; callers construct
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_move_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedMoveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_packed_move_memory_encoding()?;
    match encoding.kind {
        X86EvexPackedMoveMemoryKind::Load => {
            exact_load_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
        X86EvexPackedMoveMemoryKind::Store => {
            exact_store_sequence(block, index, encoding, virtual_definitions, virtual_uses)
        }
    }
}
