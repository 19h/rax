//! Native admission shape for a memory-source compare against a state-backed
//! GPR.
//!
//! `cmp [mem],rbp` / `test [mem],rsp` and their APX EGPR forms lift to
//! `Load v,[mem] ; CMP|TEST` with one architectural operand that does not live
//! in the host register of the same name: guest RSP/RBP are held in the
//! `GuestRegs` file (hardware RSP is the host stack pointer and hardware RBP is
//! the native frame pointer), and the EGPRs have no identity mapping at all.
//!
//! The generic memory-source fusion uses the non-memory operand as the transfer
//! register for the MMU helper, so it can only accept identity-mapped GPRs.
//! This shape stages the helper result on the caller frame instead and reloads
//! the architectural operand from its slot, leaving both host registers alone.

use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::{Address, MemWidth, OpWidth, VReg};

/// One validated memory-source compare against a state-backed GPR.
pub(crate) struct X86JitMemStateCompare<'a> {
    pub(crate) guest_pc: u64,
    pub(crate) addr: &'a Address,
    pub(crate) mem_width: MemWidth,
    pub(crate) width: OpWidth,
    /// Architectural GPR encoding index of the state-backed operand.
    pub(crate) state_index: u8,
    /// `true` for `TEST`, `false` for `CMP`.
    pub(crate) is_test: bool,
    /// `true` when the memory operand is the first `CMP` source, so the native
    /// compare must compute `memory - register`.
    pub(crate) memory_is_first: bool,
}

fn state_backed_index(reg: &VReg) -> Option<u8> {
    match reg {
        VReg::Arch(crate::smir::ir::types::ArchReg::X86(x86)) => x86
            .gpr_index()
            .filter(|index| *index >= 16 || matches!(index, 4 | 5)),
        _ => None,
    }
}

/// Recognize `Load v,[mem]; CMP|TEST` whose non-memory operand is state-backed.
pub(crate) fn x86_jit_mem_state_compare_sequence<'a>(
    block: &'a SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitMemStateCompare<'a>> {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{SignExtend, SrcOperand};

    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let OpKind::Load {
        dst: temporary @ VReg::Virtual(_),
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
        || virtual_definitions.get(temporary) != Some(&1)
        || virtual_uses.get(temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let (is_test, src1, src2, consumer_width) = match &consumer.kind {
        OpKind::Cmp { src1, src2, width } => (false, src1, src2, *width),
        OpKind::Test { src1, src2, width } => (true, src1, src2, *width),
        _ => return None,
    };
    if consumer_width != width {
        return None;
    }
    let SrcOperand::Reg(src2) = src2 else {
        return None;
    };

    let (state_index, memory_is_first) = if src1 == temporary {
        (state_backed_index(src2)?, true)
    } else if src2 == temporary {
        (state_backed_index(src1)?, false)
    } else {
        return None;
    };

    Some(X86JitMemStateCompare {
        guest_pc: load.guest_pc,
        addr,
        mem_width: *mem_width,
        width,
        state_index,
        is_test,
        memory_is_first,
    })
}

/// Length of the fused memory/state-backed compare starting at `index`, if any.
pub(crate) fn x86_jit_mem_state_compare_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_mem_state_compare_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|_| 2)
}
