//! Fail-closed helper-backed VEX immediate-permute memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SrcOperand, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexImmediatePermuteMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed for one helper-backed VEX
/// immediate-permute memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexImmediatePermuteMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) load_offset: usize,
    pub(crate) encoding: X86VexImmediatePermuteMemoryEncoding,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated VEX immediate-permute width"),
    }))
}

fn unique_virtual(register: VReg, seen: &mut HashSet<VReg>) -> Option<VReg> {
    matches!(register, VReg::Virtual(_))
        .then_some(register)
        .filter(|candidate| seen.insert(*candidate))
}

fn local_virtual_counts_match(
    ops: &[crate::smir::ir::ops::SmirOp],
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> bool {
    let mut local_definitions = HashMap::new();
    let mut local_uses = HashMap::new();
    for op in ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *local_definitions.entry(register).or_insert(0usize) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *local_uses.entry(register).or_insert(0usize) += 1;
            }
        }
    }
    let local_registers: HashSet<_> = local_definitions
        .keys()
        .chain(local_uses.keys())
        .copied()
        .collect();
    local_registers.into_iter().all(|register| {
        virtual_definitions.get(&register).copied().unwrap_or(0)
            == local_definitions.get(&register).copied().unwrap_or(0)
            && virtual_uses.get(&register).copied().unwrap_or(0)
                == local_uses.get(&register).copied().unwrap_or(0)
    })
}

/// Validate the complete 8- through 20-op canonical decomposition for a VEX
/// memory-source `VPERMILPS`, `VPERMILPD`, `VPERMQ`, or `VPERMPD`.
///
/// Source-byte provenance binds the opcode, destination, vector and element
/// widths, immediate selectors, exact memory width, and every generated index
/// edge. No locally defined virtual may escape the sequence.
///
/// The architectural maximum of eight lanes bounds classification to O(1)
/// time and O(1) auxiliary space. Callers construct definition/use maps once
/// in O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_immediate_permute_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexImmediatePermuteMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == first.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, first.guest_pc))?;
    let encoding = instruction.vex_immediate_permute_memory_encoding()?;
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let expected_consumed = 4 + usize::from(lanes) * 2;
    let sequence = block
        .ops
        .get(index..index.checked_add(expected_consumed)?)?;
    if sequence
        .iter()
        .any(|op| op.guest_pc != first.guest_pc || op.x86_hint.is_some())
        || block
            .ops
            .get(index + expected_consumed)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }

    let mut seen = HashSet::new();
    let mut cursor = 0usize;
    let OpKind::Mov {
        dst: zero,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let zero = unique_virtual(zero, &mut seen)?;
    cursor += 1;

    let OpKind::VBroadcast {
        dst: indices,
        scalar,
        elem,
        lanes: broadcast_lanes,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let indices = unique_virtual(indices, &mut seen)?;
    if scalar != zero || elem != encoding.elem || broadcast_lanes != lanes {
        return None;
    }
    cursor += 1;

    for lane in 0..lanes {
        let OpKind::Mov {
            dst: selector,
            src: SrcOperand::Imm(selector_value),
            width: OpWidth::W64,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        let selector = unique_virtual(selector, &mut seen)?;
        if selector_value != i64::from(encoding.source_lane(lane)) {
            return None;
        }
        cursor += 1;

        let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane: inserted_lane,
            elem,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        if dst != indices
            || vec != indices
            || scalar != selector
            || inserted_lane != lane
            || elem != encoding.elem
        {
            return None;
        }
        cursor += 1;
    }

    let load_offset = cursor;
    let OpKind::VLoad {
        dst: loaded,
        ref addr,
        width,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let loaded = unique_virtual(loaded, &mut seen)?;
    if width != encoding.width || !x86_jit_mem_address_shape_valid(addr) {
        return None;
    }
    cursor += 1;

    let OpKind::VPermute {
        dst,
        src1,
        src2: None,
        indices: actual_indices,
        elem,
        width,
        overwrite_table: false,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    if dst != vector_reg(encoding.destination, encoding.width)
        || src1 != loaded
        || actual_indices != indices
        || elem != encoding.elem
        || width != encoding.width
    {
        return None;
    }
    cursor += 1;

    if cursor != expected_consumed
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }
    Some(X86JitVexImmediatePermuteMemorySequence {
        consumed: expected_consumed,
        load_offset,
        encoding,
    })
}
