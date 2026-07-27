//! Native admission shape for `PUSH m16/m64`.
//!
//! A memory-operand push lifts to `Load v,[mem] ; SUB RSP,n ; Store v,[RSP]`.
//! The generic push fusion only accepts an architectural register or immediate
//! as the stored value, so the memory form was an unconditional interpreter
//! frontier. Staging the loaded value on a caller frame lets the same
//! helper-backed store and state-backed RSP update handle it.

use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::{Address, MemWidth, VReg};

/// One validated memory-operand push.
pub(crate) struct X86JitPushMemory<'a> {
    pub(crate) guest_pc: u64,
    /// Source address of the pushed value.
    pub(crate) source: &'a Address,
    /// Access width of the source read.
    pub(crate) source_width: MemWidth,
    /// Architectural stack decrement, and therefore the pushed width.
    pub(crate) delta: i64,
    pub(crate) push_width: MemWidth,
}

/// Recognize `Load v,[mem]; SUB RSP,n; Store v,[RSP]`.
pub(crate) fn x86_jit_push_memory_sequence<'a>(
    block: &'a SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitPushMemory<'a>> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SignExtend, SrcOperand, X86Reg};

    if !allow_mem {
        return None;
    }
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));

    let load = block.ops.get(index)?;
    let OpKind::Load {
        dst: temporary @ VReg::Virtual(_),
        addr: source,
        width: source_width,
        sign: SignExtend::Zero,
    } = &load.kind
    else {
        return None;
    };
    if !matches!(
        source_width,
        MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8
    ) || !super::x86_jit_mem_address_shape_valid(source)
        || virtual_definitions.get(temporary) != Some(&1)
        || virtual_uses.get(temporary) != Some(&1)
    {
        return None;
    }
    // The pushed value is the zero-extended source read, so a wider source than
    // the stack slot would silently drop bits.
    let sub = block.ops.get(index + 1)?;
    let store = block.ops.get(index + 2)?;
    if sub.guest_pc != load.guest_pc || store.guest_pc != load.guest_pc {
        return None;
    }
    let OpKind::Sub {
        dst,
        src1,
        src2: SrcOperand::Imm(delta @ (2 | 8)),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sub.kind
    else {
        return None;
    };
    if *dst != rsp || *src1 != rsp {
        return None;
    }
    let push_width = if *delta == 2 {
        MemWidth::B2
    } else {
        MemWidth::B8
    };
    if source_width.bytes() > push_width.bytes() {
        return None;
    }
    let OpKind::Store {
        src,
        addr: Address::Direct(base),
        width,
    } = &store.kind
    else {
        return None;
    };
    if src != temporary || *base != rsp || *width != push_width {
        return None;
    }

    Some(X86JitPushMemory {
        guest_pc: load.guest_pc,
        source,
        source_width: *source_width,
        delta: *delta,
        push_width,
    })
}

/// One validated `PUSHF`/`PUSHFQ`.
pub(crate) struct X86JitPushFlags {
    pub(crate) guest_pc: u64,
    /// Architectural stack decrement, and therefore the pushed width.
    pub(crate) delta: i64,
    pub(crate) push_width: MemWidth,
}

/// Recognize `ReadFlags v; SUB RSP,n; Store v,[RSP]`.
///
/// The lowerer already materializes the complete architectural flag image
/// (host status flags plus the state-backed guest AC); only its virtual
/// destination kept the sequence off the native tier.
pub(crate) fn x86_jit_push_flags_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitPushFlags> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, X86Reg};

    if !allow_mem {
        return None;
    }
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));

    let read = block.ops.get(index)?;
    let OpKind::ReadFlags {
        dst: temporary @ VReg::Virtual(_),
    } = &read.kind
    else {
        return None;
    };
    if read.x86_hint.is_some()
        || virtual_definitions.get(temporary) != Some(&1)
        || virtual_uses.get(temporary) != Some(&1)
    {
        return None;
    }

    let sub = block.ops.get(index + 1)?;
    let store = block.ops.get(index + 2)?;
    if sub.guest_pc != read.guest_pc || store.guest_pc != read.guest_pc {
        return None;
    }
    let OpKind::Sub {
        dst,
        src1,
        src2: SrcOperand::Imm(delta @ (2 | 8)),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &sub.kind
    else {
        return None;
    };
    if *dst != rsp || *src1 != rsp {
        return None;
    }
    let push_width = if *delta == 2 {
        MemWidth::B2
    } else {
        MemWidth::B8
    };
    let OpKind::Store {
        src,
        addr: Address::Direct(base),
        width,
    } = &store.kind
    else {
        return None;
    };
    if src != temporary || *base != rsp || *width != push_width {
        return None;
    }

    Some(X86JitPushFlags {
        guest_pc: read.guest_pc,
        delta: *delta,
        push_width,
    })
}

/// Length of the fused flag push starting at `index`, if any.
pub(crate) fn x86_jit_push_flags_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_push_flags_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|_| 3)
}

/// Length of the fused memory-operand push starting at `index`, if any.
pub(crate) fn x86_jit_push_memory_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_push_memory_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|_| 3)
}
