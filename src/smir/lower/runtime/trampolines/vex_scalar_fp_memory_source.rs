//! Fail-closed helper-backed VEX `VMOVSS`/`VMOVSD` memory admission.

use std::collections::HashMap;

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, SignExtend, VReg, VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{
    X86InstructionBytes, X86VexScalarFpMemoryEncoding, X86VexScalarFpMemoryKind,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact canonical decomposition consumed for one VEX scalar floating move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexScalarFpMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexScalarFpMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn is_single_definition_single_use(
    register: VReg,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    matches!(register, VReg::Virtual(_))
        && virtual_definitions.get(&register) == Some(&1)
        && virtual_uses.get(&register) == Some(&1)
}

fn element(width: MemWidth) -> Option<VecElementType> {
    match width {
        MemWidth::B4 => Some(VecElementType::F32),
        MemWidth::B8 => Some(VecElementType::F64),
        _ => None,
    }
}

fn has_exact_hint(
    op: &SmirOp,
    encoding: X86VexScalarFpMemoryEncoding,
    source_width_256: bool,
) -> bool {
    let expected_pp = match encoding.pp {
        2 => X86SsePrefix::Rep,
        3 => X86SsePrefix::Repne,
        _ => return false,
    };
    let expected_width = if source_width_256 {
        VecWidth::V256
    } else {
        VecWidth::V128
    };
    matches!(
        op.x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp,
            opcode,
            width,
            w,
        }) if pp == expected_pp
            && opcode == encoding.opcode
            && width == expected_width
            && w == encoding.w
    )
}

/// Validate the exact canonical decomposition for a VEX memory `VMOVSS` or
/// `VMOVSD`.
///
/// Loads are the canonical two-op `Load; VBroadcast` graph that zeroes every
/// destination bit above the scalar. Stores are the canonical two-op
/// `VExtractLane; Store` graph. Complete source-byte provenance and the exact
/// terminal VEX hint bind pp/opcode/W/L, vector operand, direction, and
/// transfer width; canonical IR supplies an accepted architectural address
/// shape. The sole intermediate must have one global definition and one global
/// use.
///
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space.
pub(crate) fn x86_jit_vex_scalar_fp_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexScalarFpMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == first.guest_pc {
        return None;
    }
    let source_instruction = instruction_bytes.get(&(block.id, first.guest_pc))?;
    let instruction = source_instruction
        .vex_scalar_l1_canonical_l0()
        .unwrap_or(*source_instruction);
    let encoding = instruction.vex_scalar_fp_memory_encoding()?;
    let source_width_256 = instruction != *source_instruction || encoding.width_256;
    let vector = xmm(encoding.vector);
    let elem = element(encoding.memory_width)?;

    let intermediate = match encoding.kind {
        X86VexScalarFpMemoryKind::Load => {
            let loaded = match &first.kind {
                OpKind::Load {
                    dst,
                    addr,
                    width,
                    sign: SignExtend::Zero,
                } if *width == encoding.memory_width
                    && first.x86_hint.is_none()
                    && x86_jit_mem_address_shape_valid(addr) =>
                {
                    *dst
                }
                _ => return None,
            };
            let broadcast = block.ops.get(index + 1)?;
            if !matches!(
                &broadcast.kind,
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem: actual_elem,
                    lanes: 1,
                } if *dst == vector && *scalar == loaded && *actual_elem == elem
            ) || !has_exact_hint(broadcast, encoding, source_width_256)
            {
                return None;
            }
            loaded
        }
        X86VexScalarFpMemoryKind::Store => {
            let extracted = match &first.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane: 0,
                    elem: actual_elem,
                    sign: SignExtend::Zero,
                } if *vec == vector && *actual_elem == elem && first.x86_hint.is_none() => *dst,
                _ => return None,
            };
            let store = block.ops.get(index + 1)?;
            if !matches!(
                &store.kind,
                OpKind::Store {
                    src,
                    addr,
                    width,
                } if *src == extracted
                    && *width == encoding.memory_width
                    && x86_jit_mem_address_shape_valid(addr)
            ) || !has_exact_hint(store, encoding, source_width_256)
            {
                return None;
            }
            extracted
        }
    };

    let consumed = 2;
    let end = index + consumed;
    if block.ops[index..end]
        .iter()
        .any(|op| op.guest_pc != first.guest_pc)
        || block
            .ops
            .get(end)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
        || !is_single_definition_single_use(intermediate, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexScalarFpMemorySequence { consumed, encoding })
}
