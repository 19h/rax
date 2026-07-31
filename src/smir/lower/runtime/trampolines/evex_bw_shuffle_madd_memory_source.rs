//! Fail-closed helper-backed EVEX AVX-512BW memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, VecElementType, X86Reg};
use crate::smir::ir::{
    X86EvexBwShuffleMaddKind, X86EvexBwShuffleMaddMemoryEncoding, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    exact_evex_vector_mask_result, single_definition_single_use, vector_index,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed by the helper-backed x86-64 EVEX
/// VPSHUFB/VPMADDUBSW/VPMADDWD memory lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexBwShuffleMaddMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexBwShuffleMaddMemoryEncoding,
}

fn no_following_same_pc(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    consumed: usize,
    guest_pc: GuestAddr,
) -> bool {
    !block
        .ops
        .get(index + consumed)
        .is_some_and(|op| op.guest_pc == guest_pc)
}

fn expected_load_hint(kind: X86EvexBwShuffleMaddKind) -> Option<X86OpHint> {
    match kind {
        X86EvexBwShuffleMaddKind::MultiplyAddWords => {
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
        }
        X86EvexBwShuffleMaddKind::ByteShuffle
        | X86EvexBwShuffleMaddKind::MultiplyAddUnsignedBytes => None,
    }
}

/// Validate the complete O0/O1/O2 decomposition for one EVEX VPSHUFB,
/// VPMADDUBSW, or VPMADDWD Full Mem source.
///
/// Exact provenance binds the operation, WIG value, operands, vector width,
/// writemask policy, and complete-tuple access. The matcher consumes the
/// unconditional E4NF.nb load, exact semantic operation, every merge/zero
/// lane, all virtual definition/use counts, and the guest-PC frontier.
/// Runtime is O(L) and auxiliary space is O(1) for L <= 64 result lanes;
/// callers construct definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_bw_shuffle_madd_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexBwShuffleMaddMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let guest_pc = load.guest_pc;
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_bw_shuffle_madd_memory_encoding()?;
    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == expected_load_hint(encoding.kind)
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

    let operation = block.ops.get(index + 1)?;
    if operation.guest_pc != guest_pc {
        return None;
    }
    let raw = match encoding.kind {
        X86EvexBwShuffleMaddKind::ByteShuffle => {
            if operation.x86_hint
                != Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x00,
                    width: encoding.width,
                    w: encoding.w,
                })
            {
                return None;
            }
            let OpKind::VByteShuffle {
                dst,
                src,
                control,
                lanes,
                block_lanes: 16,
            } = operation.kind
            else {
                return None;
            };
            if vector_index(&src, encoding.width) != Some(encoding.source1)
                || control != loaded
                || lanes != encoding.width.lanes(VecElementType::I8) as u8
            {
                return None;
            }
            if encoding.writemask.is_none() {
                if encoding.zeroing
                    || vector_index(&dst, encoding.width) != Some(encoding.destination)
                {
                    return None;
                }
                None
            } else if matches!(dst, VReg::Virtual(_)) {
                Some(dst)
            } else {
                return None;
            }
        }
        X86EvexBwShuffleMaddKind::MultiplyAddUnsignedBytes
        | X86EvexBwShuffleMaddKind::MultiplyAddWords => {
            if operation.x86_hint.is_some() {
                return None;
            }
            let OpKind::VDotProduct {
                dst,
                acc: VReg::Imm(0),
                src1,
                src2,
                mask: None,
                src_elem,
                acc_elem,
                width,
                src1_unsigned,
                saturate,
                zeroing: false,
            } = operation.kind
            else {
                return None;
            };
            let expected = match encoding.kind {
                X86EvexBwShuffleMaddKind::MultiplyAddUnsignedBytes => {
                    (VecElementType::I8, VecElementType::I16, true, true)
                }
                X86EvexBwShuffleMaddKind::MultiplyAddWords => {
                    (VecElementType::I16, VecElementType::I32, false, false)
                }
                X86EvexBwShuffleMaddKind::ByteShuffle => unreachable!(),
            };
            if !matches!(dst, VReg::Virtual(_))
                || vector_index(&src1, encoding.width) != Some(encoding.source1)
                || src2 != loaded
                || (src_elem, acc_elem, src1_unsigned, saturate) != expected
                || width != encoding.width
            {
                return None;
            }
            Some(dst)
        }
    };
    let mut offset = 2;

    if let Some(mask_index) = encoding.writemask {
        let raw = raw?;
        let result_elem = match encoding.kind {
            X86EvexBwShuffleMaddKind::ByteShuffle => VecElementType::I8,
            X86EvexBwShuffleMaddKind::MultiplyAddUnsignedBytes => VecElementType::I16,
            X86EvexBwShuffleMaddKind::MultiplyAddWords => VecElementType::I32,
        };
        exact_evex_vector_mask_result(
            block,
            index,
            &mut offset,
            guest_pc,
            raw,
            VReg::Arch(ArchReg::X86(X86Reg::K(mask_index))),
            encoding.width,
            result_elem,
            encoding.destination,
            encoding.zeroing,
            virtual_definitions,
            virtual_uses,
        )?;
    } else if let Some(raw) = raw {
        if encoding.zeroing || !single_definition_single_use(raw, virtual_definitions, virtual_uses)
        {
            return None;
        }
        let commit = block.ops.get(index + offset)?;
        if commit.x86_hint.is_some()
            || commit.guest_pc != guest_pc
            || !matches!(
                commit.kind,
                OpKind::VMov {
                    dst,
                    src,
                    width,
                } if vector_index(&dst, encoding.width) == Some(encoding.destination)
                    && src == raw
                    && width == encoding.width
            )
        {
            return None;
        }
        offset += 1;
    }

    if !no_following_same_pc(block, index, offset, guest_pc) {
        return None;
    }
    Some(X86JitEvexBwShuffleMaddMemorySequence {
        consumed: offset,
        address_offset: 0,
        memory_size: encoding.memory_size,
        encoding,
    })
}
