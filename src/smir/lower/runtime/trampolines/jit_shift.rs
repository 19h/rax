//! Exact helper-backed x86 scalar shift sequence validation.

use std::collections::HashMap;

use super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, SrcOperand, VReg};

/// Validate the exact memory-source pair emitted by the VEX BMI2
/// `SHLX`/`SHRX`/`SARX` lifter:
///
/// ```text
/// Load virtual, address, B4/B8, zero
/// Shl/Shr/Sar architectural-dst, virtual, architectural-count, W32/W64, NF
/// ```
///
/// The x86 helper-backed lowerer keeps the loaded value in caller-owned host
/// stack storage, executes the variable shift there, and commits the
/// architectural destination only after the complete load succeeds. The
/// virtual load result must therefore remain exact single-definition,
/// single-use SSA. Destination and count are restricted to the 16-register
/// VEX GPR namespace; APX uses a distinct three-operation masked-count shape
/// and remains fail-closed here.
pub(crate) fn x86_jit_mem_bmi2_shift_source_sequence_len(
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
    let vex_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(x86))
                if x86.gpr_index().is_some_and(|index| index < 16)
        )
    };
    let valid = consumer.guest_pc == load.guest_pc
        && consumer.x86_hint.is_none()
        && matches!(
            &consumer.kind,
            OpKind::Shl {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width: op_width @ (OpWidth::W32 | OpWidth::W64),
                flags: FlagUpdate::None,
            }
            | OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width: op_width @ (OpWidth::W32 | OpWidth::W64),
                flags: FlagUpdate::None,
            }
            | OpKind::Sar {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width: op_width @ (OpWidth::W32 | OpWidth::W64),
                flags: FlagUpdate::None,
            } if vex_gpr(dst)
                && *src == temporary
                && vex_gpr(count)
                && *op_width == expected_width
        );

    valid.then_some(2)
}
