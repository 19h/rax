//! Timing instructions: RDTSC, RDTSCP, RDPMC.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::timing;

/// Performance monitoring counters (PMCs) for RDPMC
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

/// Get TSC value based on wall-clock time (scaled to emulated CPU frequency)
#[inline]
fn get_tsc() -> u64 {
    timing::tsc()
}

/// RDTSC - Read Time-Stamp Counter (0x0F 0x31)
/// Reads 64-bit TSC into EDX:EAX. Upper 32 bits of RAX and RDX are cleared.
pub fn rdtsc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static RDTSC_COUNT: AtomicU64 = AtomicU64::new(0);

    let tsc = get_tsc();
    let count = RDTSC_COUNT.fetch_add(1, Ordering::Relaxed);

    // Debug: print R8 (target) value when in delay_tsc
    let rip = vcpu.regs.rip;
    static IN_DELAY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if rip >= 0xffffffff8224ea00 && rip <= 0xffffffff8224eaff {
        let delay_count = IN_DELAY.fetch_add(1, Ordering::Relaxed);
        if delay_count < 5 || delay_count % 1_000_000 == 0 {
            let r8 = vcpu.regs.r8;
            let r9 = vcpu.regs.r9 as u32;  // Saved cpu_number
            let rsi = vcpu.regs.rsi as u32; // Current cpu_number (after mov)
            let rdi = vcpu.regs.rdi;
            let elapsed = tsc.saturating_sub(rdi);
            let gs_base = vcpu.sregs.gs.base;
            eprintln!("[RDTSC #{} delay_tsc] elapsed={}, target={}, r9(saved_cpu)={}, rsi(cur_cpu)={}, gs.base={:#x}",
                delay_count, elapsed, r8, r9, rsi, gs_base);
        }
    }

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
    let tsc = get_tsc();
    // EDX:EAX = TSC, upper 32 bits cleared
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = (tsc >> 32) & 0xFFFF_FFFF;
    // ECX = IA32_TSC_AUX[31:0] (processor ID), upper 32 bits cleared
    vcpu.regs.rcx = 0; // Processor ID = 0
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
