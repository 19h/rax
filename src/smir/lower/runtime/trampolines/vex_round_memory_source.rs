//! Fail-closed helper-backed VEX floating-point round memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexRoundMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed for one helper-backed VEX
/// `VROUNDPS`, `VROUNDPD`, `VROUNDSS`, or `VROUNDSD` memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexRoundMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexRoundMemoryEncoding,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated VEX floating-point round width"),
    }))
}

/// Validate the complete two-op load/`X86Round` decomposition for one VEX
/// floating-point round memory source.
///
/// Exact source-byte provenance binds the opcode, destination, scalar merge
/// source, vector length (including scalar LIG), WIG value, imm8 rounding
/// controls, and memory width. The loaded virtual must be defined and consumed
/// exactly once; operation boundaries, hints, and address shape fail closed.
///
/// Classification is O(1). Callers build definition/use maps once in O(N)
/// time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_round_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexRoundMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == load.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_round_memory_encoding()?;

    let (loaded, address_valid) = match (&load.kind, encoding.merge) {
        (OpKind::VLoad { dst, addr, width }, None)
            if *width == encoding.width
                && matches!(
                    load.x86_hint,
                    Some(X86OpHint::VecAlign(
                        X86VecAlign::Unaligned | X86VecAlign::Aligned
                    ))
                ) =>
        {
            (*dst, x86_jit_mem_address_shape_valid(addr))
        }
        (
            OpKind::Load {
                dst,
                addr,
                width,
                sign: SignExtend::Zero,
            },
            Some(_),
        ) if *width
            == match encoding.elem {
                VecElementType::F32 => MemWidth::B4,
                VecElementType::F64 => MemWidth::B8,
                _ => return None,
            }
            && load.x86_hint.is_none() =>
        {
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

    let round = block.ops.get(index + 1)?;
    if round.guest_pc != load.guest_pc
        || round.x86_hint.is_some()
        || block
            .ops
            .get(index + 2)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }
    let OpKind::X86Round {
        dst,
        merge,
        src,
        elem,
        width,
        lanes,
        scalar_source,
        zero_upper,
        mode,
        suppress_precision,
    } = round.kind
    else {
        return None;
    };
    let expected_merge = encoding.merge.map_or_else(
        || vector_reg(encoding.destination, encoding.width),
        |index| vector_reg(index, VecWidth::V128),
    );
    let expected_lanes = if encoding.merge.is_some() {
        1
    } else {
        encoding.width.lanes(encoding.elem) as u8
    };
    if dst != vector_reg(encoding.destination, encoding.width)
        || merge != expected_merge
        || src != loaded
        || elem != encoding.elem
        || width != encoding.width
        || lanes != expected_lanes
        || scalar_source != encoding.merge.is_some()
        || !zero_upper
        || mode != encoding.mode()
        || suppress_precision != encoding.suppress_precision()
    {
        return None;
    }

    Some(X86JitVexRoundMemorySequence {
        consumed: 2,
        encoding,
    })
}
