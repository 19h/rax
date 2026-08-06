//! Exact native-admission shapes for x86 LAHF and SAHF.

use std::collections::HashMap;

use crate::smir::ir::SmirBlock;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

const STATUS_MASK: i64 = 0xD5;
const CLEAR_AH_MASK: i64 = !0xFF00;
const SEQUENCE_LEN: usize = 6;

/// Native x86 instruction selected by one exact AH/flags lift graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum X86JitAhFlagsKind {
    Lahf,
    Sahf,
}

/// One validated LAHF/SAHF graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct X86JitAhFlags {
    pub(crate) kind: X86JitAhFlagsKind,
    pub(crate) consumed: usize,
}

fn virtual_profile(
    reg: &VReg,
    definitions: &HashMap<VReg, usize>,
    uses: &HashMap<VReg, usize>,
    expected_definitions: usize,
    expected_uses: usize,
) -> bool {
    matches!(reg, VReg::Virtual(_))
        && definitions.get(reg) == Some(&expected_definitions)
        && uses.get(reg) == Some(&expected_uses)
}

fn one_guest_instruction(sequence: &[SmirOp]) -> bool {
    let Some(first) = sequence.first() else {
        return false;
    };
    sequence
        .iter()
        .all(|op| op.guest_pc == first.guest_pc && op.x86_hint.is_none())
}

fn sahf_sequence(
    sequence: &[SmirOp],
    definitions: &HashMap<VReg, usize>,
    uses: &HashMap<VReg, usize>,
) -> bool {
    let OpKind::ReadFlags { dst: old_flags } = &sequence[0].kind else {
        return false;
    };
    let OpKind::Shr {
        dst: ah,
        src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
        amount: SrcOperand::Imm(8),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[1].kind
    else {
        return false;
    };
    let OpKind::And {
        dst: status,
        src1: status_source,
        src2: SrcOperand::Imm(STATUS_MASK),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[2].kind
    else {
        return false;
    };
    let OpKind::And {
        dst: preserved,
        src1: preserved_source,
        src2: SrcOperand::Imm(clear_status_mask),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[3].kind
    else {
        return false;
    };
    let OpKind::Or {
        dst: merged,
        src1: merged_preserved,
        src2: SrcOperand::Reg(merged_status),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[4].kind
    else {
        return false;
    };
    let OpKind::WriteFlags { src: written_flags } = &sequence[5].kind else {
        return false;
    };

    status_source == ah
        && preserved_source == old_flags
        && *clear_status_mask == !STATUS_MASK
        && merged_preserved == preserved
        && merged_status == status
        && written_flags == merged
        && [old_flags, ah, status, preserved, merged]
            .into_iter()
            .all(|reg| virtual_profile(&reg, definitions, uses, 1, 1))
}

fn lahf_sequence(
    sequence: &[SmirOp],
    definitions: &HashMap<VReg, usize>,
    uses: &HashMap<VReg, usize>,
) -> bool {
    let OpKind::ReadFlags { dst: flags } = &sequence[0].kind else {
        return false;
    };
    let OpKind::And {
        dst: status,
        src1: status_source,
        src2: SrcOperand::Imm(STATUS_MASK),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[1].kind
    else {
        return false;
    };
    let OpKind::Or {
        dst: status_with_reserved,
        src1: reserved_source,
        src2: SrcOperand::Imm(2),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[2].kind
    else {
        return false;
    };
    let OpKind::Shl {
        dst: shifted,
        src: shifted_source,
        amount: SrcOperand::Imm(8),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[3].kind
    else {
        return false;
    };
    let OpKind::And {
        dst: cleared_rax,
        src1: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
        src2: SrcOperand::Imm(CLEAR_AH_MASK),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[4].kind
    else {
        return false;
    };
    let OpKind::Or {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
        src1: merged_rax,
        src2: SrcOperand::Reg(merged_status),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sequence[5].kind
    else {
        return false;
    };

    status_source == flags
        && status_with_reserved == status
        && reserved_source == status
        && shifted_source == status
        && merged_rax == cleared_rax
        && merged_status == shifted
        && virtual_profile(&flags, definitions, uses, 1, 1)
        && virtual_profile(&status, definitions, uses, 2, 2)
        && virtual_profile(&shifted, definitions, uses, 1, 1)
        && virtual_profile(&cleared_rax, definitions, uses, 1, 1)
}

/// Recognize one exact six-op LAHF/SAHF graph at `index`.
///
/// Generic `ReadFlags` and `WriteFlags` remain fail-closed. Admission is granted
/// only when every temporary is block-local, every flag-preserving operation
/// matches the strict x86 lifter graph, and all operations belong to one guest
/// instruction. The x86 lowerer consumes the graph as the canonical one-byte
/// host instruction so no virtual temporary can alias a live guest GPR.
pub(crate) fn x86_jit_ah_flags_sequence(
    block: &SmirBlock,
    index: usize,
    definitions: &HashMap<VReg, usize>,
    uses: &HashMap<VReg, usize>,
) -> Option<X86JitAhFlags> {
    let end = index.checked_add(SEQUENCE_LEN)?;
    let sequence = block.ops.get(index..end)?;
    if !one_guest_instruction(sequence)
        || block
            .ops
            .get(end)
            .is_some_and(|next| next.guest_pc == sequence[0].guest_pc)
    {
        return None;
    }
    let kind = if sahf_sequence(sequence, definitions, uses) {
        X86JitAhFlagsKind::Sahf
    } else if lahf_sequence(sequence, definitions, uses) {
        X86JitAhFlagsKind::Lahf
    } else {
        return None;
    };
    Some(X86JitAhFlags {
        kind,
        consumed: SEQUENCE_LEN,
    })
}

/// Return the exact six-op LAHF/SAHF graph length at `index`.
pub(crate) fn x86_jit_ah_flags_sequence_len(
    block: &SmirBlock,
    index: usize,
    definitions: &HashMap<VReg, usize>,
    uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_ah_flags_sequence(block, index, definitions, uses).map(|sequence| sequence.consumed)
}
