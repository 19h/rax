//! Interrupt instructions: INT, INT3.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// INT3 (0xCC) - Debug breakpoint interrupt
pub fn int3(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rip += ctx.cursor as u64;
    // Return breakpoint exit to caller
    Ok(Some(VcpuExit::Debug))
}

/// INT imm8 (0xCD) - Software interrupt
pub fn int_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let vector = ctx.consume_u8()?;
    vcpu.regs.rip += ctx.cursor as u64;
    // For now, just return a special exit for INT instructions
    // Real implementation would need IDT lookup and privilege checks
    Ok(Some(VcpuExit::Exception(vector)))
}
