//! Fail-closed helper-backed EVEX VSCALEF memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{
    X86EvexScaleFMemoryEncoding, X86EvexScaleFMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence, vector_index,
};

/// Exact contiguous decomposition consumed by the helper-backed x86-64
/// VSCALEFPD/PS/PH/SD/SS/SH memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexScaleFMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexScaleFMemoryEncoding,
}

fn exact_scale_f(
    op: &crate::smir::ir::ops::SmirOp,
    memory_source: VReg,
    encoding: X86EvexScaleFMemoryEncoding,
) -> bool {
    let expected_mask = encoding
        .writemask
        .map(|index| VReg::Arch(ArchReg::X86(X86Reg::K(index))));
    let expected_map = if encoding.elem == VecElementType::F16 {
        X86VecMap::Map6
    } else {
        X86VecMap::Map0F38
    };
    matches!(
        op.kind,
        OpKind::X86ScaleF {
            dst,
            src1,
            src2,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
            round,
            suppress_exceptions,
        } if vector_index(&dst, encoding.width) == Some(encoding.destination)
            && vector_index(&src1, encoding.width) == Some(encoding.source1)
            && src2 == memory_source
            && mask == expected_mask
            && elem == encoding.elem
            && width == encoding.width
            && lanes == if encoding.scalar {
                1
            } else {
                encoding.width.lanes(encoding.elem) as u8
            }
            && scalar == encoding.scalar
            && mask_zeroing == encoding.zeroing
            && round == FpRoundMode::Dynamic
            && !suppress_exceptions
            && op.x86_hint == Some(X86OpHint::EvexOp {
                map: expected_map,
                pp: X86SsePrefix::OpSize,
                opcode: if encoding.scalar { 0x2D } else { 0x2C },
                width: encoding.width,
                w: encoding.elem == VecElementType::F64,
            })
    )
}

/// Validate the complete O0/O1/O2 decomposition emitted for one packed or
/// scalar AVX-512 VSCALEF memory source.
///
/// Exact provenance binds scalar/packed class, precision, vector width,
/// architectural operands, writemask policy, broadcast/full-vector tuple,
/// helper address, dynamic MXCSR rounding, exception behavior, and the sole
/// architectural destination commit. Classification is O(L) time and O(1)
/// auxiliary space for L <= 32 lanes; callers build definition/use maps once
/// in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_scale_f_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexScaleFMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_scale_f_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexScaleFMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexScaleFMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexScaleFMemoryReplay::MaskedVector { .. } => X86EvexE4MemoryReplayForm::MaskedVector,
        X86EvexScaleFMemoryReplay::Scalar { .. } => X86EvexE4MemoryReplayForm::Scalar,
    };
    let shape = X86EvexE4MemoryShape {
        width: encoding.width,
        elem: encoding.elem,
        writemask: encoding.writemask,
        zeroing: encoding.zeroing,
        vector_load_hint: None,
        form,
    };
    let exact = exact_evex_e4_memory_sequence(
        block,
        index,
        shape,
        virtual_definitions,
        virtual_uses,
        |op, memory_source| exact_scale_f(op, memory_source, encoding),
    )?;
    Some(X86JitEvexScaleFMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
