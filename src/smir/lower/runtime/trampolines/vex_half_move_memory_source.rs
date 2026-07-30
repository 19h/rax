//! Fail-closed helper-backed VEX high/low 64-bit lane load admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexHalfMoveMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact six-op decomposition consumed for one VEX.128 64-bit high/low lane
/// memory load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexHalfMoveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexHalfMoveMemoryEncoding,
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

/// Validate the exact six-op canonical decomposition for a VEX.128
/// `VMOVLPS`, `VMOVLPD`, `VMOVHPS`, or `VMOVHPD` memory source.
///
/// Complete source-byte provenance binds map, mandatory prefix, WIG, L=0,
/// opcode, destination, merge source, and the 8-byte access width; canonical
/// IR supplies an accepted architectural address shape. The three locally
/// defined virtuals must each have one global definition and one global use,
/// preventing any elided value from escaping the fused sequence.
/// Classification is O(1); callers build definition/use maps once in O(N)
/// time and O(V) space.
pub(crate) fn x86_jit_vex_half_move_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexHalfMoveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == first.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, first.guest_pc))?;
    let encoding = instruction.vex_half_move_memory_encoding()?;
    let destination = xmm(encoding.destination);
    let merge = xmm(encoding.source1);
    let preserved_lane = 1 - encoding.memory_lane;

    let preserved = match &first.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } if *vec == merge && *lane == preserved_lane && first.x86_hint.is_none() => *dst,
        _ => return None,
    };

    let load = block.ops.get(index + 1)?;
    let loaded = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => *dst,
        _ => return None,
    };

    let zero_op = block.ops.get(index + 2)?;
    let zero = match &zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => *dst,
        _ => return None,
    };

    let clear = block.ops.get(index + 3)?;
    if !matches!(
        &clear.kind,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I64,
            lanes: 1,
        } if *dst == destination && *scalar == zero
    ) || clear.x86_hint.is_some()
    {
        return None;
    }

    let insert_preserved = block.ops.get(index + 4)?;
    if !matches!(
        &insert_preserved.kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem: VecElementType::I64,
        } if *dst == destination
            && *vec == destination
            && *scalar == preserved
            && *lane == preserved_lane
    ) || insert_preserved.x86_hint.is_some()
    {
        return None;
    }

    let insert_memory = block.ops.get(index + 5)?;
    if !matches!(
        &insert_memory.kind,
        OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane,
            elem: VecElementType::I64,
        } if *dst == destination
            && *vec == destination
            && *scalar == loaded
            && *lane == encoding.memory_lane
    ) || insert_memory.x86_hint.is_some()
    {
        return None;
    }

    let end = index + 6;
    if block.ops[index..end]
        .iter()
        .any(|op| op.guest_pc != first.guest_pc)
        || block
            .ops
            .get(end)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
        || preserved == loaded
        || preserved == zero
        || loaded == zero
        || ![preserved, loaded, zero].into_iter().all(|register| {
            is_single_definition_single_use(register, virtual_definitions, virtual_uses)
        })
    {
        return None;
    }

    Some(X86JitVexHalfMoveMemorySequence {
        consumed: 6,
        encoding,
    })
}
