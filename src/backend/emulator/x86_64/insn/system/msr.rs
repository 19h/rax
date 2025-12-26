//! MSR instructions: RDMSR, WRMSR.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// WRMSR (0x0F 0x30)
pub fn wrmsr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let ecx = vcpu.regs.rcx as u32;
    let value = ((vcpu.regs.rdx & 0xFFFF_FFFF) << 32) | (vcpu.regs.rax & 0xFFFF_FFFF);

    match ecx {
        0xC0000080 => vcpu.sregs.efer = value,       // EFER
        0xC0000081 => vcpu.sregs.star = value,       // STAR
        0xC0000082 => vcpu.sregs.lstar = value,      // LSTAR
        0xC0000083 => vcpu.sregs.cstar = value,      // CSTAR
        0xC0000084 => vcpu.sregs.fmask = value,      // FMASK
        0x174 => vcpu.sregs.sysenter_cs = value,     // IA32_SYSENTER_CS
        0x175 => vcpu.sregs.sysenter_esp = value,    // IA32_SYSENTER_ESP
        0x176 => vcpu.sregs.sysenter_eip = value,    // IA32_SYSENTER_EIP
        0xC0000100 => vcpu.sregs.fs.base = value,    // FS.base
        0xC0000101 => {
            // Debug: trace GS.base writes
            use std::sync::atomic::{AtomicU64, Ordering};
            static GS_BASE_WRITE_COUNT: AtomicU64 = AtomicU64::new(0);
            let count = GS_BASE_WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
            if count < 10 {
                eprintln!("[WRMSR GS_BASE #{}] value={:#x} (from RIP={:#x})", count, value, vcpu.regs.rip);
            }
            vcpu.sregs.gs.base = value;
        }
        0xC0000102 => {
            // Debug: trace KERNEL_GS_BASE writes
            use std::sync::atomic::{AtomicU64, Ordering};
            static KGS_BASE_WRITE_COUNT: AtomicU64 = AtomicU64::new(0);
            let count = KGS_BASE_WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
            if count < 10 {
                eprintln!("[WRMSR KERNEL_GS_BASE #{}] value={:#x}", count, value);
            }
            vcpu.kernel_gs_base = value;
        }
        _ => {}                                       // Ignore unknown MSRs
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDMSR (0x0F 0x32)
pub fn rdmsr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let ecx = vcpu.regs.rcx as u32;

    let value = match ecx {
        0x10 => {
            // TSC - Time Stamp Counter (return a non-zero simulated value)
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1_000_000)
        }
        0x1B => {
            // IA32_APIC_BASE - APIC base address
            // Bit 8: BSP flag (this is the bootstrap processor)
            // Bit 11: APIC global enable
            // Bits 12-35: APIC base physical address (default 0xFEE00000)
            (1u64 << 8) | (1u64 << 11) | 0xFEE00000u64
        }
        0xC0000080 => vcpu.sregs.efer,        // EFER
        0xC0000081 => vcpu.sregs.star,        // STAR
        0xC0000082 => vcpu.sregs.lstar,       // LSTAR
        0xC0000083 => vcpu.sregs.cstar,       // CSTAR
        0xC0000084 => vcpu.sregs.fmask,       // FMASK
        0x174 => vcpu.sregs.sysenter_cs,      // IA32_SYSENTER_CS
        0x175 => vcpu.sregs.sysenter_esp,     // IA32_SYSENTER_ESP
        0x176 => vcpu.sregs.sysenter_eip,     // IA32_SYSENTER_EIP
        0xC0000100 => vcpu.sregs.fs.base,     // FS.base
        0xC0000101 => vcpu.sregs.gs.base,     // GS.base
        0xC0000102 => vcpu.kernel_gs_base,    // KernelGSbase
        _ => 0,                               // Return 0 for unknown MSRs
    };

    vcpu.regs.rax = (value & 0xFFFF_FFFF) as u64;
    vcpu.regs.rdx = (value >> 32) as u64;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
