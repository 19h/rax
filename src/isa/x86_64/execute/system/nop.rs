//! NOP-like instructions: ENDBR, multi-byte NOP.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

/// ENDBR64/ENDBR32 and 0F 1E hint forms - treat as NOP.
pub fn endbr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    // F3 0F 1E /1 with mod=11 is RDSSPD/RDSSPQ. Intel specifies it as a
    // destination-preserving NOP when CET shadow stacks are not supported or
    // enabled. This emulator does not enumerate CET_SS, so it follows that
    // unsupported-feature behavior rather than injecting #UD.
    if modrm >> 6 != 3 {
        let (_, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ModR/M-consuming NOP and reserved-NOP forms (0F 19/1A/1B/1F).
pub fn nop_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    // Skip any additional bytes for memory operand
    if modrm >> 6 != 3 {
        let (_, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
