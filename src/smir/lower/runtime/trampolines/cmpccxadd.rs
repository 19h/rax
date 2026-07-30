//! Fail-closed original-VEX `CMPccXADD` native admission.

use std::collections::HashMap;

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, GuestAddr, MemWidth, MemoryOrder, VReg,
};
use crate::smir::ir::{SmirBlock, X86InstructionBytes, X86VexCmpccxaddMemoryEncoding};

/// One exact original-VEX `CMPccXADD` transaction consumed by the helper-backed
/// x86-64 lowerer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct X86JitCmpccxadd<'a> {
    pub(crate) consumed: usize,
    pub(crate) guest_pc: u64,
    pub(crate) addr: &'a Address,
    pub(crate) encoding: X86VexCmpccxaddMemoryEncoding,
}

fn x86_condition_code(condition: Condition) -> Option<u8> {
    Some(match condition {
        Condition::Overflow => 0x0,
        Condition::NoOverflow => 0x1,
        Condition::Ult => 0x2,
        Condition::Uge => 0x3,
        Condition::Eq => 0x4,
        Condition::Ne => 0x5,
        Condition::Ule => 0x6,
        Condition::Ugt => 0x7,
        Condition::Negative => 0x8,
        Condition::Positive => 0x9,
        Condition::Parity => 0xA,
        Condition::NoParity => 0xB,
        Condition::Slt => 0xC,
        Condition::Sge => 0xD,
        Condition::Sle => 0xE,
        Condition::Sgt => 0xF,
        Condition::Always => return None,
    })
}

fn x86_gpr_index(reg: VReg) -> Option<u8> {
    let VReg::Arch(ArchReg::X86(reg)) = reg else {
        return None;
    };
    reg.gpr_index().filter(|index| *index < 16)
}

/// Validate one complete original-VEX `CMPccXADD` instruction.
///
/// Exact source provenance prevents the semantically identical APX-promoted
/// EVEX IR from entering this VEX-only native path. The instruction must be an
/// isolated, unhinted `X86CheckAlignmentAc`/`AtomicCmpXadd` pair whose
/// architectural operands, fault metadata, condition, width, order, and
/// state-backed address all agree with the complete bytes. Runtime and
/// auxiliary space are O(1).
pub(crate) fn x86_jit_cmpccxadd_sequence<'a>(
    block: &'a SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> Option<X86JitCmpccxadd<'a>> {
    if !allow_mem {
        return None;
    }
    let alignment_op = block.ops.get(index)?;
    if index != 0 && block.ops[index - 1].guest_pc == alignment_op.guest_pc {
        return None;
    }
    let op = block.ops.get(index + 1)?;
    if op.guest_pc != alignment_op.guest_pc {
        return None;
    }
    if block
        .ops
        .get(index + 2)
        .is_some_and(|next| next.guest_pc == op.guest_pc)
    {
        return None;
    }
    let instruction = instruction_bytes.get(&(block.id, op.guest_pc))?;
    let encoding = instruction.vex_cmpccxadd_memory_encoding()?;

    let OpKind::AtomicCmpXadd {
        dst_old,
        addr,
        cmp,
        add,
        cond,
        width,
        order: MemoryOrder::SeqCst,
    } = &op.kind
    else {
        return None;
    };
    let OpKind::X86CheckAlignmentAc {
        addr: checked_addr,
        access_size,
        alignment,
        stack_segment,
        natural_alignment: false,
    } = &alignment_op.kind
    else {
        return None;
    };
    if alignment_op.x86_hint.is_some()
        || op.x86_hint.is_some()
        || checked_addr != addr
        || *access_size != width.bytes() as u8
        || *alignment != width.bytes() as u8
        || *stack_segment != encoding.stack_segment
        || dst_old != cmp
        || !super::x86_jit_mem_address_shape_valid(addr)
        || !matches!(width, MemWidth::B4 | MemWidth::B8)
        || x86_gpr_index(*cmp) != Some(encoding.cmp)
        || x86_gpr_index(*add) != Some(encoding.add)
        || x86_condition_code(*cond) != Some(encoding.condition_code)
        || *width != encoding.width
    {
        return None;
    }

    Some(X86JitCmpccxadd {
        consumed: 2,
        guest_pc: op.guest_pc,
        addr,
        encoding,
    })
}

pub(crate) fn x86_jit_cmpccxadd_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> Option<usize> {
    x86_jit_cmpccxadd_sequence(block, index, allow_mem, instruction_bytes)
        .map(|sequence| sequence.consumed)
}
