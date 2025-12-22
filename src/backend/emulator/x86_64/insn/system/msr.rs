//! MSR instructions: RDMSR, WRMSR.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// WRMSR (0x0F 0x30)
pub fn wrmsr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let ecx = vcpu.regs.rcx as u32;
    let value = ((vcpu.regs.rdx & 0xFFFF_FFFF) << 32) | (vcpu.regs.rax & 0xFFFF_FFFF);

    match ecx {
        0xC0000080 => vcpu.sregs.efer = value, // EFER
        0xC0000100 => vcpu.sregs.fs.base = value, // FS.base
        0xC0000101 => vcpu.sregs.gs.base = value, // GS.base
        0xC0000102 => {}                       // KernelGSbase - ignore
        _ => {}                                // Ignore unknown MSRs
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
        0xC0000080 => vcpu.sregs.efer, // EFER
        0xC0000100 => vcpu.sregs.fs.base, // FS.base
        0xC0000101 => vcpu.sregs.gs.base, // GS.base
        0xC0000102 => 0,               // KernelGSbase
        _ => 0,                        // Return 0 for unknown MSRs
    };

    vcpu.regs.rax = (value & 0xFFFF_FFFF) as u64;
    vcpu.regs.rdx = (value >> 32) as u64;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
