//! Fail-closed helper-backed EVEX high/low 64-bit lane load admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    X86Reg,
};
use crate::smir::ir::{X86EvexHalfMoveMemoryEncoding, X86InstructionBytes};

use super::evex_memory_source_common::{
    exact_evex_memory_apx_frontier, exact_evex_memory_sequence_frontier, no_following_same_pc,
    single_definition_single_use,
};
use super::x86_jit_mem_address_shape_valid;

/// Exact six-op decomposition consumed for one EVEX.128 64-bit high/low lane
/// memory load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitEvexHalfMoveMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) address_offset: usize,
    pub(crate) encoding: X86EvexHalfMoveMemoryEncoding,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

/// Validate the exact O0/O1/O2 six-op decomposition for one EVEX.128
/// `VMOVLPS`, `VMOVLPD`, `VMOVHPS`, or `VMOVHPD` memory source.
///
/// Complete source-byte provenance binds map, mandatory prefix, fixed W,
/// L'L=0, opcode, destination, merge source, and the unconditional 8-byte
/// Type-E9NF access. The accepted architectural address must agree exactly
/// with any APX guard, and every internal virtual has one global definition
/// and one global use. Classification is O(1); callers build definition/use
/// maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_evex_half_move_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitEvexHalfMoveMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    if !exact_evex_memory_sequence_frontier(block, index, guest_pc) {
        return None;
    }
    let encoding = instruction_bytes
        .get(&(block.id, guest_pc))?
        .evex_half_move_memory_encoding()?;
    let destination = xmm(encoding.destination);
    let merge = xmm(encoding.source1);
    let preserved_lane = 1 - encoding.memory_lane;

    let preserved = match first.kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } if first.x86_hint.is_none() && vec == merge && lane == preserved_lane => dst,
        _ => return None,
    };

    let address_offset = 1;
    let load = block.ops.get(index + address_offset)?;
    let (loaded, address) = match &load.kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } if load.x86_hint.is_none() && x86_jit_mem_address_shape_valid(addr) => (*dst, addr),
        _ => return None,
    };
    if !exact_evex_memory_apx_frontier(block, index, guest_pc, address) {
        return None;
    }

    let zero_op = block.ops.get(index + 2)?;
    let zero = match zero_op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero_op.x86_hint.is_none() => dst,
        _ => return None,
    };

    let clear = block.ops.get(index + 3)?;
    if clear.x86_hint.is_some()
        || !matches!(
            clear.kind,
            OpKind::VBroadcast {
                dst,
                scalar,
                elem: VecElementType::I64,
                lanes: 1,
            } if dst == destination && scalar == zero
        )
    {
        return None;
    }

    let insert_preserved = block.ops.get(index + 4)?;
    if insert_preserved.x86_hint.is_some()
        || !matches!(
            insert_preserved.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane,
                elem: VecElementType::I64,
            } if dst == destination
                && vec == destination
                && scalar == preserved
                && lane == preserved_lane
        )
    {
        return None;
    }

    let insert_memory = block.ops.get(index + 5)?;
    if insert_memory.x86_hint.is_some()
        || !matches!(
            insert_memory.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane,
                elem: VecElementType::I64,
            } if dst == destination
                && vec == destination
                && scalar == loaded
                && lane == encoding.memory_lane
        )
    {
        return None;
    }

    let consumed = 6;
    if block.ops[index..index + consumed]
        .iter()
        .any(|op| op.guest_pc != guest_pc)
        || !no_following_same_pc(block, index, consumed, guest_pc)
        || preserved == loaded
        || preserved == zero
        || loaded == zero
        || ![preserved, loaded, zero].into_iter().all(|register| {
            single_definition_single_use(register, virtual_definitions, virtual_uses)
        })
    {
        return None;
    }

    Some(X86JitEvexHalfMoveMemorySequence {
        consumed,
        address_offset,
        encoding,
    })
}
