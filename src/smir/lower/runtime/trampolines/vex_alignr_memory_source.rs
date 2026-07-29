//! Fail-closed helper-backed VEX `VPALIGNR` memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous VEX `VPALIGNR` memory-source decomposition consumed by the
/// helper-backed x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexAlignrMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) width: VecWidth,
    pub(crate) immediate: u8,
    pub(crate) w: bool,
}

fn low_vex_vector_index(reg: VReg, width: VecWidth) -> Option<u8> {
    match (reg, width) {
        (VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))), VecWidth::V128)
        | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(index @ 0..=15))), VecWidth::V256) => Some(index),
        _ => None,
    }
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

/// Validate the complete 36- or 68-op decomposition emitted for one AVX/AVX2
/// VEX `VPALIGNR` memory source. Source-byte provenance binds the destination,
/// first source, vector width, WIG encoding, and imm8 to the exact per-128-bit
/// selector graph. Every virtual defined inside the sequence must have all of
/// its definitions and uses inside it.
///
/// The instruction-defined maximum of 32 byte lanes bounds classification to
/// O(1) time and O(1) auxiliary space. Callers build global definition/use
/// maps once in O(N) time and O(V) space for N operations and V virtual
/// registers.
pub(crate) fn x86_jit_vex_alignr_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexAlignrMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let (destination, source1, encoded_width, immediate, w) =
        instruction.vex_memory_alignr_fields()?;
    if encoded_width != width {
        return None;
    }
    let lanes = width.lanes(VecElementType::I8) as u8;
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
        _ => unreachable!("validated VEX VPALIGNR width"),
    }));
    let source1_reg = VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(source1),
        VecWidth::V256 => X86Reg::Ymm(source1),
        _ => unreachable!("validated VEX VPALIGNR width"),
    }));
    if low_vex_vector_index(destination_reg, width) != Some(destination)
        || low_vex_vector_index(source1_reg, width) != Some(source1)
    {
        return None;
    }

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
        elem: VecElementType::I8,
        lanes: broadcast_lanes,
    } = block.ops[index + 2].kind
    else {
        return None;
    };
    let indices = unique_virtual(indices, &mut seen)?;
    if scalar != zero || broadcast_lanes != lanes {
        return None;
    }

    for lane in 0..lanes {
        let block_base = lane / 16 * 16;
        let in_block = lane % 16;
        let concatenated = u16::from(immediate) + u16::from(in_block);
        let selector = if concatenated < 16 {
            u16::from(block_base) + concatenated
        } else if concatenated < 32 {
            u16::from(lanes) + u16::from(block_base) + concatenated - 16
        } else {
            u16::from(lanes) * 2
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
            elem: VecElementType::I8,
        } = block.ops[mov_offset + 1].kind
        else {
            return None;
        };
        if dst != indices || vec != indices || scalar != selector_reg || inserted_lane != lane {
            return None;
        }
    }

    let OpKind::VShuffle {
        dst,
        src1: shuffled_low,
        src2: Some(shuffled_high),
        indices: shuffled_indices,
        elem: VecElementType::I8,
        lanes: shuffled_lanes,
    } = block.ops[index + consumed - 1].kind
    else {
        return None;
    };
    if dst != destination_reg
        || shuffled_low != loaded
        || shuffled_high != source1_reg
        || shuffled_indices != indices
        || shuffled_lanes != lanes
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexAlignrMemorySequence {
        consumed,
        memory_size: width.bytes(),
        destination,
        source1,
        width,
        immediate,
        w,
    })
}
