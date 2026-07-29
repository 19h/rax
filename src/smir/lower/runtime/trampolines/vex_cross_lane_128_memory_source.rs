//! Fail-closed helper-backed VEX 128-bit cross-lane memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexCrossLane128MemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed for one helper-backed VEX 128-bit
/// cross-lane memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexCrossLane128MemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexCrossLane128MemoryEncoding,
}

fn ymm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)))
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
    local_definitions
        .iter()
        .all(|(register, count)| virtual_definitions.get(register) == Some(count))
        && local_uses
            .iter()
            .all(|(register, count)| virtual_uses.get(register) == Some(count))
}

fn match_insert_graph(
    sequence: &[crate::smir::ir::ops::SmirOp],
    encoding: X86VexCrossLane128MemoryEncoding,
    loaded: VReg,
    seen: &mut HashSet<VReg>,
) -> bool {
    if sequence.len() != 7 {
        return false;
    }
    let raw = match sequence[1].kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width: VecWidth::V256,
        } if src1 == ymm(encoding.source1) && src2 == ymm(encoding.source1) => {
            let Some(raw) = unique_virtual(dst, seen) else {
                return false;
            };
            raw
        }
        _ => return false,
    };
    let first_lane = (encoding.immediate & 1) * 2;
    for lane in 0..2u8 {
        let extract_index = 2 + usize::from(lane) * 2;
        let scalar = match sequence[extract_index].kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extracted_lane,
                elem: VecElementType::I64,
                sign: SignExtend::Zero,
            } if vec == loaded && extracted_lane == lane => {
                let Some(scalar) = unique_virtual(dst, seen) else {
                    return false;
                };
                scalar
            }
            _ => return false,
        };
        if !matches!(
            sequence[extract_index + 1].kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted_scalar,
                lane: inserted_lane,
                elem: VecElementType::I64,
            } if dst == raw
                && vec == raw
                && inserted_scalar == scalar
                && inserted_lane == first_lane + lane
        ) {
            return false;
        }
    }
    matches!(
        sequence[6].kind,
        OpKind::VMov {
            dst,
            src,
            width: VecWidth::V256,
        } if dst == ymm(encoding.destination) && src == raw
    )
}

fn match_permute_graph(
    sequence: &[crate::smir::ir::ops::SmirOp],
    encoding: X86VexCrossLane128MemoryEncoding,
    loaded: VReg,
    seen: &mut HashSet<VReg>,
) -> bool {
    let mut cursor = 1usize;
    let mut selected = Vec::with_capacity(4);
    for (output_half, control_shift, zero_bit) in [(0u8, 0u8, 3u8), (1, 4, 7)] {
        if encoding.immediate >> zero_bit & 1 != 0 {
            continue;
        }
        let control = encoding.immediate >> control_shift & 3;
        let source = if control < 2 {
            ymm(encoding.source1)
        } else {
            loaded
        };
        let source_half = control & 1;
        for lane_in_half in 0..2u8 {
            let Some(op) = sequence.get(cursor) else {
                return false;
            };
            let scalar = match op.kind {
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                } if vec == source && lane == source_half * 2 + lane_in_half => {
                    let Some(scalar) = unique_virtual(dst, seen) else {
                        return false;
                    };
                    scalar
                }
                _ => return false,
            };
            selected.push((output_half * 2 + lane_in_half, scalar));
            cursor += 1;
        }
    }

    let Some(op) = sequence.get(cursor) else {
        return false;
    };
    let zero = match op.kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => {
            let Some(zero) = unique_virtual(dst, seen) else {
                return false;
            };
            zero
        }
        _ => return false,
    };
    cursor += 1;
    let Some(op) = sequence.get(cursor) else {
        return false;
    };
    let output = match op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I64,
            lanes: 4,
        } if scalar == zero => {
            let Some(output) = unique_virtual(dst, seen) else {
                return false;
            };
            output
        }
        _ => return false,
    };
    cursor += 1;
    for (lane, scalar) in selected {
        let Some(op) = sequence.get(cursor) else {
            return false;
        };
        if !matches!(
            op.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: inserted_scalar,
                lane: inserted_lane,
                elem: VecElementType::I64,
            } if dst == output
                && vec == output
                && inserted_scalar == scalar
                && inserted_lane == lane
        ) {
            return false;
        }
        cursor += 1;
    }
    let Some(op) = sequence.get(cursor) else {
        return false;
    };
    if !matches!(
        op.kind,
        OpKind::VMov {
            dst,
            src,
            width: VecWidth::V256,
        } if dst == ymm(encoding.destination) && src == output
    ) {
        return false;
    }
    cursor + 1 == sequence.len()
}

/// Validate the complete decomposition emitted for one `VPERM2F128`,
/// `VINSERTF128`, `VINSERTI128`, or `VPERM2I128` memory source.
///
/// Source-byte provenance binds the opcode, destination, first source,
/// immediate, source width, and memory width to the graph. Every virtual value
/// is contained within the sequence. Runtime is O(1), bounded by 12 operations;
/// callers construct definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_cross_lane_128_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexCrossLane128MemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if matches!(
                load.x86_hint,
                Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
            ) && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_cross_lane_128_memory_encoding()?;
    if encoding.source_width != width {
        return None;
    }
    let selected_lanes = if encoding.is_insert() {
        0
    } else {
        2 * usize::from(encoding.immediate & 0x08 == 0)
            + 2 * usize::from(encoding.immediate & 0x80 == 0)
    };
    let consumed = if encoding.is_insert() {
        7
    } else {
        4 + selected_lanes * 2
    };
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

    let mut seen = HashSet::new();
    let loaded = unique_virtual(loaded, &mut seen)?;
    let graph_matches = if encoding.is_insert() {
        match_insert_graph(sequence, encoding, loaded, &mut seen)
    } else {
        match_permute_graph(sequence, encoding, loaded, &mut seen)
    };
    if !graph_matches || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses) {
        return None;
    }

    Some(X86JitVexCrossLane128MemorySequence { consumed, encoding })
}
