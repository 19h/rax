//! Timing instructions: RDTSC, RDTSCP.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// Shared time-stamp counter for RDTSC and RDTSCP
static TSC: AtomicU64 = AtomicU64::new(0);

/// RDTSC - Read Time-Stamp Counter (0x0F 0x31)
/// Reads 64-bit TSC into EDX:EAX. Upper 32 bits of RAX and RDX are cleared.
pub fn rdtsc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let tsc = TSC.fetch_add(1000, Ordering::Relaxed);
    // EDX:EAX = TSC, upper 32 bits of RAX and RDX are cleared
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = (tsc >> 32) & 0xFFFF_FFFF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDTSCP - Read Time-Stamp Counter and Processor ID (0x0F 0x01 0xF9)
/// Reads 64-bit TSC into EDX:EAX, and IA32_TSC_AUX[31:0] into ECX.
/// Upper 32 bits of RAX, RDX, and RCX are cleared.
pub fn rdtscp(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let tsc = TSC.fetch_add(1000, Ordering::Relaxed);
    // EDX:EAX = TSC, upper 32 bits cleared
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = (tsc >> 32) & 0xFFFF_FFFF;
    // ECX = IA32_TSC_AUX[31:0] (processor ID), upper 32 bits cleared
    vcpu.regs.rcx = 0; // Processor ID = 0
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
