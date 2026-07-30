//! Fail-closed helper-backed unmasked EVEX packed-logical memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{BlockId, GuestAddr, VReg, VecElementType};
use crate::smir::ir::{X86EvexLogicMemoryEncoding, X86EvexLogicMemoryKind, X86InstructionBytes};

use super::evex_memory_source_common::{single_definition_single_use, vector_index};
use super::x86_jit_mem_address_shape_valid;

/// Exact two-op decomposition consumed by the helper-backed x86-64 EVEX
/// packed-logical full-vector memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexLogicMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86EvexLogicMemoryEncoding,
}

fn exact_logic_consumer(
    op: &crate::smir::ir::ops::SmirOp,
    loaded: VReg,
    encoding: X86EvexLogicMemoryEncoding,
) -> bool {
    let operands_match = match (encoding.kind, &op.kind) {
        (
            X86EvexLogicMemoryKind::And,
            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            },
        )
        | (
            X86EvexLogicMemoryKind::AndNot,
            OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            },
        )
        | (
            X86EvexLogicMemoryKind::Or,
            OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            },
        )
        | (
            X86EvexLogicMemoryKind::Xor,
            OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            },
        ) => {
            vector_index(dst, encoding.width) == Some(encoding.destination)
                && vector_index(src1, encoding.width) == Some(encoding.source1)
                && *src2 == loaded
                && *width == encoding.width
        }
        _ => false,
    };
    let prefix = match encoding.elem {
        VecElementType::F32 => X86SsePrefix::None,
        VecElementType::F64 | VecElementType::I32 | VecElementType::I64 => X86SsePrefix::OpSize,
        _ => return false,
    };
    operands_match
        && op.x86_hint
            == Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: prefix,
                opcode: match encoding.kind {
                    X86EvexLogicMemoryKind::And
                        if matches!(encoding.elem, VecElementType::I32 | VecElementType::I64) =>
                    {
                        0xDB
                    }
                    X86EvexLogicMemoryKind::AndNot
                        if matches!(encoding.elem, VecElementType::I32 | VecElementType::I64) =>
                    {
                        0xDF
                    }
                    X86EvexLogicMemoryKind::Or
                        if matches!(encoding.elem, VecElementType::I32 | VecElementType::I64) =>
                    {
                        0xEB
                    }
                    X86EvexLogicMemoryKind::Xor
                        if matches!(encoding.elem, VecElementType::I32 | VecElementType::I64) =>
                    {
                        0xEF
                    }
                    X86EvexLogicMemoryKind::And => 0x54,
                    X86EvexLogicMemoryKind::AndNot => 0x55,
                    X86EvexLogicMemoryKind::Or => 0x56,
                    X86EvexLogicMemoryKind::Xor => 0x57,
                },
                width: encoding.width,
                w: matches!(encoding.elem, VecElementType::F64 | VecElementType::I64),
            })
}

/// Validate the complete O0/O1/O2 two-op decomposition emitted for one
/// unmasked EVEX packed-logical full-vector memory source. Exact instruction
/// provenance binds operation, element class, width, registers, and the
/// helper-loaded virtual.
///
/// Classification is O(1) time and auxiliary space; callers build definition
/// and use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_logic_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexLogicMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(dst, VReg::Virtual(_))
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            let encoding = instruction_bytes
                .get(&(block.id, load.guest_pc))?
                .evex_logic_memory_encoding()?;
            if *width != encoding.width {
                return None;
            }
            *dst
        }
        _ => return None,
    };
    if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .evex_logic_memory_encoding()?;
    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc
        || !exact_logic_consumer(consumer, loaded, encoding)
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    Some(X86JitEvexLogicMemorySequence {
        consumed: 2,
        encoding,
    })
}
