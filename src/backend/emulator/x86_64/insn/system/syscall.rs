//! SYSCALL instruction support.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// SYSCALL (0x0F 0x05)
pub fn syscall(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Minimal behavior: save return address/flags and fall through.
    // Full SYSCALL semantics depend on STAR/LSTAR/FMASK MSRs, which are not modeled yet.
    let next_rip = vcpu.regs.rip + ctx.cursor as u64;
    vcpu.regs.rcx = next_rip;
    vcpu.regs.r11 = vcpu.regs.rflags;
    vcpu.regs.rip = next_rip;
    Ok(None)
}
