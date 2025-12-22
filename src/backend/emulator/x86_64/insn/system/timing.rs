//! Timing instructions: RDTSC, RDTSCP.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// RDTSC - Read Time-Stamp Counter (0x0F 0x31)
pub fn rdtsc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Return a simulated timestamp (just increment monotonically)
    use std::sync::atomic::{AtomicU64, Ordering};
    static TSC: AtomicU64 = AtomicU64::new(0);
    let tsc = TSC.fetch_add(1000, Ordering::Relaxed);
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = tsc >> 32;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDTSCP - Read Time-Stamp Counter and Processor ID (0x0F 0x01 0xF9)
pub fn rdtscp(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Similar to RDTSC but also sets ECX to processor ID
    use std::sync::atomic::{AtomicU64, Ordering};
    static TSC: AtomicU64 = AtomicU64::new(0);
    let tsc = TSC.fetch_add(1000, Ordering::Relaxed);
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = tsc >> 32;
    vcpu.regs.rcx = 0; // Processor ID = 0
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
