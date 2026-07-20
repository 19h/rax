//! Timing instructions: RDTSC, RDTSCP, RDPMC.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

const CR0_PE: u64 = 1;
const CR4_TSD: u64 = 1 << 2;
const RFLAGS_VM: u64 = 1 << 17;

/// RDTSC/RDTSCP are available when protected mode is disabled, CR4.TSD is
/// clear, or execution is at CPL0. Virtual-8086 mode has effective CPL3.
fn timestamp_read_allowed(vcpu: &X86_64Vcpu) -> bool {
    vcpu.sregs.cr0 & CR0_PE == 0
        || vcpu.sregs.cr4 & CR4_TSD == 0
        || (vcpu.regs.rflags & RFLAGS_VM == 0 && vcpu.sregs.cs.selector & 3 == 0)
}

fn check_timestamp_read(vcpu: &mut X86_64Vcpu) -> Result<bool> {
    if timestamp_read_allowed(vcpu) {
        Ok(true)
    } else {
        vcpu.inject_exception(13, Some(0))?;
        Ok(false)
    }
}

/// Performance monitoring counters (PMCs) for RDPMC.
static PMC: [AtomicU64; 8] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// RDTSC - Read Time-Stamp Counter (0x0F 0x31)
/// Reads 64-bit TSC into EDX:EAX. Upper 32 bits of RAX and RDX are cleared.
pub fn rdtsc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !check_timestamp_read(vcpu)? {
        return Ok(None);
    }
    let tsc = vcpu.tsc();

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
    if !check_timestamp_read(vcpu)? {
        return Ok(None);
    }
    let tsc = vcpu.tsc();
    // EDX:EAX = TSC, upper 32 bits cleared
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = (tsc >> 32) & 0xFFFF_FFFF;
    // ECX = IA32_TSC_AUX[31:0], upper 32 bits cleared
    vcpu.regs.rcx = vcpu.tsc_aux as u64;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDPMC - Read Performance Monitoring Counter (0x0F 0x33)
/// Reads the performance counter specified by ECX into EDX:EAX.
/// ECX[31] = 0: counter is IA32_PMCx (general purpose PMC)
/// ECX[31] = 1: counter is IA32_FIXED_CTRx (fixed function PMC)
/// ECX[29] = 1: "fast read mode" (returns only low 32 bits in EAX, EDX=0)
/// Upper 32 bits of RAX and RDX are cleared.
pub fn rdpmc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let counter_sel = vcpu.regs.rcx as u32;
    let fast_read = (counter_sel & (1 << 29)) != 0;
    let counter_idx = (counter_sel & 0x7) as usize; // Use lower 3 bits as counter index

    // Increment the selected PMC to simulate activity
    let pmc_value = PMC[counter_idx].fetch_add(100, Ordering::Relaxed);

    if fast_read {
        // Fast read mode: only return low 32 bits, EDX = 0
        vcpu.regs.rax = pmc_value & 0xFFFF_FFFF;
        vcpu.regs.rdx = 0;
    } else {
        // Normal mode: return full 64-bit value in EDX:EAX
        vcpu.regs.rax = pmc_value & 0xFFFF_FFFF;
        vcpu.regs.rdx = (pmc_value >> 32) & 0xFFFF_FFFF;
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
