//! Fail-closed helper-backed AMD VEX FMA4 memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, VecWidth,
    X86FmaOrder, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexFma4MemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous scalar or packed FMA4 memory decomposition consumed by
/// the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexFma4MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexFma4MemoryEncoding,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated FMA4 vector width"),
    }))
}

fn single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&1)
        && virtual_uses.get(&register) == Some(&1)
}

/// Validate the complete 3-op packed or 4-op scalar decomposition emitted for
/// one FMA4 memory source.
///
/// The byte classifier binds map, prefix, opcode, W/L, destination,
/// VEX.vvvv, `/is4`, and memory width to the semantic graph. Every virtual
/// memory/result value must have exactly one definition and one use in the
/// sequence. Runtime is O(1); callers construct definition/use maps once in
/// O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_fma4_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexFma4MemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_fma4_memory_encoding()?;

    let (memory_vector, fma_offset) = if encoding.scalar {
        let expected_width = match encoding.elem {
            VecElementType::F32 => MemWidth::B4,
            VecElementType::F64 => MemWidth::B8,
            _ => return None,
        };
        let loaded_scalar = match &load.kind {
            OpKind::Load {
                dst,
                addr,
                width,
                sign: SignExtend::Zero,
            } if load.x86_hint.is_none()
                && *width == expected_width
                && x86_jit_mem_address_shape_valid(addr) =>
            {
                *dst
            }
            _ => return None,
        };
        if !single_definition_single_use(loaded_scalar, virtual_definitions, virtual_uses) {
            return None;
        }

        let broadcast = block.ops.get(index + 1)?;
        let loaded = match &broadcast.kind {
            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes: 1,
            } if broadcast.guest_pc == load.guest_pc
                && broadcast.x86_hint.is_none()
                && *scalar == loaded_scalar
                && *elem == encoding.elem =>
            {
                *dst
            }
            _ => return None,
        };
        if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
            return None;
        }
        (loaded, 2)
    } else {
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
        (loaded, 1)
    };

    let fma = block.ops.get(index + fma_offset)?;
    let raw = match &fma.kind {
        OpKind::X86Fma(operation)
            if fma.guest_pc == load.guest_pc
                && fma.x86_hint
                    == Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F3A,
                        pp: X86SsePrefix::OpSize,
                        opcode: encoding.opcode,
                        width: encoding.width,
                        w: encoding.w,
                    })
                && operation.src1 == vector_reg(encoding.source1, encoding.width)
                && operation.src2
                    == if encoding.w {
                        vector_reg(encoding.is4, encoding.width)
                    } else {
                        memory_vector
                    }
                && operation.src3
                    == if encoding.w {
                        memory_vector
                    } else {
                        vector_reg(encoding.is4, encoding.width)
                    }
                && operation.mask.is_none()
                && operation.elem == encoding.elem
                && operation.kind == encoding.kind
                && operation.order == X86FmaOrder::Order123
                && operation.round == FpRoundMode::Dynamic
                && operation.lanes
                    == if encoding.scalar {
                        1
                    } else {
                        encoding.width.lanes(encoding.elem) as u8
                    } =>
        {
            operation.dst
        }
        _ => return None,
    };
    if !single_definition_single_use(raw, virtual_definitions, virtual_uses) {
        return None;
    }

    let result_offset = fma_offset + 1;
    let result = block.ops.get(index + result_offset)?;
    if result.guest_pc != load.guest_pc
        || result.x86_hint.is_some()
        || !matches!(
            result.kind,
            OpKind::VMov {
                dst,
                src,
                width,
            } if dst == vector_reg(encoding.destination, encoding.width)
                && src == raw
                && width == encoding.width
        )
    {
        return None;
    }
    let consumed = result_offset + 1;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    Some(X86JitVexFma4MemorySequence { consumed, encoding })
}
