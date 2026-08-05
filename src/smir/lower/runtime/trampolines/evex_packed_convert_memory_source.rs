//! Fail-closed helper-backed EVEX packed conversion memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, BlockId, GuestAddr, VReg, X86Reg};
use crate::smir::ir::{
    X86EvexPackedConvertMemoryEncoding, X86EvexPackedConvertMemoryKind,
    X86EvexPackedConvertMemoryReplay, X86InstructionBytes,
};

use super::evex_memory_source_common::{
    X86EvexE4MemoryReplayForm, X86EvexE4MemoryShape, exact_evex_e4_memory_sequence,
};

/// Exact EVEX packed conversion memory decomposition consumed by x86-64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexPackedConvertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86EvexPackedConvertMemoryEncoding,
}

fn prefix(pp: u8) -> Option<X86SsePrefix> {
    match pp {
        0 => Some(X86SsePrefix::None),
        1 => Some(X86SsePrefix::OpSize),
        2 => Some(X86SsePrefix::Rep),
        3 => Some(X86SsePrefix::Repne),
        _ => None,
    }
}

fn exact_conversion(
    op: &crate::smir::ir::ops::SmirOp,
    source: VReg,
    encoding: X86EvexPackedConvertMemoryEncoding,
) -> bool {
    if op.x86_hint
        != Some(X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: match prefix(encoding.pp) {
                Some(pp) => pp,
                None => return false,
            },
            opcode: encoding.opcode,
            width: encoding.operation_width,
            w: encoding.w,
        })
    {
        return false;
    }
    let expected_destination = match encoding.destination_width {
        crate::smir::ir::types::VecWidth::V128 => {
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(encoding.destination)))
        }
        crate::smir::ir::types::VecWidth::V256 => {
            VReg::Arch(ArchReg::X86(X86Reg::Ymm(encoding.destination)))
        }
        crate::smir::ir::types::VecWidth::V512 => {
            VReg::Arch(ArchReg::X86(X86Reg::Zmm(encoding.destination)))
        }
        _ => return false,
    };
    let expected_mask = encoding
        .writemask
        .map(|mask| VReg::Arch(ArchReg::X86(X86Reg::K(mask))));

    match (encoding.kind, op.kind.clone()) {
        (
            X86EvexPackedConvertMemoryKind::FpPrecision { from, to },
            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from: actual_from,
                to: actual_to,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: false,
                report_fp16_denormal: false,
            },
        ) => {
            dst == expected_destination
                && src == source
                && mask == expected_mask
                && actual_from == from
                && actual_to == to
                && lanes == encoding.lanes
                && dst_width == encoding.destination_width
                && mask_zeroing == encoding.zeroing
                && round == encoding.kind.round()
        }
        (
            X86EvexPackedConvertMemoryKind::IntToFp {
                int_elem,
                fp_elem,
                signed,
            },
            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem: actual_int_elem,
                fp_elem: actual_fp_elem,
                signed: actual_signed,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: false,
            },
        ) => {
            dst == expected_destination
                && src == source
                && mask == expected_mask
                && actual_int_elem == int_elem
                && actual_fp_elem == fp_elem
                && actual_signed == signed
                && lanes == encoding.lanes
                && src_width == encoding.source_width
                && dst_width == encoding.destination_width
                && mask_zeroing == encoding.zeroing
                && round == encoding.kind.round()
        }
        (
            X86EvexPackedConvertMemoryKind::FpToInt {
                fp_elem,
                int_elem,
                signed,
                truncate,
            },
            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem: actual_fp_elem,
                int_elem: actual_int_elem,
                signed: actual_signed,
                truncate: actual_truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: false,
            },
        ) => {
            dst == expected_destination
                && src == source
                && mask == expected_mask
                && actual_fp_elem == fp_elem
                && actual_int_elem == int_elem
                && actual_signed == signed
                && actual_truncate == truncate
                && lanes == encoding.lanes
                && src_width == encoding.source_width
                && dst_width == encoding.destination_width
                && mask_zeroing == encoding.zeroing
                && round == encoding.kind.round()
        }
        _ => false,
    }
}

/// Validate the complete O0/O1/O2 decomposition for any selected EVEX
/// packed F32/F64/I32/I64 conversion memory source.
///
/// Exact provenance binds all 26 mnemonics, operation/source/destination
/// widths, signedness, truncation, masks, broadcast, APX address frontier,
/// MXCSR control, and the terminal guest-PC boundary. Matching is O(L) time
/// and O(1) auxiliary space for L <= 16 source lanes; callers construct
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_packed_convert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexPackedConvertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let encoding = instruction_bytes
        .get(&(block.id, first.guest_pc))?
        .evex_packed_convert_memory_encoding()?;
    let form = match encoding.replay {
        X86EvexPackedConvertMemoryReplay::Vector { .. } => X86EvexE4MemoryReplayForm::Vector,
        X86EvexPackedConvertMemoryReplay::Broadcast { .. } => X86EvexE4MemoryReplayForm::Broadcast,
        X86EvexPackedConvertMemoryReplay::MaskedVector { .. } => {
            X86EvexE4MemoryReplayForm::MaskedVector
        }
    };
    let exact = exact_evex_e4_memory_sequence(
        block,
        index,
        X86EvexE4MemoryShape {
            width: encoding.source_width,
            elem: encoding.kind.source_elem(),
            writemask: encoding.writemask,
            zeroing: encoding.zeroing,
            vector_load_hint: None,
            form,
            memory_source_uses: 1,
        },
        virtual_definitions,
        virtual_uses,
        |conversion, source| exact_conversion(conversion, source, encoding),
    )?;
    Some(X86JitEvexPackedConvertMemorySequence {
        consumed: exact.consumed,
        address_offset: exact.address_offset,
        memory_size: exact.memory_size,
        encoding,
    })
}
