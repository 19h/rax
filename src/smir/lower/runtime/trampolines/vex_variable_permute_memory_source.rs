//! Fail-closed helper-backed VEX variable-permute memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    ArchReg, BlockId, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth,
    X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexVariablePermuteMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed for one helper-backed VEX
/// variable-permute memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexVariablePermuteMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexVariablePermuteMemoryEncoding,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated VEX variable-permute width"),
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
    local_definitions
        .iter()
        .all(|(register, count)| virtual_definitions.get(register) == Some(count))
        && local_uses
            .iter()
            .all(|(register, count)| virtual_uses.get(register) == Some(count))
}

fn match_full_width_graph(
    sequence: &[crate::smir::ir::ops::SmirOp],
    encoding: X86VexVariablePermuteMemoryEncoding,
    loaded: VReg,
) -> bool {
    sequence.len() == 2
        && matches!(
            sequence[1].kind,
            OpKind::VPermute {
                dst,
                src1,
                src2: None,
                indices,
                elem,
                width,
                overwrite_table: false,
            } if dst == vector_reg(encoding.destination, encoding.width)
                && src1 == loaded
                && indices == vector_reg(encoding.source1, encoding.width)
                && elem == encoding.elem
                && width == encoding.width
        )
}

fn match_permil_graph(
    sequence: &[crate::smir::ir::ops::SmirOp],
    encoding: X86VexVariablePermuteMemoryEncoding,
    loaded: VReg,
    seen: &mut HashSet<VReg>,
) -> bool {
    let lanes = encoding.width.lanes(encoding.elem) as u8;
    let (domain_lanes, control_shift) = match encoding.elem {
        VecElementType::F32 => (4u8, 0u8),
        VecElementType::F64 => (2u8, 1u8),
        _ => return false,
    };
    let mut cursor = 1usize;
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
    let indices = match op.kind {
        OpKind::VBroadcast {
            dst,
            scalar,
            elem,
            lanes: broadcast_lanes,
        } if scalar == zero && elem == encoding.elem && broadcast_lanes == lanes => {
            let Some(indices) = unique_virtual(dst, seen) else {
                return false;
            };
            indices
        }
        _ => return false,
    };
    cursor += 1;

    for lane in 0..lanes {
        let Some(op) = sequence.get(cursor) else {
            return false;
        };
        let control = match op.kind {
            OpKind::VExtractLane {
                dst,
                vec,
                lane: extracted_lane,
                elem,
                sign: SignExtend::Zero,
            } if vec == loaded && extracted_lane == lane && elem == encoding.elem => {
                let Some(control) = unique_virtual(dst, seen) else {
                    return false;
                };
                control
            }
            _ => return false,
        };
        cursor += 1;

        let shifted = if control_shift == 0 {
            match sequence.get(cursor).map(|op| &op.kind) {
                Some(OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(src),
                    width: OpWidth::W64,
                }) if *src == control => {
                    let Some(shifted) = unique_virtual(*dst, seen) else {
                        return false;
                    };
                    cursor += 1;
                    shifted
                }
                _ => control,
            }
        } else {
            let Some(op) = sequence.get(cursor) else {
                return false;
            };
            let shifted = match op.kind {
                OpKind::Shr {
                    dst,
                    src,
                    amount: SrcOperand::Imm(amount),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                } if src == control && amount == i64::from(control_shift) => {
                    let Some(shifted) = unique_virtual(dst, seen) else {
                        return false;
                    };
                    shifted
                }
                _ => return false,
            };
            cursor += 1;
            shifted
        };

        let Some(op) = sequence.get(cursor) else {
            return false;
        };
        let selected = match op.kind {
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if src1 == shifted && mask == i64::from(domain_lanes - 1) => {
                let Some(selected) = unique_virtual(dst, seen) else {
                    return false;
                };
                selected
            }
            _ => return false,
        };
        cursor += 1;

        let Some(op) = sequence.get(cursor) else {
            return false;
        };
        let lane_base = i64::from(lane / domain_lanes * domain_lanes);
        let absolute = match op.kind {
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Imm(base),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if src1 == selected && base == lane_base => {
                let Some(absolute) = unique_virtual(dst, seen) else {
                    return false;
                };
                absolute
            }
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W64,
            } if lane_base == 0 && src == selected => {
                let Some(absolute) = unique_virtual(dst, seen) else {
                    return false;
                };
                absolute
            }
            _ => return false,
        };
        cursor += 1;

        let Some(op) = sequence.get(cursor) else {
            return false;
        };
        if !matches!(
            op.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane: inserted_lane,
                elem,
            } if dst == indices
                && vec == indices
                && scalar == absolute
                && inserted_lane == lane
                && elem == encoding.elem
        ) {
            return false;
        }
        cursor += 1;
    }

    let Some(op) = sequence.get(cursor) else {
        return false;
    };
    matches!(
        op.kind,
        OpKind::VPermute {
            dst,
            src1,
            src2: None,
            indices: actual_indices,
            elem,
            width,
            overwrite_table: false,
        } if dst == vector_reg(encoding.destination, encoding.width)
            && src1 == vector_reg(encoding.source1, encoding.width)
            && actual_indices == indices
            && elem == encoding.elem
            && width == encoding.width
    ) && cursor + 1 == sequence.len()
}

/// Validate the complete decomposition emitted for one memory-source
/// `VPERMILPS`, `VPERMILPD`, `VPERMPS`, or `VPERMD`.
///
/// Source-byte provenance binds the opcode, roles, vector width, element
/// width, and memory width to the graph. Every virtual value is contained
/// within the sequence. Runtime is O(1), bounded by 44 operations; callers
/// construct definition/use maps once in O(N) time and O(V) space.
pub(crate) fn x86_jit_vex_variable_permute_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexVariablePermuteMemorySequence> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (loaded, width) = match &load.kind {
        OpKind::VLoad { dst, addr, width }
            if load.x86_hint.is_none()
                && matches!(width, VecWidth::V128 | VecWidth::V256)
                && x86_jit_mem_address_shape_valid(addr) =>
        {
            (*dst, *width)
        }
        _ => return None,
    };
    let instruction = instruction_bytes.get(&(block.id, load.guest_pc))?;
    let encoding = instruction.vex_variable_permute_memory_encoding()?;
    if encoding.width != width {
        return None;
    }

    let lanes = encoding.width.lanes(encoding.elem) as usize;
    let lengths = if encoding.is_permil() && encoding.elem == VecElementType::F32 {
        [Some(4 + lanes * 5), Some(4 + lanes * 4)]
    } else if encoding.is_permil() {
        [Some(4 + lanes * 5), None]
    } else {
        [Some(2), None]
    };
    for consumed in lengths.into_iter().flatten() {
        let Some(sequence) = block.ops.get(index..index.checked_add(consumed)?) else {
            continue;
        };
        if sequence
            .iter()
            .skip(1)
            .any(|op| op.guest_pc != load.guest_pc || op.x86_hint.is_some())
            || block
                .ops
                .get(index + consumed)
                .is_some_and(|op| op.guest_pc == load.guest_pc)
        {
            continue;
        }

        let mut seen = HashSet::new();
        let Some(loaded) = unique_virtual(loaded, &mut seen) else {
            return None;
        };
        let graph_matches = if encoding.is_permil() {
            match_permil_graph(sequence, encoding, loaded, &mut seen)
        } else {
            match_full_width_graph(sequence, encoding, loaded)
        };
        if graph_matches && local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
        {
            return Some(X86JitVexVariablePermuteMemorySequence { consumed, encoding });
        }
    }
    None
}
