//! Cache hint instructions: CLDEMOTE, CLWB.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

/// CLDEMOTE m8 (0F 1C /0) - cache line demote hint (treated as NOP).
pub fn cldemote(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let _ = vcpu.decode_modrm(ctx)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
