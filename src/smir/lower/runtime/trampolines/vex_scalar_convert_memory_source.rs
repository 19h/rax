//! Fail-closed helper-backed VEX scalar-conversion memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86InstructionBytes, X86VexScalarConvertMemoryEncoding, X86VexScalarConvertMemoryKind,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact two-op decomposition consumed for one deterministic VEX.L=0 scalar
/// conversion memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexScalarConvertMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexScalarConvertMemoryEncoding,
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn xmm(index: u8) -> VReg {
    x86(X86Reg::Xmm(index))
}

/// Validate an exact load/conversion pair for the eight deterministic
/// VEX.L=0 scalar conversion memory families.
///
/// Complete source-byte provenance binds the opcode, W-selected integer
/// width, F3/F2 floating format, destination, merge source, and forbidden
/// generation-dependent VEX.L=1 state. The loaded virtual must have exactly
/// one definition and one use. Classification is O(1); callers construct the
/// definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_scalar_convert_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexScalarConvertMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == load.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_scalar_convert_memory_encoding()?;

    let expected_mem_width = match encoding.memory_size {
        4 => MemWidth::B4,
        8 => MemWidth::B8,
        _ => return None,
    };
    let expected_sign = if matches!(encoding.kind, X86VexScalarConvertMemoryKind::IntToFp { .. }) {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let (loaded, address_valid) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width,
            sign,
        } if *width == expected_mem_width && *sign == expected_sign && load.x86_hint.is_none() => {
            (*dst, x86_jit_mem_address_shape_valid(addr))
        }
        _ => return None,
    };
    if !address_valid
        || !matches!(loaded, VReg::Virtual(_))
        || virtual_definitions.get(&loaded) != Some(&1)
        || virtual_uses.get(&loaded) != Some(&1)
    {
        return None;
    }

    let conversion = block.ops.get(index + 1)?;
    let prefix = if encoding.pp == 2 {
        X86SsePrefix::Rep
    } else {
        X86SsePrefix::Repne
    };
    if conversion.guest_pc != load.guest_pc
        || conversion.x86_hint
            != Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: prefix,
                opcode: encoding.opcode,
                width: VecWidth::V128,
                w: encoding.w,
            })
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let semantics_match = match (encoding.kind, &conversion.kind) {
        (
            X86VexScalarConvertMemoryKind::FpConvert { from, to },
            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask: None,
                from: op_from,
                to: op_to,
                mask_zeroing: false,
                round: crate::smir::ir::types::FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: true,
            },
        ) => {
            *dst == xmm(encoding.destination)
                && *merge == xmm(encoding.merge?)
                && *src == loaded
                && *op_from == from
                && *op_to == to
        }
        (
            X86VexScalarConvertMemoryKind::IntToFp { elem, int_width },
            OpKind::X86IntToFp {
                dst,
                merge,
                src,
                elem: op_elem,
                int_width: op_int_width,
                signed: true,
                round: crate::smir::ir::types::FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: true,
            },
        ) => {
            *dst == xmm(encoding.destination)
                && *merge == xmm(encoding.merge?)
                && *src == loaded
                && *op_elem == elem
                && *op_int_width == int_width
        }
        (
            X86VexScalarConvertMemoryKind::FpToInt {
                elem,
                int_width,
                truncate,
            },
            OpKind::X86FpToInt {
                dst,
                src,
                elem: op_elem,
                int_width: op_int_width,
                signed: true,
                truncate: op_truncate,
                round,
                suppress_exceptions: false,
            },
        ) => {
            *dst == x86(X86Reg::gpr(encoding.destination))
                && *src == loaded
                && *op_elem == elem
                && *op_int_width == int_width
                && *op_truncate == truncate
                && Some(*round) == encoding.fp_to_int_round()
        }
        _ => false,
    };
    if !semantics_match {
        return None;
    }

    Some(X86JitVexScalarConvertMemorySequence {
        consumed: 2,
        encoding,
    })
}
