//! Cache hints and privileged cache maintenance.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

/// CLDEMOTE m8 (0F 1C /0) - cache line demote hint (treated as NOP).
pub fn cldemote(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let _ = vcpu.decode_modrm(ctx)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// INVD/WBINVD/WBNOINVD: the data-cache model has no deferred dirty lines.
/// Retain guest privilege checks and ordering; WBINVD/WBNOINVD serialize.
pub fn cache_maintenance(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !super::control_regs::is_cpl0(vcpu) {
        return super::control_regs::raise_gp0(vcpu);
    }
    std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
    vcpu.regs.rip = vcpu.regs.rip.wrapping_add(ctx.cursor as u64);
    Ok(None)
}
