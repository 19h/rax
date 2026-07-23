//! Exact native-admission shape for legacy high-byte MOVRS loads.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, SrcOperand, VReg};
use crate::smir::ir::{SmirBlock, X86InstructionBytes};
use std::collections::HashMap;

/// Validate the fault-precise sequence emitted for `MOVRS AH/CH/DH/BH,m8`:
///
/// `Load byte; And 0xFF; Shl 8; And parent,!0xFF00; Or parent`.
///
/// The native lowerer fuses the complete sequence so none of its SSA
/// temporaries enter the identity-mapped guest GPR namespace.
pub(crate) fn x86_jit_movrs_high_byte_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &HashMap<VReg, usize>,
    virtual_uses: &HashMap<VReg, usize>,
) -> Option<usize> {
    if !allow_mem {
        return None;
    }
    let [load, mask_byte, shift_byte, preserve_parent, merge] = block.ops.get(index..index + 5)?
    else {
        return None;
    };
    if [mask_byte, shift_byte, preserve_parent, merge]
        .iter()
        .any(|op| op.guest_pc != load.guest_pc || op.x86_hint.is_some())
        || load.x86_hint.is_some()
    {
        return None;
    }

    let loaded = match &load.kind {
        OpKind::Load {
            dst: loaded @ VReg::Virtual(_),
            addr,
            width: MemWidth::B1,
            sign: SignExtend::Zero,
        } if super::x86_jit_mem_address_shape_valid(addr) => *loaded,
        _ => return None,
    };
    let masked = match mask_byte.kind {
        OpKind::And {
            dst: masked @ VReg::Virtual(_),
            src1,
            src2: SrcOperand::Imm(0xFF),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src1 == loaded => masked,
        _ => return None,
    };
    let shifted = match shift_byte.kind {
        OpKind::Shl {
            dst: shifted @ VReg::Virtual(_),
            src,
            amount: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if src == masked => shifted,
        _ => return None,
    };
    let (preserved, parent) = match preserve_parent.kind {
        OpKind::And {
            dst: preserved @ VReg::Virtual(_),
            src1: parent @ VReg::Arch(ArchReg::X86(reg)),
            src2: SrcOperand::Imm(mask),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if reg.gpr_index().is_some_and(|index| index <= 3) && mask == !0xFF00_u64 as i64 => {
            (preserved, parent)
        }
        _ => return None,
    };
    if !matches!(
        merge.kind,
        OpKind::Or {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if dst == parent && src1 == preserved && src2 == shifted
    ) {
        return None;
    }

    for temporary in [loaded, masked, shifted, preserved] {
        if virtual_definitions.get(&temporary) != Some(&1)
            || virtual_uses.get(&temporary) != Some(&1)
        {
            return None;
        }
    }
    Some(5)
}

/// Validate a single MOVRS load whose architectural destination is guest RSP
/// or RBP. These registers live only in `GuestRegs` while native code uses the
/// host stack and frame pointers; helper lowering commits directly to that
/// state slot after a successful load.
pub(crate) fn x86_jit_movrs_state_backed_load_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    instruction: Option<&X86InstructionBytes>,
) -> Option<usize> {
    if !allow_mem || !instruction.is_some_and(x86_instruction_is_movrs) {
        return None;
    }
    let load = block.ops.get(index)?;
    matches!(
        &load.kind,
        OpKind::Load {
            dst: VReg::Arch(ArchReg::X86(reg)),
            addr,
            width: MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8,
            sign: SignExtend::Zero,
        } if matches!(reg.gpr_index(), Some(4) | Some(5))
            && load.x86_hint.is_none()
            && super::x86_jit_mem_address_shape_valid(addr)
    )
    .then_some(1)
}

/// Recognize MOVRS opcode provenance captured by the x86 lifter. Operand and
/// reserved-field legality is supplied by the semantic load shape: invalid
/// encodings lift to terminal `#UD` and therefore cannot reach this helper.
fn x86_instruction_is_movrs(instruction: &X86InstructionBytes) -> bool {
    let bytes = instruction.as_slice();
    let mut cursor = 0;
    while bytes.get(cursor).is_some_and(|byte| {
        matches!(
            byte,
            0x26 | 0x2E
                | 0x36
                | 0x3E
                | 0x40..=0x4F
                | 0x64..=0x67
                | 0xF0
                | 0xF2
                | 0xF3
        )
    }) {
        cursor += 1;
    }

    matches!(bytes.get(cursor..), Some([0x0F, 0x38, 0x8A | 0x8B, _, ..]))
        || matches!(
            bytes.get(cursor..),
            Some([0x62, p0, _, _, 0x8A | 0x8B, _, ..]) if p0 & 0x07 == 4
        )
}
