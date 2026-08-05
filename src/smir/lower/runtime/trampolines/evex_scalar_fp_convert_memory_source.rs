//! Fail-closed helper-backed EVEX scalar floating-point conversion admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecWidth, X86Reg,
};
use crate::smir::ir::{X86EvexScalarFpConvertMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier,
    exact_virtual_definition_use,
};
use super::evex_scalar_memory_source_common::exact_evex_scalar_mask_condition;
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous EVEX scalar precision-conversion memory decomposition
/// consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarFpConvertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) load_offset: usize,
    pub(crate) encoding: X86EvexScalarFpConvertMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn expected_hint(encoding: X86EvexScalarFpConvertMemoryEncoding) -> Option<X86OpHint> {
    let map = match encoding.map {
        1 => X86VecMap::Map0F,
        5 => X86VecMap::Map5,
        6 => X86VecMap::Map6,
        _ => return None,
    };
    let pp = match encoding.pp {
        0 => X86SsePrefix::None,
        2 => X86SsePrefix::Rep,
        3 => X86SsePrefix::Repne,
        _ => return None,
    };
    let width = match encoding.ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };
    Some(X86OpHint::EvexOp {
        map,
        pp,
        opcode: encoding.opcode,
        width,
        w: encoding.w,
    })
}

fn exact_load(op: &crate::smir::ir::ops::SmirOp, expected_width: MemWidth) -> Option<VReg> {
    match &op.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign: SignExtend::Zero,
        } if op.x86_hint.is_none()
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            Some(*dst)
        }
        _ => None,
    }
}

fn exact_predicated_load(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    condition: VReg,
    expected_width: MemWidth,
) -> bool {
    matches!(
        &op.kind,
        OpKind::PredLoad {
            dst,
            cond,
            addr,
            width,
            signed: SignExtend::Zero,
        } if op.x86_hint.is_none()
            && *dst == loaded
            && *cond == condition
            && *width == expected_width
            && x86_jit_mem_address_shape_valid(addr)
    )
}

fn exact_conversion(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexScalarFpConvertMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    matches!(
        op.kind,
        OpKind::X86FpConvert {
            dst,
            merge,
            src,
            mask,
            from,
            to,
            mask_zeroing,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            zero_upper: true,
        } if op.x86_hint == expected_hint(encoding)
            && dst == xmm(encoding.destination)
            && merge == xmm(encoding.merge)
            && src == loaded
            && mask == expected_mask
            && from == encoding.from
            && to == encoding.to
            && mask_zeroing == encoding.zeroing
    )
}

fn exact_unmasked_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarFpConvertMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpConvertMemorySequence> {
    if encoding.writemask.is_some() || encoding.zeroing {
        return None;
    }
    let load = block.ops.get(index)?;
    let loaded = exact_load(load, encoding.memory_width)?;
    if !exact_virtual_definition_use(loaded, 1, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let conversion = block.ops.get(index + 1)?;
    if conversion.guest_pc != load.guest_pc || !exact_conversion(conversion, loaded, encoding) {
        return None;
    }
    Some(X86JitEvexScalarFpConvertMemorySequence {
        consumed: 2,
        load_offset: 0,
        encoding,
    })
}

fn exact_masked_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    encoding: X86EvexScalarFpConvertMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpConvertMemorySequence> {
    let mask = encoding.writemask?;
    let guest_pc = block.ops.get(index)?.guest_pc;
    let condition = exact_evex_scalar_mask_condition(
        block,
        index,
        guest_pc,
        mask,
        1,
        virtual_definitions,
        virtual_uses,
    )?;

    let seed = block.ops.get(index + 1)?;
    let loaded = match seed.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if seed.x86_hint.is_none() => dst,
        _ => return None,
    };
    if seed.guest_pc != guest_pc
        || !exact_virtual_definition_use(loaded, 2, 1, virtual_definitions, virtual_uses)
    {
        return None;
    }

    let load_offset = 2;
    let load = block.ops.get(index + load_offset)?;
    if load.guest_pc != guest_pc
        || !exact_predicated_load(load, loaded, condition, encoding.memory_width)
    {
        return None;
    }
    let conversion = block.ops.get(index + 3)?;
    if conversion.guest_pc != guest_pc || !exact_conversion(conversion, loaded, encoding) {
        return None;
    }
    Some(X86JitEvexScalarFpConvertMemorySequence {
        consumed: 4,
        load_offset,
        encoding,
    })
}

/// Validate the complete O0/O1/O2 decomposition emitted for one EVEX scalar
/// floating-point precision conversion whose final source is memory.
///
/// Exact provenance binds the conversion direction, destination and merge
/// registers, LLIG image, writemask policy, helper address, dynamic rounding,
/// exception policy, and single architectural destination commit. Matching is
/// O(1) time and space; callers construct definition/use maps once in O(N)
/// time and O(V) space.
pub(crate) fn x86_jit_evex_scalar_fp_convert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpConvertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if !exact_evex_memory_sequence_frontier(block, index, first.guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_scalar_fp_convert_memory_encoding()?;
    let sequence = if encoding.writemask.is_some() {
        exact_masked_sequence(block, index, encoding, virtual_definitions, virtual_uses)?
    } else {
        exact_unmasked_sequence(block, index, encoding, virtual_definitions, virtual_uses)?
    };
    if block
        .ops
        .get(index + sequence.consumed)
        .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }
    let address = match &block.ops.get(index + sequence.load_offset)?.kind {
        OpKind::Load { addr, .. } | OpKind::PredLoad { addr, .. } => addr,
        _ => return None,
    };
    exact_evex_memory_apx_frontier(block, index, first.guest_pc, address).then_some(sequence)
}
