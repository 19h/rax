//! Fail-closed helper-backed VEX immediate-blend memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexImmediateBlendMemoryFields};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed for one helper-backed VEX
/// immediate-blend memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexImmediateBlendMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) memory_size: u32,
    pub(crate) encoding: X86VexImmediateBlendMemoryFields,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated VEX immediate-blend width"),
    }))
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

/// Validate the complete 8- through 36-op decomposition emitted for one
/// `VPBLENDD`, `VBLENDPS`, `VBLENDPD`, or `VPBLENDW` memory source.
///
/// Source-lane provenance binds the destination, first source, element and
/// vector widths, imm8, W/WIG encoding, and repeated 128-bit `VPBLENDW` mask to
/// the exact extraction/insertion graph. Every virtual defined inside the
/// sequence must have all of its definitions and uses inside it.
///
/// The architectural maximum of 16 word lanes bounds classification to O(1)
/// time and O(1) auxiliary space. Callers build global definition/use maps once
/// in O(N) time and O(V) space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_immediate_blend_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexImmediateBlendMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(
                load.x86_hint,
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            ) && matches!(dst, VReg::Virtual(_))
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };

    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_memory_immediate_blend_fields()?;
    if encoding.width != width {
        return None;
    }
    let lanes = width.lanes(encoding.element) as u8;
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

    let destination = vector_reg(encoding.destination, width);
    let source1 = vector_reg(encoding.source1, width);
    let mut seen = HashSet::new();
    unique_virtual(loaded, &mut seen)?;
    let mut selected = Vec::with_capacity(usize::from(lanes));
    let block_lanes = (16 / encoding.element.bytes()) as u8;
    for lane in 0..lanes {
        let OpKind::VExtractLane {
            dst,
            vec,
            lane: extracted_lane,
            elem,
            sign: SignExtend::Zero,
        } = block.ops[index + 1 + usize::from(lane)].kind
        else {
            return None;
        };
        let dst = unique_virtual(dst, &mut seen)?;
        let bit = if encoding.repeat_128 {
            lane % block_lanes
        } else {
            lane
        };
        let expected_source = if (encoding.immediate >> bit) & 1 != 0 {
            loaded
        } else {
            source1
        };
        if vec != expected_source || extracted_lane != lane || elem != encoding.element {
            return None;
        }
        selected.push(dst);
    }

    let zero_index = index + 1 + usize::from(lanes);
    let OpKind::Mov {
        dst: zero,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = block.ops[zero_index].kind
    else {
        return None;
    };
    let zero = unique_virtual(zero, &mut seen)?;
    let OpKind::VBroadcast {
        dst: output,
        scalar,
        elem,
        lanes: broadcast_lanes,
    } = block.ops[zero_index + 1].kind
    else {
        return None;
    };
    let output = unique_virtual(output, &mut seen)?;
    if scalar != zero || elem != encoding.element || broadcast_lanes != lanes {
        return None;
    }

    let insert_start = zero_index + 2;
    for (lane, scalar) in selected.into_iter().enumerate() {
        let OpKind::VInsertLane {
            dst,
            vec,
            scalar: inserted_scalar,
            lane: inserted_lane,
            elem,
        } = block.ops[insert_start + lane].kind
        else {
            return None;
        };
        if dst != output
            || vec != output
            || inserted_scalar != scalar
            || usize::from(inserted_lane) != lane
            || elem != encoding.element
        {
            return None;
        }
    }

    let OpKind::VMov {
        dst,
        src,
        width: move_width,
    } = block.ops[index + consumed - 1].kind
    else {
        return None;
    };
    if dst != destination
        || src != output
        || move_width != width
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }

    Some(X86JitVexImmediateBlendMemorySequence {
        consumed,
        memory_size: width.bytes(),
        encoding,
    })
}
