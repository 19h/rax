//! Exact helper-backed AMD TBM memory-source sequence validation.

use std::collections::HashMap;

use super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, VReg};

/// Validate either exact memory-source pair emitted by the XOP TBM lifter:
///
/// ```text
/// Load virtual, address, B4/B8, zero
/// X86Tbm architectural-dst, virtual, W32/W64, exact flags
///
/// Load virtual, address, B4/B8, zero
/// Bextr architectural-dst, virtual, imm32-control, W32/W64, exact flags
/// ```
///
/// The helper-backed lowerer stages the loaded scalar in caller-owned native
/// stack storage and commits the full-width architectural destination only
/// after a successful load. The virtual must therefore be exact
/// single-definition/single-use SSA. XOP can address the 16 legacy/REX GPRs;
/// guest RSP/RBP use state-backed destination commit.
pub(crate) fn x86_jit_mem_tbm_source_sequence_len(
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
        } if load.x86_hint.is_none() => (*temporary, addr, *mem_width),
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
    let xop_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(x86))
                if x86.gpr_index().is_some_and(|index| index < 16)
        )
    };
    let tbm_flags = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    let bextr_flags = FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF);
    let valid = consumer.guest_pc == load.guest_pc
        && consumer.x86_hint.is_none()
        && match &consumer.kind {
            OpKind::X86Tbm {
                dst,
                src,
                width: op_width @ (OpWidth::W32 | OpWidth::W64),
                flags,
                ..
            } => {
                xop_gpr(dst)
                    && *src == temporary
                    && *op_width == expected_width
                    && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(tbm_flags))
            }
            OpKind::Bextr {
                dst,
                src,
                control: VReg::Imm(control),
                width: op_width @ (OpWidth::W32 | OpWidth::W64),
                flags,
            } => {
                xop_gpr(dst)
                    && *src == temporary
                    && u32::try_from(*control).is_ok()
                    && *op_width == expected_width
                    && (*flags == FlagUpdate::None || *flags == FlagUpdate::Specific(bextr_flags))
            }
            _ => false,
        };

    valid.then_some(2)
}
