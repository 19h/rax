//! Fail-closed helper-backed EVEX packed unary floating-point memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, Avx10FP16Op, BlockId, FpRoundMode, GuestAddr, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{
    X86EvexPackedFpUnaryMemoryEncoding, X86EvexPackedFpUnaryMemoryKind,
    X86EvexPackedFpUnaryMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence_tail,
    exact_evex_vector_mask_result, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// packed unary floating-point memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedFpUnaryMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedFpUnaryMemoryEncoding,
}

fn exact_packed_fp_unary(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexPackedFpUnaryMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let expected_map = match encoding.map {
        2 => X86VecMap::Map0F38,
        3 => X86VecMap::Map0F3A,
        6 => X86VecMap::Map6,
        _ => return false,
    };
    let expected_prefix = match encoding.pp {
        0 => X86SsePrefix::None,
        1 => X86SsePrefix::OpSize,
        _ => return false,
    };
    let exact_hint = op.x86_hint
        == Some(X86OpHint::EvexOp {
            map: expected_map,
            pp: expected_prefix,
            opcode: encoding.opcode,
            width: encoding.width,
            w: encoding.w,
        });
    if !exact_hint {
        return false;
    }

    let lanes = encoding.width.lanes(encoding.elem) as u8;
    match op.kind {
        OpKind::X86GetExponent {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            scalar: false,
            mask_zeroing,
            suppress_exceptions: false,
        } => {
            encoding.kind == X86EvexPackedFpUnaryMemoryKind::GetExponent
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && actual_lanes == lanes
                && mask_zeroing == encoding.zeroing
        }
        OpKind::X86Exp2 {
            dst,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            mask_zeroing,
            suppress_exceptions: false,
        } => {
            encoding.kind == X86EvexPackedFpUnaryMemoryKind::Exp2
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && actual_lanes == lanes
                && mask_zeroing == encoding.zeroing
        }
        OpKind::X86GetMantissa {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            imm,
            scalar: false,
            mask_zeroing,
            suppress_exceptions: false,
        }
        | OpKind::X86RoundScale {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            imm,
            scalar: false,
            mask_zeroing,
            suppress_exceptions: false,
        }
        | OpKind::X86Reduce {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            imm,
            scalar: false,
            mask_zeroing,
            suppress_exceptions: false,
        } => {
            let actual_kind = match op.kind {
                OpKind::X86GetMantissa { .. } => X86EvexPackedFpUnaryMemoryKind::GetMantissa,
                OpKind::X86RoundScale { .. } => X86EvexPackedFpUnaryMemoryKind::RoundScale,
                OpKind::X86Reduce { .. } => X86EvexPackedFpUnaryMemoryKind::Reduce,
                _ => unreachable!("matched packed immediate unary operation"),
            };
            encoding.kind == actual_kind
                && encoding.immediate == Some(imm)
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && actual_lanes == lanes
                && mask_zeroing == encoding.zeroing
        }
        OpKind::X86Recip14 {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            scalar: false,
            mask_zeroing,
        }
        | OpKind::X86Rsqrt14 {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            scalar: false,
            mask_zeroing,
        } => {
            let actual_kind = if matches!(op.kind, OpKind::X86Rsqrt14 { .. }) {
                X86EvexPackedFpUnaryMemoryKind::Rsqrt14
            } else {
                X86EvexPackedFpUnaryMemoryKind::Recip14
            };
            encoding.kind == actual_kind
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && actual_lanes == lanes
                && mask_zeroing == encoding.zeroing
        }
        OpKind::X86Recip28 {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            scalar: false,
            mask_zeroing,
            suppress_exceptions: false,
        }
        | OpKind::X86Rsqrt28 {
            dst,
            merge: None,
            src,
            mask,
            elem,
            width,
            lanes: actual_lanes,
            scalar: false,
            mask_zeroing,
            suppress_exceptions: false,
        } => {
            let actual_kind = if matches!(op.kind, OpKind::X86Rsqrt28 { .. }) {
                X86EvexPackedFpUnaryMemoryKind::Rsqrt28
            } else {
                X86EvexPackedFpUnaryMemoryKind::Recip28
            };
            encoding.kind == actual_kind
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && elem == encoding.elem
                && width == encoding.width
                && actual_lanes == lanes
                && mask_zeroing == encoding.zeroing
        }
        OpKind::X86RecipFp16 {
            dst,
            merge: None,
            src,
            mask,
            width,
            lanes: actual_lanes,
            scalar: false,
            mask_zeroing,
        }
        | OpKind::X86RsqrtFp16 {
            dst,
            merge: None,
            src,
            mask,
            width,
            lanes: actual_lanes,
            scalar: false,
            mask_zeroing,
        } => {
            let actual_kind = if matches!(op.kind, OpKind::X86RsqrtFp16 { .. }) {
                X86EvexPackedFpUnaryMemoryKind::RsqrtFp16
            } else {
                X86EvexPackedFpUnaryMemoryKind::RecipFp16
            };
            encoding.kind == actual_kind
                && encoding.elem == VecElementType::F16
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src == memory_source
                && mask == expected_mask
                && width == encoding.width
                && actual_lanes == lanes
                && mask_zeroing == encoding.zeroing
        }
        _ => false,
    }
}

fn exact_packed_sqrt_tail(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    memory_source: VReg,
    encoding: X86EvexPackedFpUnaryMemoryEncoding,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    if encoding.kind != X86EvexPackedFpUnaryMemoryKind::Sqrt {
        return None;
    }
    let op = block.ops.get(index)?;
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    if encoding.elem == VecElementType::F16 {
        let expected_mask = encoding
            .writemask
            .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
        return matches!(
            op.kind,
            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask,
                op: Avx10FP16Op::Sqrt,
                round: FpRoundMode::Dynamic,
                width,
                lanes: actual_lanes,
                zeroing,
            } if op.x86_hint.is_none()
                && vector_index(&dst, encoding.width) == Some(encoding.destination)
                && src1 == memory_source
                && src2 == memory_source
                && mask == expected_mask
                && width == encoding.width
                && actual_lanes == lanes
                && zeroing == encoding.zeroing
        )
        .then_some(1);
    }

    let expected_prefix = match encoding.elem {
        VecElementType::F32 => X86SsePrefix::None,
        VecElementType::F64 => X86SsePrefix::OpSize,
        _ => return None,
    };
    let raw = match op.kind {
        OpKind::X86Sqrt {
            dst,
            src,
            elem,
            lanes: actual_lanes,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        } if op.x86_hint
            == Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: expected_prefix,
                opcode: 0x51,
                width: encoding.width,
                w: encoding.w,
            })
            && src == memory_source
            && elem == encoding.elem
            && actual_lanes == lanes =>
        {
            dst
        }
        _ => return None,
    };

    let Some(mask_index) = encoding.writemask else {
        return (!encoding.zeroing
            && vector_index(&raw, encoding.width) == Some(encoding.destination))
        .then_some(1);
    };
    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(mask_index)));
    let mut offset = 1usize;
    exact_evex_vector_mask_result(
        block,
        index,
        &mut offset,
        op.guest_pc,
        raw,
        mask,
        encoding.width,
        encoding.elem,
        encoding.destination,
        encoding.zeroing,
        virtual_definitions,
        virtual_uses,
    )?;
    Some(offset)
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed
/// `VSQRT*`, `VGETEXP*`, `VGETMANT*`, `VRNDSCALE*`, `VREDUCE*`, `VEXP2*`,
/// `VRCP14*`, `VRSQRT14*`, `VRCP28*`, `VRSQRT28*`, `VRCPPH`, or `VRSQRTPH`
/// memory source.
///
/// Exact provenance binds the operation, precision, vector width,
/// architectural destination and writemask, merge/zero policy,
/// broadcast/full-vector tuple, helper address, per-lane fault suppression,
/// APX address guard, and sole architectural commit. Matching is O(L) time
/// and O(1) auxiliary space for at most 32 lanes; callers construct shared
/// definition/use maps in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_fp_unary_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedFpUnaryMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_packed_fp_unary_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexPackedFpUnaryMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexPackedFpUnaryMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexPackedFpUnaryMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: encoding.zeroing,
        vector_load_hint: None,
        form,
        memory_source_uses: usize::from(
            encoding.kind == X86EvexPackedFpUnaryMemoryKind::Sqrt
                && encoding.elem == VecElementType::F16,
        ) + 1,
    };
    let exact = exact_evex_e4_memory_sequence_tail(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |block, tail_index, memory_source| {
            if encoding.kind == X86EvexPackedFpUnaryMemoryKind::Sqrt {
                exact_packed_sqrt_tail(
                    block,
                    tail_index,
                    memory_source,
                    encoding,
                    virtual_definitions,
                    virtual_uses,
                )
            } else {
                exact_packed_fp_unary(block.ops.get(tail_index)?, memory_source, encoding)
                    .then_some(1)
            }
        },
    )?;
    Some(X86JitEvexPackedFpUnaryMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
