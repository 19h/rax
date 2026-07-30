//! Fail-closed helper-backed VEX packed-string memory-source admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecWidth, X86Reg};
use crate::smir::ir::{X86InstructionBytes, X86VexPackedStringMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX packed-string memory-source decomposition consumed by
/// the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexPackedStringMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexPackedStringMemoryEncoding,
}

/// Validate one complete `VLoad` plus `X86PackedStringCompare` memory graph.
///
/// Instruction provenance binds the mandatory prefix, W/L/vvvv fields,
/// opcode, imm8, architectural inputs/output, and 16-byte memory footprint.
/// The loaded virtual must have exactly one definition and one use, and the
/// two operations must be the complete same-PC instruction graph. An aligned
/// load hint is accepted only because O2 may infer stronger pointer alignment;
/// the source instruction itself remains unaligned-capable.
///
/// Classification is O(1) time and O(1) auxiliary space. Callers construct
/// definition/use maps once in O(N) time and O(V) space for N operations and V
/// virtual registers.
pub(crate) fn x86_jit_vex_packed_string_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexPackedStringMemorySequence> {
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
            .get(index.checked_add(2)?)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let encoding = instruction_bytes
        .get(&(block.id, load.guest_pc))?
        .vex_packed_string_memory_encoding()?;
    let loaded = match &load.kind {
        OpKind::VLoad {
            dst,
            addr,
            width: VecWidth::V128,
        } if matches!(
            load.x86_hint,
            None | Some(X86OpHint::VecAlign(
                X86VecAlign::Unaligned | X86VecAlign::Aligned
            ))
        ) && matches!(dst, VReg::Virtual(_))
            && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if virtual_definitions.get(&loaded) != Some(&1) || virtual_uses.get(&loaded) != Some(&1) {
        return None;
    }

    let OpKind::X86PackedStringCompare {
        dst,
        src1,
        src2,
        len1,
        len2,
        length_width,
        kind,
        imm,
        zero_upper,
    } = &operation.kind
    else {
        return None;
    };
    let expected_destination = VReg::Arch(ArchReg::X86(if encoding.kind.returns_mask() {
        X86Reg::Xmm(0)
    } else {
        X86Reg::Rcx
    }));
    let expected_source1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.source1)));
    let (expected_len1, expected_len2) = if encoding.kind.is_explicit() {
        (
            Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
            Some(VReg::Arch(ArchReg::X86(X86Reg::Rdx))),
        )
    } else {
        (None, None)
    };
    if *dst != expected_destination
        || *src1 != expected_source1
        || *src2 != loaded
        || *len1 != expected_len1
        || *len2 != expected_len2
        || *length_width != encoding.length_width
        || *kind != encoding.kind
        || *imm != encoding.immediate
        || *zero_upper != encoding.kind.returns_mask()
    {
        return None;
    }

    Some(X86JitVexPackedStringMemorySequence {
        consumed: 2,
        encoding,
    })
}
