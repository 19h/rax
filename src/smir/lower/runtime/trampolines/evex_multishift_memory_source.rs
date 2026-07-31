//! Fail-closed helper-backed EVEX VPMULTISHIFTQB memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, X86Reg,
};
use crate::smir::ir::{
    X86EvexMultiShiftMemoryEncoding, X86EvexMultiShiftMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{single_definition_single_use, vector_index};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// VPMULTISHIFTQB memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexMultiShiftMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexMultiShiftMemoryEncoding,
}

/// Validate the complete O0/O1/O2 decomposition for one EVEX
/// VPMULTISHIFTQB memory source.
///
/// Exact provenance binds opcode, W, vector width, operands, writemask, and
/// broadcast while validating a complete memory encoding. The matcher
/// requires one unconditional E4NF memory operation with a supported SMIR
/// address, the exact terminal operation, confined virtuals, and the guest-PC
/// frontier. Runtime and auxiliary space are O(1); callers construct global
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_multishift_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexMultiShiftMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_multishift_memory_encoding()?;

    let (loaded, mut consumed) = match encoding.replay {
        X86EvexMultiShiftMemoryReplay::Vector { .. } => {
            let loaded = match &first.kind {
                OpKind::VLoad { dst, addr, width }
                    if first.x86_hint.is_none()
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
        }
        X86EvexMultiShiftMemoryReplay::Broadcast { .. } => {
            let scalar = match &first.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                } if first.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *dst,
                _ => return None,
            };
            if !single_definition_single_use(scalar, virtual_definitions, virtual_uses) {
                return None;
            }
            let broadcast = block.ops.get(index + 1)?;
            let loaded = match broadcast.kind {
                OpKind::VBroadcast {
                    dst,
                    scalar: actual_scalar,
                    elem: VecElementType::I64,
                    lanes,
                } if broadcast.x86_hint.is_none()
                    && broadcast.guest_pc == guest_pc
                    && actual_scalar == scalar
                    && lanes == encoding.width.lanes(VecElementType::I64) as u8 =>
                {
                    dst
                }
                _ => return None,
            };
            if !single_definition_single_use(loaded, virtual_definitions, virtual_uses) {
                return None;
            }
            (loaded, 2)
        }
    };

    let operation = block.ops.get(index + consumed)?;
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));
    if operation.x86_hint.is_some()
        || operation.guest_pc != guest_pc
        || !matches!(
            operation.kind,
            OpKind::X86MultiShiftQB {
                dst,
                control,
                source,
                mask,
                width,
                zeroing,
            } if vector_index(&dst, encoding.width) == Some(encoding.destination)
                && vector_index(&control, encoding.width) == Some(encoding.control)
                && source == loaded
                && mask == expected_mask
                && width == encoding.width
                && zeroing == encoding.zeroing
        )
    {
        return None;
    }
    consumed += 1;
    if block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == guest_pc)
    {
        return None;
    }

    Some(X86JitEvexMultiShiftMemorySequence {
        consumed,
        address_offset: 0,
        memory_size: encoding.memory_size,
        encoding,
    })
}
