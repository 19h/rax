//! Fail-closed helper-backed VEX `VMOVNTDQA` memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::X86VexMovntdqaMemoryEncoding;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecWidth, X86Reg};
use crate::smir::ir::{SmirBlock, X86InstructionBytes};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VMOVNTDQA` sequence consumed by the helper-backed
/// x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexMovntdqaMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexMovntdqaMemoryEncoding,
}

fn low_vex_destination(reg: &VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(*index),
        _ => None,
    }
}

/// Validate one complete `X86CheckAlignment`/`VLoad`/`VMov` group derived
/// from a VEX.128/256 `VMOVNTDQA` memory-source instruction.
///
/// The alignment guard and load must use the same state-backed address, and
/// the required alignment must equal the encoded transfer width. The loaded
/// virtual must have exactly one definition and use. Exact byte provenance
/// binds the VEX map, mandatory prefix, reserved vvvv field, destination,
/// vector length, WIG value, memory-only form, and complete instruction
/// length.
///
/// Classification is O(1). Callers construct definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_movntdqa_memory_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexMovntdqaMemorySequence> {
    if !allow_mem {
        return None;
    }
    let guard = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == guard.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, guard.guest_pc))?;
    let encoding = instruction.vex_movntdqa_memory_encoding()?;

    let (guard_address, alignment) = match &guard.kind {
        OpKind::X86CheckAlignment { addr, alignment }
            if guard.x86_hint.is_none()
                && x86_jit_mem_address_shape_valid(addr)
                && u32::from(*alignment) == encoding.width.bytes() =>
        {
            (addr, *alignment)
        }
        _ => return None,
    };

    let load = block.ops.get(index + 1)?;
    if load.guest_pc != guard.guest_pc {
        return None;
    }
    let temporary = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(dst, VReg::Virtual(_))
                && addr == guard_address
                && *width == encoding.width
                && load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Aligned)) =>
        {
            *dst
        }
        _ => return None,
    };
    if virtual_definitions.get(&temporary) != Some(&1) || virtual_uses.get(&temporary) != Some(&1) {
        return None;
    }

    let write = block.ops.get(index + 2)?;
    if write.guest_pc != guard.guest_pc
        || write.x86_hint.is_some()
        || block
            .ops
            .get(index + 3)
            .is_some_and(|op| op.guest_pc == guard.guest_pc)
    {
        return None;
    }
    let OpKind::VMov { dst, src, width } = write.kind else {
        return None;
    };
    if src != temporary
        || width != encoding.width
        || low_vex_destination(&dst, width) != Some(encoding.destination)
        || u32::from(alignment) != width.bytes()
    {
        return None;
    }

    Some(X86JitVexMovntdqaMemorySequence {
        consumed: 3,
        encoding,
    })
}
