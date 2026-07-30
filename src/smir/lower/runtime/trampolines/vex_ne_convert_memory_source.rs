//! Fail-closed helper-backed AVX_NE_CONVERT memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexNeConvertKind, X86VexNeConvertMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact two-op decomposition consumed for one AVX_NE_CONVERT memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexNeConvertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexNeConvertMemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("AVX_NE_CONVERT has 128-/256-bit vector widths"),
    }))
}

/// Validate the complete memory-load/conversion pair for one
/// AVX_NE_CONVERT instruction.
///
/// Exact instruction-byte provenance binds the operation, destination,
/// reserved `vvvv`, vector length, and 2-/16-/32-byte memory footprint. The
/// loaded virtual must have exactly one definition and one use, the consumer
/// must be adjacent at the same guest PC, and no additional operation may
/// share that instruction frontier.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_ne_convert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexNeConvertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == load.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_ne_convert_memory_encoding()?;

    let loaded = if encoding.kind.broadcast() {
        match &load.kind {
            OpKind::Load {
                dst,
                addr,
                width: MemWidth::B2,
                sign: SignExtend::Zero,
            } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *dst,
            _ => return None,
        }
    } else {
        match &load.kind {
            OpKind::VLoad { dst, addr, width }
                if *width == encoding.width
                    && matches!(
                        load.x86_hint,
                        None | Some(X86OpHint::VecAlign(
                            X86VecAlign::Aligned | X86VecAlign::Unaligned
                        ))
                    )
                    && x86_jit_mem_address_shape_valid(addr) =>
            {
                *dst
            }
            _ => return None,
        }
    };
    if !matches!(loaded, VReg::Virtual(_))
        || virtual_definitions.get(&loaded) != Some(&1)
        || virtual_uses.get(&loaded) != Some(&1)
    {
        return None;
    }

    let conversion = block.ops.get(index + 1)?;
    if conversion.guest_pc != load.guest_pc
        || conversion.x86_hint.is_some()
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let semantics_match = match (encoding.kind, &conversion.kind) {
        (
            kind @ (X86VexNeConvertKind::BroadcastBf16
            | X86VexNeConvertKind::BroadcastFp16
            | X86VexNeConvertKind::EvenBf16
            | X86VexNeConvertKind::EvenFp16
            | X86VexNeConvertKind::OddBf16
            | X86VexNeConvertKind::OddFp16),
            OpKind::X86Convert16ToFp32 {
                dst,
                src,
                width,
                fp16,
                odd,
                broadcast,
            },
        ) => {
            *dst == vector(encoding.destination, encoding.width)
                && *src == loaded
                && *width == encoding.width
                && *fp16 == kind.fp16()
                && *odd == kind.odd()
                && *broadcast == kind.broadcast()
        }
        (
            X86VexNeConvertKind::Fp32ToBf16,
            OpKind::VCvtFP32ToBF16 {
                dst,
                src1,
                src2: None,
                mask: None,
                width,
                zeroing: false,
            },
        ) => {
            *dst == vector(encoding.destination, VecWidth::V128)
                && *src1 == loaded
                && *width == encoding.width
        }
        _ => false,
    };
    semantics_match.then_some(X86JitVexNeConvertMemorySequence {
        consumed: 2,
        encoding,
    })
}
