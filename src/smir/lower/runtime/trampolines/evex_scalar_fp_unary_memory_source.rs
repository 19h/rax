//! Fail-closed helper-backed EVEX scalar floating-point unary memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86EvexScalarFpUnaryMemoryEncoding, X86EvexScalarFpUnaryMemoryKind, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, exact_lane_predicate,
    exact_virtual_definition_use, no_following_same_pc, single_definition_single_use,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous scalar special or approximate floating-point memory
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScalarFpUnaryMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) load_offset: usize,
    pub(crate) encoding: X86EvexScalarFpUnaryMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn expected_hint(encoding: X86EvexScalarFpUnaryMemoryEncoding) -> Option<X86OpHint> {
    let map = match encoding.map {
        2 => X86VecMap::Map0F38,
        3 => X86VecMap::Map0F3A,
        6 => X86VecMap::Map6,
        _ => return None,
    };
    let pp = match encoding.pp {
        0 => X86SsePrefix::None,
        1 => X86SsePrefix::OpSize,
        _ => return None,
    };
    Some(X86OpHint::EvexOp {
        map,
        pp,
        opcode: encoding.opcode,
        width: VecWidth::V128,
        w: encoding.w,
    })
}

fn exact_load(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    condition: Option<VReg>,
    expected_width: MemWidth,
) -> bool {
    match (&op.kind, condition) {
        (
            OpKind::Load {
                dst,
                addr,
                width,
                sign: SignExtend::Zero,
            },
            None,
        ) => {
            op.x86_hint.is_none()
                && *dst == loaded
                && *width == expected_width
                && x86_jit_mem_address_shape_valid(addr)
        }
        (
            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed: SignExtend::Zero,
            },
            Some(expected_condition),
        ) => {
            op.x86_hint.is_none()
                && *dst == loaded
                && *cond == expected_condition
                && *width == expected_width
                && x86_jit_mem_address_shape_valid(addr)
        }
        _ => false,
    }
}

fn exact_semantic(
    op: &crate::smir::ir::ops::SmirOp,
    source: VReg,
    encoding: X86EvexScalarFpUnaryMemoryEncoding,
) -> bool {
    let destination = xmm(encoding.destination);
    let merge = Some(xmm(encoding.merge));
    let mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let common = |dst: VReg,
                  actual_merge: Option<VReg>,
                  src: VReg,
                  actual_mask: Option<VReg>,
                  elem: VecElementType,
                  width: VecWidth,
                  lanes: u8,
                  scalar: bool,
                  mask_zeroing: bool,
                  suppress_exceptions: bool| {
        dst == destination
            && actual_merge == merge
            && src == source
            && actual_mask == mask
            && elem == encoding.elem
            && width == VecWidth::V128
            && lanes == 1
            && scalar
            && mask_zeroing == encoding.zeroing
            && !suppress_exceptions
    };
    let shape = match op.kind {
        OpKind::X86GetExponent {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
            suppress_exceptions,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::GetExponent
            && encoding.immediate.is_none() =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            )
        }
        OpKind::X86GetMantissa {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            imm,
            scalar,
            mask_zeroing,
            suppress_exceptions,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::GetMantissa
            && encoding.immediate == Some(imm) =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            )
        }
        OpKind::X86RoundScale {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            imm,
            scalar,
            mask_zeroing,
            suppress_exceptions,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::RoundScale
            && encoding.immediate == Some(imm) =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            )
        }
        OpKind::X86Reduce {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            imm,
            scalar,
            mask_zeroing,
            suppress_exceptions,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::Reduce
            && encoding.immediate == Some(imm) =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            )
        }
        OpKind::X86Recip14 {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::Recip14
            && encoding.immediate.is_none() =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                false,
            )
        }
        OpKind::X86Rsqrt14 {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::Rsqrt14
            && encoding.immediate.is_none() =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                false,
            )
        }
        OpKind::X86RecipFp16 {
            dst,
            merge,
            src,
            mask,
            width,
            lanes,
            scalar,
            mask_zeroing,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::RecipFp16
            && encoding.immediate.is_none() =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                VecElementType::F16,
                width,
                lanes,
                scalar,
                mask_zeroing,
                false,
            )
        }
        OpKind::X86RsqrtFp16 {
            dst,
            merge,
            src,
            mask,
            width,
            lanes,
            scalar,
            mask_zeroing,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::RsqrtFp16
            && encoding.immediate.is_none() =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                VecElementType::F16,
                width,
                lanes,
                scalar,
                mask_zeroing,
                false,
            )
        }
        OpKind::X86Recip28 {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
            suppress_exceptions,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::Recip28
            && encoding.immediate.is_none() =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            )
        }
        OpKind::X86Rsqrt28 {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
            suppress_exceptions,
        } if encoding.kind == X86EvexScalarFpUnaryMemoryKind::Rsqrt28
            && encoding.immediate.is_none() =>
        {
            common(
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            )
        }
        _ => false,
    };
    shape && op.x86_hint == expected_hint(encoding)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one scalar
/// special or approximate floating-point memory source.
///
/// Exact provenance binds the family, element type, destination and merge
/// registers, LLIG image, imm8, writemask policy, helper address, exception
/// policy, and single architectural destination commit. Matching is O(1)
/// time and space; callers construct definition/use maps once in O(N) time
/// and O(V) space.
#[allow(clippy::too_many_arguments)]
pub(crate) fn x86_jit_evex_scalar_fp_unary_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScalarFpUnaryMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_scalar_fp_unary_memory_encoding()?;

    let loaded = match first.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if first.x86_hint.is_none() => dst,
        _ => return None,
    };
    if !exact_virtual_definition_use(loaded, 2, 1, virtual_definitions, virtual_uses) {
        return None;
    }
    let mut offset = 1usize;
    let condition = if let Some(mask) = encoding.writemask {
        Some(exact_lane_predicate(
            block,
            index,
            &mut offset,
            guest_pc,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask))),
            0,
            virtual_definitions,
            virtual_uses,
        )?)
    } else {
        None
    };

    let load_offset = offset;
    let load = block.ops.get(index + offset)?;
    if load.guest_pc != guest_pc || !exact_load(load, loaded, condition, encoding.memory_width) {
        return None;
    }
    offset += 1;

    let broadcast = block.ops.get(index + offset)?;
    let source = match broadcast.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: 1,
        } if broadcast.x86_hint.is_none() && scalar == loaded && elem == encoding.elem => dst,
        _ => return None,
    };
    if broadcast.guest_pc != guest_pc
        || !single_definition_single_use(source, virtual_definitions, virtual_uses)
    {
        return None;
    }
    offset += 1;

    let semantic = block.ops.get(index + offset)?;
    if semantic.guest_pc != guest_pc || !exact_semantic(semantic, source, encoding) {
        return None;
    }
    let consumed = offset + 1;
    if !no_following_same_pc(block, index, consumed, guest_pc) {
        return None;
    }
    let address = match &load.kind {
        OpKind::Load { addr, .. } | OpKind::PredLoad { addr, .. } => addr,
        _ => unreachable!("validated scalar unary sequence owns its memory operation"),
    };
    exact_evex_memory_apx_frontier(block, index, guest_pc, address).then_some(
        X86JitEvexScalarFpUnaryMemorySequence {
            consumed,
            load_offset,
            encoding,
        },
    )
}
