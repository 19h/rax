//! Fail-closed helper-backed VEX SM3/SM4 memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecWidth, X86Reg};
use crate::smir::ir::{X86InstructionBytes, X86VexSm3Sm4MemoryEncoding, X86VexSm3Sm4MemoryKind};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX SM3/SM4 memory-source decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexSm3Sm4MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexSm3Sm4MemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("SM3/SM4 has 128-/256-bit vector widths"),
    }))
}

/// Validate the exact `VLoad` plus SM3/SM4 operation emitted for one memory
/// source.
///
/// Instruction provenance binds the map, prefix, W/L fields, opcode, imm8,
/// architectural operands, memory width, and native register-source rewrite.
/// The loaded virtual must have exactly one definition and one use, and the
/// two operations must be the complete same-PC instruction graph.
///
/// Classification is O(1) time and O(1) auxiliary space. Callers construct
/// definition/use maps once in O(N) time and O(V) space for N operations and V
/// virtual registers.
pub(crate) fn x86_jit_vex_sm3_sm4_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexSm3Sm4MemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let operation = block.ops.get(index.checked_add(1)?)?;
    if operation.guest_pc != load.guest_pc
        || operation.x86_hint.is_some()
        || index
            .checked_sub(1)
            .and_then(|previous| block.ops.get(previous))
            .is_some_and(|op| op.guest_pc == load.guest_pc)
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .vex_sm3_sm4_memory_encoding()?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if *width == encoding.width
                && matches!(
                    load.x86_hint,
                    Some(X86OpHint::VecAlign(
                        X86VecAlign::Unaligned | X86VecAlign::Aligned
                    ))
                )
                && matches!(dst, VReg::Virtual(_))
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if virtual_definitions.get(&loaded) != Some(&1) || virtual_uses.get(&loaded) != Some(&1) {
        return None;
    }

    let destination = vector(encoding.destination, encoding.width);
    let source1 = vector(encoding.source1, encoding.width);
    let exact_operation = match (&operation.kind, encoding.kind) {
        (OpKind::X86Sm3Msg1 { dst, src1, src2 }, X86VexSm3Sm4MemoryKind::Sm3Msg1)
        | (OpKind::X86Sm3Msg2 { dst, src1, src2 }, X86VexSm3Sm4MemoryKind::Sm3Msg2) => {
            *dst == destination && *src1 == source1 && *src2 == loaded
        }
        (
            OpKind::X86Sm3Rounds2 {
                dst,
                state,
                words,
                imm,
            },
            X86VexSm3Sm4MemoryKind::Sm3Rounds2,
        ) => {
            *dst == destination
                && *state == source1
                && *words == loaded
                && encoding.immediate == Some(*imm)
        }
        (
            OpKind::X86Sm4 {
                dst,
                src1,
                src2,
                width,
                key_schedule,
            },
            kind @ (X86VexSm3Sm4MemoryKind::Sm4Key4 | X86VexSm3Sm4MemoryKind::Sm4Rounds4),
        ) => {
            *dst == destination
                && *src1 == source1
                && *src2 == loaded
                && *width == encoding.width
                && *key_schedule == (kind == X86VexSm3Sm4MemoryKind::Sm4Key4)
        }
        _ => false,
    };
    exact_operation.then_some(X86JitVexSm3Sm4MemorySequence {
        consumed: 2,
        encoding,
    })
}
