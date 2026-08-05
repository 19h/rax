//! Fail-closed helper-backed AVX-512 4FMAPS memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexFourFmaMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_nonzero_mask_predicate, exact_virtual_definition_use, no_following_same_pc,
    single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous Tuple1_4X decomposition consumed by the helper-backed
/// x86-64 4FMAPS lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexFourFmaMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexFourFmaMemoryEncoding,
}

fn exact_four_fma(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexFourFmaMemoryEncoding,
) -> bool {
    let width = if encoding.scalar {
        VecWidth::V128
    } else {
        VecWidth::V512
    };
    let hint_width = match encoding.ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return false,
    };
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    matches!(
        op.kind,
        OpKind::X86FourFma {
            dst,
            src0,
            src1,
            src2,
            src3,
            mem,
            mask,
            scalar,
            negate_product,
            mask_zeroing,
        } if vector_index(&dst, width) == Some(encoding.destination)
            && vector_index(&src0, width) == Some(encoding.source_base)
            && vector_index(&src1, width) == Some(encoding.source_base + 1)
            && vector_index(&src2, width) == Some(encoding.source_base + 2)
            && vector_index(&src3, width) == Some(encoding.source_base + 3)
            && mem == memory_source
            && mask == expected_mask
            && scalar == encoding.scalar
            && negate_product == encoding.negate_product
            && mask_zeroing == encoding.zeroing
            && op.x86_hint == Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::Repne,
                opcode: encoding.opcode,
                width: hint_width,
                w: false,
            })
    )
}

fn exact_unmasked_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexFourFmaMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFourFmaMemorySequence> {
    if encoding.writemask.is_some() || encoding.zeroing {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, address) = match &load.kind {
        OpKind::VLoad {
            dst,
            addr,
            width: VecWidth::V128,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => (*dst, addr),
        _ => return None,
    };
    if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
        return None;
    }
    let operation = block.ops.get(index + 1)?;
    if operation.guest_pc != load.guest_pc || !exact_four_fma(operation, loaded, encoding) {
        return None;
    }
    let consumed = 2;
    if !no_following_same_pc(block, index, consumed, load.guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, load.guest_pc, address)
    {
        return None;
    }
    Some(X86JitEvexFourFmaMemorySequence {
        consumed,
        address_offset: 0,
        encoding,
    })
}

fn exact_masked_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexFourFmaMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFourFmaMemorySequence> {
    let mask_index = encoding.writemask?;
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(mask_index)));
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let mut offset = 0usize;
    let active = exact_nonzero_mask_predicate(
        block,
        index,
        &mut offset,
        guest_pc,
        mask,
        if encoding.scalar { 1 } else { 0xFFFF },
        virtual_definitions,
        virtual_uses,
    )?;

    let zero_op = block.ops.get(index + offset)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };
    if zero_op.guest_pc != guest_pc
        || !single_definition_single_use(zero, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let loaded = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I32,
            lanes: 4,
        } if broadcast.x86_hint.is_none() && scalar == zero => dst,
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !exact_virtual_definition_use(loaded, 2, 2, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let address_offset = offset;
    let load = block.ops.get(index + offset)?;
    let address = match &load.kind {
        OpKind::PredVLoad {
            dst,
            cond,
            addr,
            width: VecWidth::V128,
        } if load.x86_hint.is_none()
            && *dst == loaded
            && *cond == active
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            addr
        }
        _ => return None,
    };
    if load.guest_pc != guest_pc {
        return None;
    }
    offset += 1;

    let operation = block.ops.get(index + offset)?;
    if operation.guest_pc != guest_pc || !exact_four_fma(operation, loaded, encoding) {
        return None;
    }
    offset += 1;
    if !no_following_same_pc(block, index, offset, guest_pc)
        || !exact_evex_memory_apx_frontier(block, index, guest_pc, address)
    {
        return None;
    }
    Some(X86JitEvexFourFmaMemorySequence {
        consumed: offset,
        address_offset,
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one AVX-512
/// 4FMAPS Tuple1_4X memory source.
///
/// Exact byte provenance binds map/opcode, LLIG, source-block alignment,
/// destination, mask policy, the single all-or-none 16-byte access, dynamic
/// MXCSR semantics, APX address guard, and guest-PC frontier. Classification
/// is O(1) time and auxiliary space; callers construct global definition/use
/// maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_four_fma_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexFourFmaMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_four_fma_memory_encoding()?;
    if encoding.writemask.is_some() {
        exact_masked_sequence(block, index, encoding, virtual_definitions, virtual_uses)
    } else {
        exact_unmasked_sequence(block, index, encoding, virtual_definitions, virtual_uses)
    }
}
