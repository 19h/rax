//! INVPCID decode-independent validation and direct execution.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

use super::control_regs::{is_cpl0, raise_gp0};

const CR4_PCIDE: u64 = 1 << 17;

/// The architecturally consumed fields of one 128-bit INVPCID descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86InvpcidDescriptor {
    pub pcid: u16,
    pub linear: u64,
}

/// Validate an INVPCID type and descriptor after the complete memory operand
/// has been read. Returning `Err(())` denotes the instruction's #GP(0)
/// conditions; no translation state has been changed.
pub(crate) fn validate_x86_invpcid(
    invpcid_type: u64,
    descriptor_low: u64,
    descriptor_linear: u64,
    cr4: u64,
) -> core::result::Result<X86InvpcidDescriptor, ()> {
    let pcid = (descriptor_low & 0x0FFF) as u16;
    let reserved = descriptor_low & !0x0FFF;
    if invpcid_type > 3
        || reserved != 0
        || (cr4 & CR4_PCIDE == 0 && invpcid_type <= 1 && pcid != 0)
        || (invpcid_type == 0 && !super::is_canonical_48(descriptor_linear))
    {
        return Err(());
    }
    Ok(X86InvpcidDescriptor {
        pcid,
        linear: descriptor_linear,
    })
}

/// Execute legacy `66 0F 38 82 /r` INVPCID.
pub fn invpcid(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override || ctx.rex2.is_some() {
        return vcpu.inject_undefined_instruction();
    }
    invpcid_decoded(vcpu, ctx, 0)
}

/// Execute a validated APX-promoted INVPCID encoding.
pub(crate) fn invpcid_apx(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
) -> Result<Option<VcpuExit>> {
    invpcid_decoded(vcpu, ctx, ctx.evex_dest_reg())
}

fn invpcid_decoded(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    reg_extension: u8,
) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.peek_u8()?;
    if modrm >> 6 == 3 {
        return vcpu.inject_undefined_instruction();
    }

    // Privilege faults precede effective-address and descriptor accesses.
    if !is_cpl0(vcpu) {
        return raise_gp0(vcpu);
    }

    let reg = ((modrm >> 3) & 7) | ctx.any_rex_r() | reg_extension;
    let (addr, extra, stack_segment) =
        vcpu.decode_modrm_addr_with_stack_segment(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;

    // In 64-bit mode every byte of the 16-byte source must be canonical.
    // Select #SS(0) only when the effective segment is SS.
    if vcpu.sregs.cs.l
        && !addr
            .checked_add(15)
            .is_some_and(|last| super::is_canonical_48(addr) && super::is_canonical_48(last))
    {
        vcpu.inject_exception(if stack_segment { 12 } else { 13 }, Some(0))?;
        return Ok(None);
    }

    let invpcid_type = vcpu.get_reg(reg, if vcpu.sregs.cs.l { 8 } else { 4 });
    let (descriptor_low, descriptor_linear) = vcpu.read_invpcid_descriptor(addr)?;
    let descriptor = match validate_x86_invpcid(
        invpcid_type,
        descriptor_low,
        descriptor_linear,
        vcpu.sregs.cr4,
    ) {
        Ok(descriptor) => descriptor,
        Err(()) => return raise_gp0(vcpu),
    };

    // RAX's direct-mapped TLB cannot retain PCID/global distinctions. Intel
    // permits every INVPCID type to invalidate additional mappings, so a full
    // translation-dependent cache flush is a conservative implementation.
    vcpu.invalidate_process_context(invpcid_type, descriptor.pcid, descriptor.linear);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
