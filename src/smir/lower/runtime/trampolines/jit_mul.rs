//! Exact helper-backed x86 scalar multiply sequence validation.

use std::collections::HashMap;

use super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint};
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, SrcOperand, VReg, X86Reg};

/// Validate the exact implicit widening memory multiply emitted by the x86
/// lifter: `Load virtual; MulU/MulS RDX:RAX,RAX,virtual`. Native lowering
/// stages the source in an aligned caller-owned stack frame, lets the memory
/// helper restore the original implicit registers, and only then executes the
/// native group-3 multiply. The load temporary must remain exact SSA.
pub(crate) fn x86_jit_mem_widening_mul_source_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => (*temporary, addr, *width),
        _ => return None,
    };
    let width = mem_width.to_op_width()?;
    if !matches!(
        width,
        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
    ) || !x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let valid = match &consumer.kind {
        OpKind::MulU {
            dst_lo,
            dst_hi,
            src1,
            src2: SrcOperand::Reg(source),
            width: op_width,
            flags,
        }
        | OpKind::MulS {
            dst_lo,
            dst_hi,
            src1,
            src2: SrcOperand::Reg(source),
            width: op_width,
            flags,
        } => {
            *dst_lo == rax
                && *dst_hi
                    == if width == OpWidth::W8 {
                        None
                    } else {
                        Some(rdx)
                    }
                && *src1 == rax
                && *source == temporary
                && *op_width == width
                && matches!(flags, FlagUpdate::None | FlagUpdate::All)
        }
        _ => false,
    };

    valid.then_some(2)
}

/// Validate the exact memory-source `MULX` pair emitted by the VEX/APX lifter:
/// `Load virtual; MulU dst_lo,dst_hi,RDX,virtual [Mulx]`. The helper-backed
/// lowerer keeps the memory value in caller-owned host-stack storage, so the
/// virtual value is never allocated over live guest state and a fault occurs
/// before either destination is committed.
pub(crate) fn x86_jit_mem_mulx_source_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    if !allow_mem {
        return None;
    }
    let load = block.ops.get(index)?;
    let (temporary, addr, mem_width) = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width: mem_width @ (MemWidth::B4 | MemWidth::B8),
            sign: SignExtend::Zero,
        } => (*temporary, addr, *mem_width),
        _ => return None,
    };
    if !x86_jit_mem_address_shape_valid(addr)
        || virtual_definitions.get(&temporary) != Some(&1)
        || virtual_uses.get(&temporary) != Some(&1)
    {
        return None;
    }

    let consumer = block.ops.get(index + 1)?;
    let expected_width = mem_width.to_op_width()?;
    let arch_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());
    let valid = consumer.guest_pc == load.guest_pc
        && matches!(consumer.x86_hint, Some(X86OpHint::Mulx))
        && matches!(
            &consumer.kind,
            OpKind::MulU {
                dst_lo,
                dst_hi: Some(dst_hi),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                src2: SrcOperand::Reg(source),
                width: op_width @ (OpWidth::W32 | OpWidth::W64),
                flags: FlagUpdate::None,
            } if arch_gpr(dst_lo)
                && arch_gpr(dst_hi)
                && *source == temporary
                && *op_width == expected_width
        );

    valid.then_some(2)
}
