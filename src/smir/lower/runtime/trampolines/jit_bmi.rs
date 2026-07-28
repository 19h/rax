//! Exact helper-backed x86 scalar BMI memory-source sequence validation.

use std::collections::HashMap;

use super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, SrcOperand, VReg};

/// Validate the exact memory-source pairs emitted by the VEX/APX scalar BMI
/// lifters:
///
/// ```text
/// Load virtual, address, B4/B8, zero
/// ANDN/BLS*/BZHI/BEXTR/PDEP/PEXT/RORX architectural operands, virtual, W32/W64
/// ```
///
/// The x86 helper-backed lowerer stages the loaded scalar in caller-owned host
/// stack storage, executes the consumer entirely against scratch registers and
/// the canonical `GuestRegs` snapshot, and commits the architectural
/// destination only after the complete load succeeds. The virtual load result
/// must therefore remain exact single-definition, single-use SSA.
pub(crate) fn x86_jit_mem_bmi_source_sequence_len(
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
    if consumer.guest_pc != load.guest_pc || consumer.x86_hint.is_some() {
        return None;
    }

    let expected_width = mem_width.to_op_width()?;
    let arch_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());
    let exact_flags = |flags: &FlagUpdate, defined: FlagSet| {
        *flags == FlagUpdate::None || *flags == FlagUpdate::Specific(defined)
    };
    let cf_zf_sf_of = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    let cf_zf_of = FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF);

    let valid = match &consumer.kind {
        OpKind::AndNot {
            dst,
            src1,
            src2: SrcOperand::Reg(src2),
            width: width @ (OpWidth::W32 | OpWidth::W64),
            flags,
        } => {
            arch_gpr(dst)
                && *src1 == temporary
                && arch_gpr(src2)
                && *width == expected_width
                && exact_flags(flags, cf_zf_sf_of)
        }
        OpKind::X86Bls {
            dst,
            src,
            width: width @ (OpWidth::W32 | OpWidth::W64),
            flags,
            ..
        } => {
            arch_gpr(dst)
                && *src == temporary
                && *width == expected_width
                && exact_flags(flags, cf_zf_sf_of)
        }
        OpKind::Bzhi {
            dst,
            src,
            index,
            width: width @ (OpWidth::W32 | OpWidth::W64),
            flags,
        } => {
            arch_gpr(dst)
                && *src == temporary
                && arch_gpr(index)
                && *width == expected_width
                && exact_flags(flags, cf_zf_sf_of)
        }
        OpKind::Bextr {
            dst,
            src,
            control,
            width: width @ (OpWidth::W32 | OpWidth::W64),
            flags,
        } => {
            arch_gpr(dst)
                && *src == temporary
                && arch_gpr(control)
                && *width == expected_width
                && exact_flags(flags, cf_zf_of)
        }
        OpKind::Pdep {
            dst,
            src,
            mask,
            width: width @ (OpWidth::W32 | OpWidth::W64),
        }
        | OpKind::Pext {
            dst,
            src,
            mask,
            width: width @ (OpWidth::W32 | OpWidth::W64),
        } => arch_gpr(dst) && arch_gpr(src) && *mask == temporary && *width == expected_width,
        OpKind::Ror {
            dst,
            src,
            amount: SrcOperand::Imm(amount),
            width: width @ (OpWidth::W32 | OpWidth::W64),
            flags: FlagUpdate::None,
        } => {
            arch_gpr(dst)
                && *src == temporary
                && u8::try_from(*amount).is_ok()
                && *width == expected_width
        }
        _ => false,
    };

    valid.then_some(2)
}
