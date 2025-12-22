//! Stack-based flag instructions: PUSHF, POPF.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// PUSHF (0x9C)
pub fn pushf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.push64(vcpu.regs.rflags)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// POPF (0x9D)
pub fn popf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let flags_val = vcpu.pop64()?;
    // Preserve reserved bits
    vcpu.regs.rflags = (flags_val & 0x00000000_00257FD5) | 0x2;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
