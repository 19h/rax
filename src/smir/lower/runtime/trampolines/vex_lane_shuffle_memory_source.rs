//! Fail-closed helper-backed VEX packed lane-shuffle memory admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous `VPSHUFD`/`VPSHUFHW`/`VPSHUFLW` memory-source
/// decomposition consumed by the helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexLaneShuffleMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) high_words: Option<bool>,
    pub(crate) immediate: u8,
    pub(crate) w: bool,
}

fn unique_virtual(reg: VReg, seen: &mut HashSet<VReg>) -> Option<VReg> {
    matches!(reg, VReg::Virtual(_))
        .then_some(reg)
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
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *local_definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *local_uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }
    local_definitions
        .iter()
        .all(|(reg, count)| virtual_definitions.get(reg) == Some(count))
        && local_uses
            .iter()
            .all(|(reg, count)| virtual_uses.get(reg) == Some(count))
}

/// Validate the complete 12-, 20-, or 36-op decomposition emitted for one
/// VEX `VPSHUFD`, `VPSHUFHW`, or `VPSHUFLW` memory source.
///
/// Source-byte provenance binds destination, vector width, shuffle kind, WIG
/// encoding, reserved VEX.vvvv, and imm8 to the exact generated index graph.
/// Every virtual defined inside the sequence must have all definitions and
/// uses inside it, and no same-PC tail may remain unconsumed.
///
/// The architectural maximum of 16 lanes bounds classification to O(1) time
/// and O(1) auxiliary space. Callers build global definition/use maps once in
/// O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_lane_shuffle_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexLaneShuffleMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint == Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (destination, encoded_width, elem, high_words, immediate, w) =
        instruction.vex_memory_lane_shuffle_fields()?;
    if encoded_width != width {
        return None;
    }

    let lanes = width.lanes(elem) as u8;
    let consumed = 4 + usize::from(lanes) * 2;
    let sequence = block.ops.get(index..index.checked_add(consumed)?)?;
    if sequence
        .iter()
        .skip(1)
        .any(|op| op.guest_pc != load.guest_pc || op.x86_hint.is_some())
        || block
            .ops
            .get(index + consumed)
            .is_some_and(|op| op.guest_pc == load.guest_pc)
    {
        return None;
    }

    let destination_reg = VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(destination),
        VecWidth::V256 => X86Reg::Ymm(destination),
        _ => unreachable!("validated VEX lane-shuffle width"),
    }));
    let mut seen = HashSet::new();
    unique_virtual(loaded, &mut seen)?;
    let OpKind::Mov {
        dst: zero,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = block.ops[index + 1].kind
    else {
        return None;
    };
    let zero = unique_virtual(zero, &mut seen)?;
    let OpKind::VBroadcast {
        dst: indices,
        scalar,
        elem: broadcast_elem,
        lanes: broadcast_lanes,
    } = block.ops[index + 2].kind
    else {
        return None;
    };
    let indices = unique_virtual(indices, &mut seen)?;
    if scalar != zero || broadcast_elem != elem || broadcast_lanes != lanes {
        return None;
    }

    let block_lanes = if elem == VecElementType::I32 { 4 } else { 8 };
    for lane in 0..lanes {
        let within = lane % block_lanes;
        let lane_block = lane - within;
        let shuffled = match high_words {
            None => true,
            Some(true) => within >= 4,
            Some(false) => within < 4,
        };
        let selector = if shuffled {
            let output = within % 4;
            lane_block
                + if high_words == Some(true) { 4 } else { 0 }
                + ((immediate >> (output * 2)) & 3)
        } else {
            lane
        };
        let mov_offset = index + 3 + usize::from(lane) * 2;
        let OpKind::Mov {
            dst: selector_reg,
            src: SrcOperand::Imm(encoded_selector),
            width: OpWidth::W64,
        } = block.ops[mov_offset].kind
        else {
            return None;
        };
        let selector_reg = unique_virtual(selector_reg, &mut seen)?;
        if encoded_selector != i64::from(selector) {
            return None;
        }
        let OpKind::VInsertLane {
            dst,
            vec,
            scalar,
            lane: inserted_lane,
            elem: inserted_elem,
        } = block.ops[mov_offset + 1].kind
        else {
            return None;
        };
        if dst != indices
            || vec != indices
            || scalar != selector_reg
            || inserted_lane != lane
            || inserted_elem != elem
        {
            return None;
        }
    }

    let OpKind::VShuffle {
        dst,
        src1,
        src2: None,
        indices: shuffled_indices,
        elem: shuffled_elem,
        lanes: shuffled_lanes,
    } = block.ops[index + consumed - 1].kind
    else {
        return None;
    };
    if dst != destination_reg
        || src1 != loaded
        || shuffled_indices != indices
        || shuffled_elem != elem
        || shuffled_lanes != lanes
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexLaneShuffleMemorySequence {
        consumed,
        memory_size: width.bytes(),
        destination,
        width,
        elem,
        high_words,
        immediate,
        w,
    })
}
