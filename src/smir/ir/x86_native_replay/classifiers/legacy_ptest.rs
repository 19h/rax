//! Exact register-only legacy `PTEST` replay classification and semantic
//! graph validation.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, Condition, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, X86Reg,
};

/// Decoded architectural operands of one exact register-only legacy `PTEST`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyPtestReplay {
    pub(crate) first_source: u8,
    pub(crate) second_source: u8,
}

/// Expected block-wide definition/use counts for one temporary elided by
/// exact native replay.
pub(crate) type X86LegacyPtestVirtualRequirement = (VReg, usize, usize);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn exact_zero(operation: &SmirOp) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::Mov {
            dst: result @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => Some(result),
        _ => None,
    }
}

fn exact_extract(operation: &SmirOp, vector: VReg, lane: u8) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::VExtractLane {
            dst: result @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane => Some(result),
        _ => None,
    }
}

fn exact_and(operation: &SmirOp, lhs: VReg, rhs: VReg) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::And {
            dst: result @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Reg(actual_rhs),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == lhs && actual_rhs == rhs => Some(result),
        _ => None,
    }
}

fn exact_andnot(operation: &SmirOp, lhs: VReg, rhs: VReg) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::AndNot {
            dst: result @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Reg(actual_rhs),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == lhs && actual_rhs == rhs => Some(result),
        _ => None,
    }
}

fn exact_accumulate_or(operation: &SmirOp, accumulator: VReg, value: VReg) -> bool {
    operation.x86_hint.is_none()
        && matches!(
            operation.kind,
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(actual_value),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == accumulator && src1 == accumulator && actual_value == value
        )
}

fn exact_read_flags(operation: &SmirOp) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::ReadFlags {
            dst: result @ VReg::Virtual(_),
        } => Some(result),
        _ => None,
    }
}

fn exact_compare_zero(operation: &SmirOp, value: VReg) -> bool {
    operation.x86_hint.is_none()
        && matches!(
            operation.kind,
            OpKind::Cmp {
                src1,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
            } if src1 == value
        )
}

fn exact_set_equal(operation: &SmirOp) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::SetCC {
            dst: result @ VReg::Virtual(_),
            cond: Condition::Eq,
            width: OpWidth::W64,
        } => Some(result),
        _ => None,
    }
}

fn exact_shift_zf(operation: &SmirOp, zf: VReg) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::Shl {
            dst: result @ VReg::Virtual(_),
            src,
            amount: SrcOperand::Imm(6),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src == zf => Some(result),
        _ => None,
    }
}

fn exact_clear_defined_flags(operation: &SmirOp, old_flags: VReg) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::And {
            dst: result @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Imm(mask),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == old_flags && mask == !0x8D5i64 => Some(result),
        _ => None,
    }
}

fn exact_or(operation: &SmirOp, lhs: VReg, rhs: VReg) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::Or {
            dst: result @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Reg(actual_rhs),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == lhs && actual_rhs == rhs => Some(result),
        _ => None,
    }
}

fn exact_write_flags(operation: &SmirOp, value: VReg) -> bool {
    operation.x86_hint.is_none()
        && matches!(operation.kind, OpKind::WriteFlags { src } if src == value)
}

/// Validate the complete stable two-qword reduction and status-merge graph
/// emitted for one register-only legacy `PTEST`. Each returned tuple is
/// `(virtual register, definitions, uses)` so the grouping layer proves that
/// no elided temporary escapes the source instruction.
pub(crate) fn x86_legacy_ptest_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyPtestReplay,
) -> Option<Vec<X86LegacyPtestVirtualRequirement>> {
    let [
        zero_intersection,
        zero_outside,
        extract_first_0,
        extract_second_0,
        intersect_0,
        accumulate_intersection_0,
        outside_0,
        accumulate_outside_0,
        extract_first_1,
        extract_second_1,
        intersect_1,
        accumulate_intersection_1,
        outside_1,
        accumulate_outside_1,
        read_flags,
        compare_intersection,
        set_zf,
        compare_outside,
        set_cf,
        shift_zf,
        clear_defined_flags,
        merge_cf,
        merge_zf,
        write_flags,
    ] = ops
    else {
        return None;
    };

    let intersection_accumulator = exact_zero(zero_intersection)?;
    let outside_accumulator = exact_zero(zero_outside)?;
    let first = xmm(replay.first_source);
    let second = xmm(replay.second_source);

    let first_0 = exact_extract(extract_first_0, first, 0)?;
    let second_0 = exact_extract(extract_second_0, second, 0)?;
    let intersection_0 = exact_and(intersect_0, first_0, second_0)?;
    if !exact_accumulate_or(
        accumulate_intersection_0,
        intersection_accumulator,
        intersection_0,
    ) {
        return None;
    }
    let outside_value_0 = exact_andnot(outside_0, second_0, first_0)?;
    if !exact_accumulate_or(accumulate_outside_0, outside_accumulator, outside_value_0) {
        return None;
    }

    let first_1 = exact_extract(extract_first_1, first, 1)?;
    let second_1 = exact_extract(extract_second_1, second, 1)?;
    let intersection_1 = exact_and(intersect_1, first_1, second_1)?;
    if !exact_accumulate_or(
        accumulate_intersection_1,
        intersection_accumulator,
        intersection_1,
    ) {
        return None;
    }
    let outside_value_1 = exact_andnot(outside_1, second_1, first_1)?;
    if !exact_accumulate_or(accumulate_outside_1, outside_accumulator, outside_value_1) {
        return None;
    }

    let old_flags = exact_read_flags(read_flags)?;
    if !exact_compare_zero(compare_intersection, intersection_accumulator) {
        return None;
    }
    let zf = exact_set_equal(set_zf)?;
    if !exact_compare_zero(compare_outside, outside_accumulator) {
        return None;
    }
    let cf = exact_set_equal(set_cf)?;
    let shifted_zf = exact_shift_zf(shift_zf, zf)?;
    let cleared_flags = exact_clear_defined_flags(clear_defined_flags, old_flags)?;
    let flags_with_cf = exact_or(merge_cf, cleared_flags, cf)?;
    let final_flags = exact_or(merge_zf, flags_with_cf, shifted_zf)?;
    if !exact_write_flags(write_flags, final_flags) {
        return None;
    }

    let requirements = vec![
        (intersection_accumulator, 3, 3),
        (outside_accumulator, 3, 3),
        (first_0, 1, 2),
        (second_0, 1, 2),
        (intersection_0, 1, 1),
        (outside_value_0, 1, 1),
        (first_1, 1, 2),
        (second_1, 1, 2),
        (intersection_1, 1, 1),
        (outside_value_1, 1, 1),
        (old_flags, 1, 1),
        (zf, 1, 1),
        (cf, 1, 1),
        (shifted_zf, 1, 1),
        (cleared_flags, 1, 1),
        (flags_with_cf, 1, 1),
        (final_flags, 1, 1),
    ];
    let mut unique = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| unique.insert(*register))
        .then_some(requirements)
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy `PTEST`.
    ///
    /// The mandatory 66H prefix may be followed by one final REX prefix.
    /// REX.R/B extend the two XMM sources; REX.W/X are ignored by the
    /// instruction and retained in the exact replay bytes. Memory,
    /// other/reordered prefixes, duplicate REX, REX2, VEX/EVEX, truncated,
    /// and trailing-byte forms fail closed.
    pub(crate) fn legacy_register_ptest_replay(&self) -> Option<X86LegacyPtestReplay> {
        let (rex, modrm) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, 0x0F, 0x38, 0x17, modrm] => (Some(*rex), *modrm),
            [0x66, 0x0F, 0x38, 0x17, modrm] => (None, *modrm),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let rex = rex.unwrap_or(0);
        Some(X86LegacyPtestReplay {
            first_source: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            second_source: (modrm & 7) | ((rex & 0x01) << 3),
        })
    }
}
