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

/// The seven operations every memory bit-string operation shares: the scaled
/// address computation, the index normalization, and the element read.
pub(crate) struct X86JitBitOffsetPrefix<'a> {
    pub(crate) addr: &'a Address,
    pub(crate) mem_width: MemWidth,
    pub(crate) width: OpWidth,
    pub(crate) term: X86JitBitOffsetTerm,
}

/// Parse the shared prefix, returning it with the temporaries later operations
/// must consume. Definition counts are checked here; use counts differ between
/// the test-only and update forms and are checked by each caller.
#[allow(clippy::type_complexity)]
fn x86_jit_bit_offset_prefix<'a>(
    block: &'a SmirBlock,
    index: usize,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
) -> Option<(
    X86JitBitOffsetPrefix<'a>,
    &'a VReg,
    &'a VReg,
    &'a VReg,
    VReg,
)> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{SignExtend, SrcOperand};

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

    let width = *from_width;
    let (expected_right, expected_left) = element_shifts(width, *mem_width)?;
    if i64::from(expected_right) != *shift_right
        || i64::from(expected_left) != *shift_left
        || *mask != i64::from(width.bits()) - 1
    {
        return None;
    }
    for temporary in [
        signed, delta, byte_delta, base, effective, normalized, value,
    ] {
        if virtual_definitions.get(temporary) != Some(&1) {
            return None;
        }
    }

    Some((
        X86JitBitOffsetPrefix {
            addr,
            mem_width: *mem_width,
            width,
            term: X86JitBitOffsetTerm {
                index: super::x86_native_identity_gpr_index(index_register)?,
                from_width: width,
                shift_right: expected_right,
                shift_left: expected_left,
            },
        },
        effective,
        normalized,
        value,
        *index_register,
    ))
}

/// Recognize the eight-operation memory `BT` starting at `index`.
pub(crate) fn x86_jit_mem_bit_offset_test_sequence<'a>(
    block: &'a SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitBitOffsetTest<'a>> {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::SrcOperand;

    if !allow_mem {
        return None;
    }
    let (prefix, effective, normalized, value, index_register) =
        x86_jit_bit_offset_prefix(block, index, virtual_definitions)?;

    let test = block
        .ops
        .get(index + 7)
        .filter(|op| op.guest_pc == block.ops[index].guest_pc && op.x86_hint.is_none())?;
    let OpKind::Bt {
        src: tested,
        index: SrcOperand::Reg(bit_index),
        width,
    } = &test.kind
    else {
        return None;
    };
    if tested != value || bit_index != normalized || *width != prefix.width {
        return None;
    }

    // The pure test form consumes every temporary exactly once.
    for temporary in [effective, normalized, value] {
        if virtual_uses.get(temporary) != Some(&1) {
            return None;
        }
    }
    if !x86_jit_bit_offset_address_temporaries_single_use(block, index, virtual_uses) {
        return None;
    }

    Some(X86JitBitOffsetTest {
        guest_pc: block.ops[index].guest_pc,
        addr: prefix.addr,
        mem_width: prefix.mem_width,
        width: prefix.width,
        term: prefix.term,
        index_register,
    })
}

/// The four address temporaries are always consumed exactly once.
fn x86_jit_bit_offset_address_temporaries_single_use(
    block: &SmirBlock,
    index: usize,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> bool {
    use crate::smir::ir::ops::OpKind;

    for offset in 0..4 {
        let Some(op) = block.ops.get(index + offset) else {
            return false;
        };
        let dst = match &op.kind {
            OpKind::SignExtend { dst, .. }
            | OpKind::Sar { dst, .. }
            | OpKind::Shl { dst, .. }
            | OpKind::Lea { dst, .. } => dst,
            _ => return false,
        };
        if virtual_uses.get(dst) != Some(&1) {
            return false;
        }
    }
    true
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

/// Which bit-string update a `BTS`/`BTR`/`BTC` performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86JitBitUpdate {
    /// `BTS`: `element |= mask`.
    Set,
    /// `BTR`: `element &= !mask`.
    Reset,
    /// `BTC`: `element ^= mask`.
    Complement,
}

/// One validated memory `BTS`/`BTR`/`BTC` with a register bit offset.
pub(crate) struct X86JitBitOffsetUpdate<'a> {
    pub(crate) consumed: usize,
    pub(crate) guest_pc: u64,
    pub(crate) addr: &'a Address,
    pub(crate) mem_width: MemWidth,
    pub(crate) width: OpWidth,
    pub(crate) term: X86JitBitOffsetTerm,
    pub(crate) index_register: VReg,
    pub(crate) update: X86JitBitUpdate,
    /// Whether the architectural CF survived optimization.
    pub(crate) publishes_cf: bool,
}

/// Recognize the memory `BTS`/`BTR`/`BTC` expansion starting at `index`.
///
/// It shares the first seven operations with the plain test and continues with
/// `Mov mask,1 ; Shl mask,index ; [Not mask] ; <Or|And|Xor> new,old,mask ;
/// Store new` plus the optional trailing `Bt`. The architectural CF is
/// committed only after the store retires, so the fused lowering emits it last.
pub(crate) fn x86_jit_mem_bit_offset_update_sequence<'a>(
    block: &'a SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86JitBitOffsetUpdate<'a>> {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::SrcOperand;

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

    let (prefix, effective, normalized, old_value, index_register) =
        x86_jit_bit_offset_prefix(block, index, virtual_definitions)?;

    // mask = 1 << (index mod bits)
    let OpKind::Mov {
        dst: mask @ VReg::Virtual(_),
        src: SrcOperand::Imm(1),
        width: mask_width,
    } = &at(7)?.kind
    else {
        return None;
    };
    let OpKind::Shl {
        dst: shifted_mask,
        src: shift_src,
        amount: SrcOperand::Reg(shift_amount),
        width: shift_width,
        flags: FlagUpdate::None,
    } = &at(8)?.kind
    else {
        return None;
    };
    if shifted_mask != mask
        || shift_src != mask
        || shift_amount != normalized
        || shift_width != mask_width
    {
        return None;
    }

    let mut cursor = 9;
    let mut mask_definitions = 2usize;
    let mut mask_uses = 2usize;
    let inverted = matches!(
        at(cursor).map(|op| &op.kind),
        Some(OpKind::Not { dst, src, width })
            if dst == mask && src == mask && width == mask_width
    );
    if inverted {
        cursor += 1;
        mask_definitions += 1;
        mask_uses += 1;
    }

    let combine = at(cursor)?;
    let (update, new_value, combine_old, combine_mask, combine_width, combine_flags) =
        match &combine.kind {
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            } => (X86JitBitUpdate::Set, dst, src1, src2, width, flags),
            OpKind::And {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            } => (X86JitBitUpdate::Reset, dst, src1, src2, width, flags),
            OpKind::Xor {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            } => (X86JitBitUpdate::Complement, dst, src1, src2, width, flags),
            _ => return None,
        };
    // Only `BTR` complements the mask, and only `BTR` combines with AND.
    if inverted != matches!(update, X86JitBitUpdate::Reset)
        || !matches!(new_value, VReg::Virtual(_))
        || combine_old != old_value
        || combine_mask != mask
        || combine_width != mask_width
        || *combine_flags != FlagUpdate::None
        || virtual_definitions.get(new_value) != Some(&1)
        || virtual_uses.get(new_value) != Some(&1)
    {
        return None;
    }
    cursor += 1;

    let OpKind::Store {
        src: stored,
        addr: Address::Direct(store_base),
        width: store_width,
    } = &at(cursor)?.kind
    else {
        return None;
    };
    if stored != new_value || store_base != effective || store_width != &prefix.mem_width {
        return None;
    }
    cursor += 1;

    // Optional architectural CF, deleted by optimization when it is dead.
    let mut publishes_cf = false;
    if let Some(test) = at(cursor) {
        if let OpKind::Bt {
            src,
            index: SrcOperand::Reg(bit_index),
            width,
        } = &test.kind
        {
            if src == old_value && bit_index == normalized && width == &prefix.width {
                publishes_cf = true;
                cursor += 1;
            }
        }
    }

    if !x86_jit_bit_offset_address_temporaries_single_use(block, index, virtual_uses) {
        return None;
    }
    if *mask_width != prefix.width
        || virtual_definitions.get(mask) != Some(&mask_definitions)
        || virtual_uses.get(mask) != Some(&mask_uses)
        || virtual_uses.get(effective) != Some(&2)
        || virtual_uses.get(normalized) != Some(&(1 + usize::from(publishes_cf)))
        || virtual_uses.get(old_value) != Some(&(1 + usize::from(publishes_cf)))
    {
        return None;
    }

    Some(X86JitBitOffsetUpdate {
        consumed: cursor,
        guest_pc,
        addr: prefix.addr,
        mem_width: prefix.mem_width,
        width: prefix.width,
        term: prefix.term,
        index_register,
        update,
        publishes_cf,
    })
}

/// Length of the fused memory bit-string update starting at `index`, if any.
pub(crate) fn x86_jit_mem_bit_offset_update_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_mem_bit_offset_update_sequence(
        block,
        index,
        allow_mem,
        virtual_definitions,
        virtual_uses,
    )
    .map(|sequence| sequence.consumed)
}
