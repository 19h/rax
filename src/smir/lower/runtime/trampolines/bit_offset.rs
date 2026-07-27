//! Native admission shape for `BT` with a register bit offset into memory.
//!
//! `bt [mem],reg` does not address `[mem]`: the architectural operand is a bit
//! string, so the accessed element is
//! `base + ((sign_extend(reg) >> log2(bits)) << log2(bytes))` and the bit index
//! is `reg mod bits`. The x86 lifter expands that into eight operations whose
//! `Load` address is a *virtual* expression, which no [`Address`] can describe
//! and therefore no memory helper could evaluate.
//!
//! The fused form hands the scaled term to the helper's address stage, where
//! every guest GPR has already been spilled and scratch registers are free.

use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::{Address, MemWidth, OpWidth, VReg};

pub use crate::smir::lower::X86JitBitOffsetTerm;

/// One validated memory `BT` with a register bit offset.
pub(crate) struct X86JitBitOffsetTest<'a> {
    pub(crate) guest_pc: u64,
    /// Base address of the bit string.
    pub(crate) addr: &'a Address,
    pub(crate) mem_width: MemWidth,
    pub(crate) width: OpWidth,
    pub(crate) term: X86JitBitOffsetTerm,
    /// Architectural GPR holding the raw bit offset.
    pub(crate) index_register: VReg,
}

fn element_shifts(width: OpWidth, mem_width: MemWidth) -> Option<(u8, u8)> {
    let right = match width {
        OpWidth::W16 => 4,
        OpWidth::W32 => 5,
        OpWidth::W64 => 6,
        _ => return None,
    };
    let left = match mem_width {
        MemWidth::B2 => 1,
        MemWidth::B4 => 2,
        MemWidth::B8 => 3,
        _ => return None,
    };
    Some((right, left))
}

/// Recognize the eight-operation memory `BT` starting at `index`.
pub(crate) fn x86_jit_mem_bit_offset_test_sequence<'a>(
    block: &'a SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitBitOffsetTest<'a>> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{SignExtend, SrcOperand};

    if !allow_mem {
        return None;
    }
    let guest_pc = block.ops.get(index)?.guest_pc;
    let at = |offset: usize| {
        block
            .ops
            .get(index + offset)
            .filter(|op| op.guest_pc == guest_pc && op.x86_hint.is_none())
    };

    let OpKind::SignExtend {
        dst: signed @ VReg::Virtual(_),
        src: index_register,
        from_width,
        to_width: OpWidth::W64,
    } = &at(0)?.kind
    else {
        return None;
    };
    if !super::x86_native_identity_gpr(index_register) {
        return None;
    }

    let OpKind::Sar {
        dst: delta @ VReg::Virtual(_),
        src: sar_src,
        amount: SrcOperand::Imm(shift_right),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &at(1)?.kind
    else {
        return None;
    };
    if sar_src != signed {
        return None;
    }

    let OpKind::Shl {
        dst: byte_delta @ VReg::Virtual(_),
        src: shl_src,
        amount: SrcOperand::Imm(shift_left),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &at(2)?.kind
    else {
        return None;
    };
    if shl_src != delta {
        return None;
    }

    let OpKind::Lea {
        dst: base @ VReg::Virtual(_),
        addr,
    } = &at(3)?.kind
    else {
        return None;
    };
    if !super::x86_jit_mem_address_shape_valid(addr) {
        return None;
    }

    let OpKind::Add {
        dst: effective @ VReg::Virtual(_),
        src1: add_base,
        src2: SrcOperand::Reg(add_delta),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &at(4)?.kind
    else {
        return None;
    };
    if add_base != base || add_delta != byte_delta {
        return None;
    }

    let OpKind::And {
        dst: normalized @ VReg::Virtual(_),
        src1: and_src,
        src2: SrcOperand::Imm(mask),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    } = &at(5)?.kind
    else {
        return None;
    };
    if and_src != index_register {
        return None;
    }

    let OpKind::Load {
        dst: value @ VReg::Virtual(_),
        addr: Address::Direct(load_base),
        width: mem_width,
        sign: SignExtend::Zero,
    } = &at(6)?.kind
    else {
        return None;
    };
    if load_base != effective {
        return None;
    }

    let OpKind::Bt {
        src: tested,
        index: SrcOperand::Reg(bit_index),
        width,
    } = &at(7)?.kind
    else {
        return None;
    };
    if tested != value || bit_index != normalized || width != from_width {
        return None;
    }

    let (expected_right, expected_left) = element_shifts(*width, *mem_width)?;
    if i64::from(expected_right) != *shift_right
        || i64::from(expected_left) != *shift_left
        || *mask != i64::from(width.bits()) - 1
    {
        return None;
    }

    // Every temporary the expansion introduces is consumed exactly once by it.
    for temporary in [
        signed, delta, byte_delta, base, effective, normalized, value,
    ] {
        if virtual_definitions.get(temporary) != Some(&1) || virtual_uses.get(temporary) != Some(&1)
        {
            return None;
        }
    }

    Some(X86JitBitOffsetTest {
        guest_pc,
        addr,
        mem_width: *mem_width,
        width: *width,
        term: X86JitBitOffsetTerm {
            index: super::x86_native_identity_gpr_index(index_register)?,
            from_width: *from_width,
            shift_right: expected_right,
            shift_left: expected_left,
        },
        index_register: *index_register,
    })
}

/// Length of the fused memory `BT` starting at `index`, if any.
pub(crate) fn x86_jit_mem_bit_offset_test_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_mem_bit_offset_test_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|_| 8)
}
