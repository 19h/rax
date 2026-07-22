//! Fail-closed admission for helper-backed scalar MOVBE memory sequences.

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, OpWidth, SignExtend, VReg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86JitMovbeMemoryDirection {
    Load,
    Store,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitMovbeMemorySequence {
    pub(crate) direction: X86JitMovbeMemoryDirection,
    pub(crate) width: OpWidth,
    pub(crate) consumed: usize,
}

/// Validate one exact MOVBE memory pair emitted by the x86 lifter:
/// `Load virtual; Bswap architectural_dst,virtual` or
/// `Bswap virtual,architectural_src; Store virtual`.
///
/// The virtual is elided by helper-backed lowering, so it must have one
/// definition and one use. Every other shape remains rejected by the identity
/// register-map gate.
pub(crate) fn x86_jit_movbe_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitMovbeMemorySequence> {
    if !allow_mem {
        return None;
    }

    let architectural_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());
    let first = block.ops.get(index)?;
    let second = block.ops.get(index + 1)?;
    if first.guest_pc != second.guest_pc || first.x86_hint.is_some() || second.x86_hint.is_some() {
        return None;
    }

    let sequence = match (&first.kind, &second.kind) {
        (
            OpKind::Load {
                dst: temporary @ VReg::Virtual(_),
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
            },
            OpKind::Bswap { dst, src, width },
        ) if src == temporary
            && architectural_gpr(dst)
            && mem_width.to_op_width() == Some(*width)
            && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
            && super::x86_jit_mem_address_shape_valid(addr) =>
        {
            (*temporary, X86JitMovbeMemoryDirection::Load, *width)
        }
        (
            OpKind::Bswap {
                dst: temporary @ VReg::Virtual(_),
                src,
                width,
            },
            OpKind::Store {
                src: stored,
                addr,
                width: mem_width,
            },
        ) if stored == temporary
            && architectural_gpr(src)
            && mem_width.to_op_width() == Some(*width)
            && matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
            && super::x86_jit_mem_address_shape_valid(addr) =>
        {
            (*temporary, X86JitMovbeMemoryDirection::Store, *width)
        }
        _ => return None,
    };

    (virtual_definitions.get(&sequence.0) == Some(&1) && virtual_uses.get(&sequence.0) == Some(&1))
        .then_some(X86JitMovbeMemorySequence {
            direction: sequence.1,
            width: sequence.2,
            consumed: 2,
        })
}

pub(crate) fn x86_jit_movbe_memory_sequence_len(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_movbe_memory_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|sequence| sequence.consumed)
}
