//! Fail-closed helper-backed VEX packed bit-test memory-source admission.

use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    ArchReg, BlockId, Condition, GuestAddr, OpWidth, SignExtend, SrcOperand, VReg, VecElementType,
    VecWidth, X86Reg,
};
use crate::smir::ir::{X86InstructionBytes, X86VexPtestMemoryEncoding};

use super::x86_jit_mem_address_shape_valid;

/// Exact contiguous decomposition consumed for one helper-backed VEX
/// `VPTEST`, `VTESTPS`, or `VTESTPD` memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86JitVexPtestMemorySequence {
    pub(crate) consumed: usize,
    pub(crate) encoding: X86VexPtestMemoryEncoding,
}

fn vector_reg(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("validated VEX packed bit-test width"),
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

/// Validate the complete 25- through 45-op canonical decomposition for a VEX
/// memory-source `VPTEST`, `VTESTPS`, or `VTESTPD`.
///
/// Source-byte provenance binds both source operands, vector width, W policy,
/// tested-bit mask, exact memory width, both whole-vector reductions, and the
/// complete CF/ZF plus OF/SF/AF/PF flag update. No locally defined virtual may
/// escape the sequence.
///
/// Four 64-bit lanes bound classification to O(1) time and O(1) auxiliary
/// space. Callers construct definition/use maps once in O(N) time and O(V)
/// space for N operations and V virtual registers.
pub(crate) fn x86_jit_vex_ptest_memory_sequence(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<X86JitVexPtestMemorySequence> {
    if !allow_mem {
        return None;
    }
    let first = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == first.guest_pc {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, first.guest_pc))?;
    let encoding = instruction.vex_ptest_memory_encoding()?;
    let lanes = encoding.width.lanes(VecElementType::I64) as u8;
    let lane_ops = if encoding.tested_bits.is_some() { 8 } else { 6 };
    let expected_consumed = 13 + usize::from(lanes) * lane_ops;
    let sequence = block
        .ops
        .get(index..index.checked_add(expected_consumed)?)?;
    if sequence.iter().any(|op| op.guest_pc != first.guest_pc)
        || sequence.get(1..)?.iter().any(|op| op.x86_hint.is_some())
        || block
            .ops
            .get(index + expected_consumed)
            .is_some_and(|op| op.guest_pc == first.guest_pc)
    {
        return None;
    }

    let mut seen = HashSet::new();
    let mut cursor = 0usize;
    let OpKind::VLoad {
        dst: loaded,
        ref addr,
        width,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let loaded = unique_virtual(loaded, &mut seen)?;
    if width != encoding.width
        || !matches!(
            sequence[cursor].x86_hint,
            Some(X86OpHint::VecAlign(
                X86VecAlign::Unaligned | X86VecAlign::Aligned
            ))
        )
        || !x86_jit_mem_address_shape_valid(addr)
    {
        return None;
    }
    cursor += 1;

    let OpKind::Mov {
        dst: and_acc,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let and_acc = unique_virtual(and_acc, &mut seen)?;
    cursor += 1;

    let OpKind::Mov {
        dst: andnot_acc,
        src: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let andnot_acc = unique_virtual(andnot_acc, &mut seen)?;
    cursor += 1;

    for lane in 0..lanes {
        let OpKind::VExtractLane {
            dst: raw_first,
            vec: first_vector,
            lane: actual_lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        let raw_first = unique_virtual(raw_first, &mut seen)?;
        if first_vector != vector_reg(encoding.first_source, encoding.width) || actual_lane != lane
        {
            return None;
        }
        cursor += 1;

        let OpKind::VExtractLane {
            dst: raw_second,
            vec: second_vector,
            lane: actual_lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        let raw_second = unique_virtual(raw_second, &mut seen)?;
        if second_vector != loaded || actual_lane != lane {
            return None;
        }
        cursor += 1;

        let (tested_first, tested_second) = if let Some(mask) = encoding.tested_bits {
            let OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(actual_mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } = sequence.get(cursor)?.kind
            else {
                return None;
            };
            let tested_first = unique_virtual(dst, &mut seen)?;
            if src1 != raw_first || actual_mask != mask as i64 {
                return None;
            }
            cursor += 1;

            let OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Imm(actual_mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } = sequence.get(cursor)?.kind
            else {
                return None;
            };
            let tested_second = unique_virtual(dst, &mut seen)?;
            if src1 != raw_second || actual_mask != mask as i64 {
                return None;
            }
            cursor += 1;
            (tested_first, tested_second)
        } else {
            (raw_first, raw_second)
        };

        let OpKind::And {
            dst: intersection,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        let intersection = unique_virtual(intersection, &mut seen)?;
        if src1 != tested_first || src2 != tested_second {
            return None;
        }
        cursor += 1;

        let OpKind::Or {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        if dst != and_acc || src1 != and_acc || src2 != intersection {
            return None;
        }
        cursor += 1;

        let OpKind::AndNot {
            dst: outside,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        let outside = unique_virtual(outside, &mut seen)?;
        if src1 != tested_second || src2 != tested_first {
            return None;
        }
        cursor += 1;

        let OpKind::Or {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } = sequence.get(cursor)?.kind
        else {
            return None;
        };
        if dst != andnot_acc || src1 != andnot_acc || src2 != outside {
            return None;
        }
        cursor += 1;
    }

    let OpKind::ReadFlags { dst: old_flags } = sequence.get(cursor)?.kind else {
        return None;
    };
    let old_flags = unique_virtual(old_flags, &mut seen)?;
    cursor += 1;

    let OpKind::Cmp {
        src1,
        src2: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    if src1 != and_acc {
        return None;
    }
    cursor += 1;

    let OpKind::SetCC {
        dst: zf,
        cond: Condition::Eq,
        width: OpWidth::W64,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let zf = unique_virtual(zf, &mut seen)?;
    cursor += 1;

    let OpKind::Cmp {
        src1,
        src2: SrcOperand::Imm(0),
        width: OpWidth::W64,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    if src1 != andnot_acc {
        return None;
    }
    cursor += 1;

    let OpKind::SetCC {
        dst: cf,
        cond: Condition::Eq,
        width: OpWidth::W64,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let cf = unique_virtual(cf, &mut seen)?;
    cursor += 1;

    let OpKind::Shl {
        dst: shifted_zf,
        src,
        amount: SrcOperand::Imm(6),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let shifted_zf = unique_virtual(shifted_zf, &mut seen)?;
    if src != zf {
        return None;
    }
    cursor += 1;

    let OpKind::And {
        dst: cleared,
        src1,
        src2: SrcOperand::Imm(clear_mask),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let cleared = unique_virtual(cleared, &mut seen)?;
    if src1 != old_flags || clear_mask != !0x8D5 {
        return None;
    }
    cursor += 1;

    let OpKind::Or {
        dst: with_cf,
        src1,
        src2: SrcOperand::Reg(src2),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let with_cf = unique_virtual(with_cf, &mut seen)?;
    if src1 != cleared || src2 != cf {
        return None;
    }
    cursor += 1;

    let OpKind::Or {
        dst: new_flags,
        src1,
        src2: SrcOperand::Reg(src2),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = sequence.get(cursor)?.kind
    else {
        return None;
    };
    let new_flags = unique_virtual(new_flags, &mut seen)?;
    if src1 != with_cf || src2 != shifted_zf {
        return None;
    }
    cursor += 1;

    let OpKind::WriteFlags { src } = sequence.get(cursor)?.kind else {
        return None;
    };
    if src != new_flags {
        return None;
    }
    cursor += 1;

    if cursor != expected_consumed
        || !local_virtual_counts_match(sequence, virtual_definitions, virtual_uses)
    {
        return None;
    }
    Some(X86JitVexPtestMemorySequence {
        consumed: expected_consumed,
        encoding,
    })
}
