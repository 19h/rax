//! Fail-closed helper-backed VEX packed-conversion memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, FpRoundMode, GuestAddr, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86InstructionBytes, X86VexPackedConvertMemoryEncoding, X86VexPackedConvertMemoryKind,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact two-op decomposition consumed for one defined VEX packed conversion
/// whose source is memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexPackedConvertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexPackedConvertMemoryEncoding,
}

fn vector(index: u8, width: VecWidth) -> Option<VReg> {
    let register = match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => return None,
    };
    Some(VReg::Arch(ArchReg::X86(register)))
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

/// Validate an exact `VLoad` plus packed-conversion pair.
///
/// Complete source-byte provenance binds the opcode, WIG bit, vector length,
/// destination, reserved `vvvv`, and 8-/16-/32-byte memory tuple. The loaded
/// virtual must have exactly one definition and one use, the consumer must be
/// adjacent at the same guest PC, and no additional operation may share the
/// instruction boundary. The load may remain unhinted or carry the aligned
/// hint established by vector-alignment inference.
///
/// Classification is O(1); callers construct definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_packed_convert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexPackedConvertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == load.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_packed_convert_memory_encoding()?;

    let loaded = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if *width == encoding.source_width
                && matches!(
                    load.x86_hint,
                    None | Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                )
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            *dst
        }
        _ => return None,
    };
    if !matches!(loaded, VReg::Virtual(_))
        || virtual_definitions.get(&loaded) != Some(&1)
        || virtual_uses.get(&loaded) != Some(&1)
    {
        return None;
    }

    let conversion = block.ops.get(index + 1)?;
    if conversion.guest_pc != load.guest_pc
        || conversion.x86_hint
            != Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: prefix(encoding.pp)?,
                opcode: encoding.opcode,
                width: encoding.operation_width,
                w: encoding.w,
            })
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let destination = vector(encoding.destination, encoding.destination_width)?;
    let semantics_match = match (encoding.kind, &conversion.kind) {
        (
            X86VexPackedConvertMemoryKind::FpPrecision { from, to },
            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask: None,
                from: op_from,
                to: op_to,
                lanes,
                dst_width,
                mask_zeroing: false,
                zero_upper: true,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                report_fp16_denormal: false,
            },
        ) => {
            *dst == destination
                && *src == loaded
                && *op_from == from
                && *op_to == to
                && *lanes == encoding.lanes()
                && *dst_width == encoding.destination_width
        }
        (
            X86VexPackedConvertMemoryKind::IntToFp { fp_elem },
            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask: None,
                int_elem: VecElementType::I32,
                fp_elem: op_fp_elem,
                signed: true,
                lanes,
                src_width,
                dst_width,
                mask_zeroing: false,
                zero_upper: true,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
            },
        ) => {
            *dst == destination
                && *src == loaded
                && *op_fp_elem == fp_elem
                && *lanes == encoding.lanes()
                && *src_width == encoding.source_width
                && *dst_width == encoding.destination_width
        }
        (
            X86VexPackedConvertMemoryKind::FpToInt { fp_elem, truncate },
            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask: None,
                fp_elem: op_fp_elem,
                int_elem: VecElementType::I32,
                signed: true,
                truncate: op_truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing: false,
                zero_upper: true,
                round,
                suppress_exceptions: false,
            },
        ) => {
            let expected_round = if truncate {
                FpRoundMode::RoundTowardZero
            } else {
                FpRoundMode::Dynamic
            };
            *dst == destination
                && *src == loaded
                && *op_fp_elem == fp_elem
                && *op_truncate == truncate
                && *lanes == encoding.lanes()
                && *src_width == encoding.source_width
                && *dst_width == encoding.destination_width
                && *round == expected_round
        }
        _ => false,
    };
    semantics_match.then_some(X86JitVexPackedConvertMemorySequence {
        consumed: 2,
        encoding,
    })
}
