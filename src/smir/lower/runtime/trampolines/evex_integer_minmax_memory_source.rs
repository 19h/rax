//! Fail-closed helper-backed EVEX packed integer minimum/maximum admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecCmpCond,
    X86Reg,
};
use crate::smir::ir::{
    X86EvexIntegerArithmeticMemoryReplay, X86EvexIntegerMinMaxMemoryEncoding, X86InstructionBytes,
};

use super::evex_integer_arithmetic_memory_source::{
    MatchedMemorySource, X86EvexIntegerMemoryShape, matched_integer_memory_source,
};
use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, exact_nonzero_mask_predicate, exact_virtual_definition_use,
    single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// packed integer minimum/maximum memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexIntegerMinMaxMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexIntegerMinMaxMemoryEncoding,
}

fn predicate_first_masked_broadcast_source(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexIntegerMinMaxMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<MatchedMemorySource> {
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?)));
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let applicable_bits = if lanes == 64 {
        u64::MAX
    } else {
        (1u64 << lanes) - 1
    };
    let guest_pc = block.ops.get(index)?.guest_pc;
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
    let memory_width = match encoding.elem {
        crate::smir::ir::types::VecElementType::I32 => MemWidth::B4,
        crate::smir::ir::types::VecElementType::I64 => MemWidth::B8,
        _ => return None,
    };
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
            && *width == memory_width
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
        || !exact_virtual_definition_use(loaded, 1, 2, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;
    Some(MatchedMemorySource {
        loaded,
        offset,
        address_offset,
        memory_size: memory_width.bytes(),
    })
}

fn exact_minmax(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    loaded: VReg,
    encoding: X86EvexIntegerMinMaxMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<VReg> {
    let compare = block.ops.get(index + *offset)?;
    let expected_condition = match (encoding.minimum, encoding.signed) {
        (true, true) => VecCmpCond::Lt,
        (true, false) => VecCmpCond::Ltu,
        (false, true) => VecCmpCond::Gt,
        (false, false) => VecCmpCond::Gtu,
    };
    let select_src1 = match compare.kind {
        OpKind::VCmp {
            dst,
            src1,
            src2,
            cond,
            elem,
            lanes,
        } if compare.x86_hint.is_none()
            && vector_index(&src1, encoding.width) == Some(encoding.source1)
            && src2 == loaded
            && cond == expected_condition
            && elem == encoding.elem
            && lanes == encoding.width.lanes(encoding.elem) as u8 =>
        {
            dst
        }
        _ => return None,
    };
    if compare.guest_pc != guest_pc
        || !single_definition_single_use(select_src1, virtual_definitions, virtual_uses)
    {
        return None;
    }
    *offset += 1;

    let select = block.ops.get(index + *offset)?;
    let raw = match select.kind {
        OpKind::VBitSelect {
            dst,
            mask,
            src_true,
            src_false,
            width,
        } if select.x86_hint.is_none()
            && mask == select_src1
            && vector_index(&src_true, encoding.width) == Some(encoding.source1)
            && src_false == loaded
            && width == encoding.width =>
        {
            dst
        }
        _ => return None,
    };
    if select.guest_pc != guest_pc {
        return None;
    }
    *offset += 1;
    Some(raw)
}

#[allow(clippy::too_many_arguments)]
fn exact_unmasked_commit(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    offset: &mut usize,
    guest_pc: GuestAddr,
    raw: VReg,
    encoding: X86EvexIntegerMinMaxMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<()> {
    if !exact_virtual_definition_use(raw, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let commit = block.ops.get(index + *offset)?;
    if commit.x86_hint.is_some()
        || commit.guest_pc != guest_pc
        || !matches!(
            commit.kind,
            OpKind::VMov { dst, src, width }
                if vector_index(&dst, encoding.width) == Some(encoding.destination)
                    && src == raw
                    && width == encoding.width
        )
    {
        return None;
    }
    *offset += 1;
    Some(())
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX packed
/// signed/unsigned integer minimum/maximum memory source.
///
/// Exact provenance binds the map, opcode, W/WIG interpretation, vector and
/// element widths, architectural operands, signedness, min/max direction,
/// mask policy, tuple kind, address, every active-lane predicate, compare and
/// select semantics, and final commit. Runtime is O(L) with O(1) auxiliary
/// space for L <= 64 lanes; callers build global definition/use maps once in
/// O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_integer_minmax_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexIntegerMinMaxMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_integer_minmax_memory_encoding()?;
    let shape = X86EvexIntegerMemoryShape::from(encoding);
    let source = if matches!(
        encoding.replay,
        X86EvexIntegerArithmeticMemoryReplay::Broadcast { .. }
    ) && encoding.writemask.is_some()
    {
        predicate_first_masked_broadcast_source(
            block,
            index,
            encoding,
            virtual_definitions,
            virtual_uses,
        )?
    } else {
        matched_integer_memory_source(
            block,
            index,
            shape,
            encoding.replay,
            2,
            virtual_definitions,
            virtual_uses,
        )?
    };

    let mut offset = source.offset;
    let raw = exact_minmax(
        block,
        index,
        &mut offset,
        guest_pc,
        source.loaded,
        encoding,
        virtual_definitions,
        virtual_uses,
    )?;
    if encoding.writemask.is_some() {
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(encoding.writemask?))),
            encoding.width,
            encoding.elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else {
        exact_unmasked_commit(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            encoding,
            virtual_definitions,
            virtual_uses,
        )?;
    }
    if block
        .ops
        .get(index + offset)
        .is_some_and(|op| op.guest_pc == guest_pc)
    {
        return None;
    }

    Some(X86JitEvexIntegerMinMaxMemorySequence {
        consumed: offset,
        address_offset: source.address_offset,
        memory_size: source.memory_size,
        encoding,
    })
}
