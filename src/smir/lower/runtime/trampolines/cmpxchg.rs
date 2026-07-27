//! Native admission shape for memory-destination `CMPXCHG`.
//!
//! The x86 lifter expands a memory `CMPXCHG` into
//! `Mov v_src,src ; Mov v_acc,RAX ; Load v_old,[mem] ; Cmp v_acc,v_old ;
//!  SetCC v_eq,Eq ; Select v_new,v_eq,v_src,v_old ; PredStore v_new,v_eq,[mem] ;
//!  CMove RAX,v_old,Ne`.
//!
//! Optimization folds either snapshot `Mov` into its consumer and deletes the
//! accumulator write-back when RAX is dead, so the recognizer accepts the whole
//! family. Every intermediate value is virtual, which is what kept the
//! instruction off the native tier.
//!
//! Note that a `LOCK`-prefixed `CMPXCHG` lifts to the same predicated shape:
//! the emulator provides no stronger indivisibility in either interpreter, so
//! the fused native form reproduces interpretation exactly.

use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::{Address, MemWidth, OpWidth, VReg};

/// A scalar operand that the fused lowering can materialize without a
/// guest-register home: an identity-mapped architectural GPR or an immediate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86JitScalarValue {
    Register(VReg),
    Immediate(i64),
}

/// One validated memory-destination `CMPXCHG`.
pub(crate) struct X86JitCmpxchg<'a> {
    pub(crate) consumed: usize,
    pub(crate) guest_pc: u64,
    pub(crate) addr: &'a Address,
    pub(crate) mem_width: MemWidth,
    pub(crate) width: OpWidth,
    /// Value compared against the memory operand (architecturally RAX).
    pub(crate) accumulator: X86JitScalarValue,
    /// Value written back when the comparison matches.
    pub(crate) source: X86JitScalarValue,
    /// Whether the architectural accumulator write-back survived optimization.
    pub(crate) writes_accumulator: bool,
}

fn identity_value(operand: &VReg) -> Option<X86JitScalarValue> {
    super::x86_native_identity_gpr(operand).then_some(X86JitScalarValue::Register(*operand))
}

/// Recognize the fused memory `CMPXCHG` starting at `index`.
pub(crate) fn x86_jit_cmpxchg_sequence<'a>(
    block: &'a SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitCmpxchg<'a>> {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, Condition, SignExtend, SrcOperand, X86Reg};

    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let same_instruction = |op: &crate::smir::ir::ops::SmirOp| op.guest_pc == guest_pc;

    // Zero, one, or two leading snapshot MOVs, depending on what optimization
    // folded into the consumers.
    let mut snapshots: Vec<(VReg, X86JitScalarValue)> = Vec::new();
    let mut cursor = index;
    while snapshots.len() < 2 {
        let Some(op) = block.ops.get(cursor).filter(|op| same_instruction(op)) else {
            break;
        };
        let OpKind::Mov {
            dst: dst @ VReg::Virtual(_),
            src,
            ..
        } = &op.kind
        else {
            break;
        };
        let value = match src {
            SrcOperand::Reg(source) => identity_value(source)?,
            SrcOperand::Imm(value) => X86JitScalarValue::Immediate(*value),
            _ => return None,
        };
        if op.x86_hint.is_some() || virtual_definitions.get(dst) != Some(&1) {
            return None;
        }
        snapshots.push((*dst, value));
        cursor += 1;
    }

    let load = block.ops.get(cursor).filter(|op| same_instruction(op))?;
    let OpKind::Load {
        dst: old @ VReg::Virtual(_),
        addr,
        width: mem_width,
        sign: SignExtend::Zero,
    } = &load.kind
    else {
        return None;
    };
    let width = mem_width.to_op_width()?;
    if !matches!(
        width,
        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
    ) || !super::x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(old) != Some(&1)
    {
        return None;
    }

    let compare = block
        .ops
        .get(cursor + 1)
        .filter(|op| same_instruction(op))?;
    let OpKind::Cmp {
        src1: accumulator,
        src2: SrcOperand::Reg(compared),
        width: compare_width,
    } = &compare.kind
    else {
        return None;
    };
    if compared != old || *compare_width != width {
        return None;
    }

    let set = block
        .ops
        .get(cursor + 2)
        .filter(|op| same_instruction(op))?;
    let OpKind::SetCC {
        dst: matched @ VReg::Virtual(_),
        cond: Condition::Eq,
        width: OpWidth::W8,
    } = &set.kind
    else {
        return None;
    };
    if set.x86_hint.is_some() || virtual_definitions.get(matched) != Some(&1) {
        return None;
    }

    let select = block
        .ops
        .get(cursor + 3)
        .filter(|op| same_instruction(op))?;
    let OpKind::Select {
        dst: new_value @ VReg::Virtual(_),
        cond,
        src_true: source,
        src_false,
        width: select_width,
    } = &select.kind
    else {
        return None;
    };
    if cond != matched
        || src_false != old
        || *select_width != width
        || virtual_definitions.get(new_value) != Some(&1)
        || virtual_uses.get(new_value) != Some(&1)
    {
        return None;
    }

    let store = block
        .ops
        .get(cursor + 4)
        .filter(|op| same_instruction(op))?;
    let OpKind::PredStore {
        src: SrcOperand::Reg(stored),
        cond: store_cond,
        addr: store_addr,
        width: store_width,
    } = &store.kind
    else {
        return None;
    };
    if stored != new_value
        || store_cond != matched
        || store_addr != addr
        || *store_width != *mem_width
    {
        return None;
    }

    // Optional architectural accumulator write-back on a mismatch.
    let mut consumed = cursor + 5 - index;
    let mut writes_accumulator = false;
    if let Some(write) = block.ops.get(cursor + 5).filter(|op| same_instruction(op)) {
        if let OpKind::CMove {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src,
            cond: Condition::Ne,
            width: move_width,
        } = &write.kind
        {
            if src == old && *move_width == width && write.x86_hint.is_none() {
                writes_accumulator = true;
                consumed += 1;
            }
        }
    }

    // Resolve the accumulator and source operands against the snapshots.
    let resolve = |operand: &VReg| -> Option<X86JitScalarValue> {
        snapshots
            .iter()
            .find(|(snapshot, _)| snapshot == operand)
            .map(|(_, value)| *value)
            .or_else(|| identity_value(operand))
    };
    let accumulator_value = resolve(accumulator)?;
    let source_value = resolve(source)?;

    // Every virtual the shape introduces must be consumed exactly by it.
    let expected_old_uses = 2 + usize::from(writes_accumulator);
    if virtual_uses.get(old) != Some(&expected_old_uses) || virtual_uses.get(matched) != Some(&2) {
        return None;
    }
    for (snapshot, _) in &snapshots {
        let expected = usize::from(snapshot == accumulator) + usize::from(snapshot == source);
        if expected == 0 || virtual_uses.get(snapshot).copied().unwrap_or(0) != expected {
            return None;
        }
    }

    Some(X86JitCmpxchg {
        consumed,
        guest_pc,
        addr,
        mem_width: *mem_width,
        width,
        accumulator: accumulator_value,
        source: source_value,
        writes_accumulator,
    })
}

/// Length of the fused memory `CMPXCHG` starting at `index`, if any.
pub(crate) fn x86_jit_cmpxchg_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_cmpxchg_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|sequence| sequence.consumed)
}
