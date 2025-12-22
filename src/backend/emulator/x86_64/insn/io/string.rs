//! String I/O instructions: INSB, INSW, OUTSB, OUTSW.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// INSB (0x6C) - Input byte from port DX into ES:[RDI]
pub fn insb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    let df = (vcpu.regs.rflags & 0x400) != 0;

    // Handle REP prefix
    if ctx.rep_prefix == Some(0xF3) || ctx.rep_prefix == Some(0xF2) {
        let count = vcpu.regs.rcx;
        for _ in 0..count {
            // Write 0 (simulated I/O input)
            vcpu.write_mem(vcpu.regs.rdi, 0u64, 1)?;
            if df {
                vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
            } else {
                vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
            }
        }
        vcpu.regs.rcx = 0;
    } else {
        // Single iteration
        vcpu.write_mem(vcpu.regs.rdi, 0u64, 1)?;
        if df {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoIn { port, size: 1 }))
}

/// INSW/INSD (0x6D) - Input word/dword from port DX into ES:[RDI]
pub fn insw(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    let size: u8 = if ctx.operand_size_override { 2 } else { 4 };
    let df = (vcpu.regs.rflags & 0x400) != 0;

    // Handle REP prefix
    if ctx.rep_prefix == Some(0xF3) || ctx.rep_prefix == Some(0xF2) {
        let count = vcpu.regs.rcx;
        for _ in 0..count {
            vcpu.write_mem(vcpu.regs.rdi, 0u64, size)?;
            if df {
                vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(size as u64);
            } else {
                vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(size as u64);
            }
        }
        vcpu.regs.rcx = 0;
    } else {
        vcpu.write_mem(vcpu.regs.rdi, 0u64, size)?;
        if df {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(size as u64);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(size as u64);
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoIn { port, size }))
}

/// OUTSB (0x6E) - Output byte from DS:[RSI] to port DX
pub fn outsb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    let df = (vcpu.regs.rflags & 0x400) != 0;
    let mut data = Vec::new();

    // Handle REP prefix
    if ctx.rep_prefix == Some(0xF3) || ctx.rep_prefix == Some(0xF2) {
        let count = vcpu.regs.rcx;
        for _ in 0..count {
            let byte = vcpu.read_mem(vcpu.regs.rsi, 1)? as u8;
            data.push(byte);
            if df {
                vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(1);
            } else {
                vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(1);
            }
        }
        vcpu.regs.rcx = 0;
    } else {
        let byte = vcpu.read_mem(vcpu.regs.rsi, 1)? as u8;
        data.push(byte);
        if df {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(1);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(1);
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoOut { port, data }))
}

/// OUTSW/OUTSD (0x6F) - Output word/dword from DS:[RSI] to port DX
pub fn outsw(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    let size: u8 = if ctx.operand_size_override { 2 } else { 4 };
    let df = (vcpu.regs.rflags & 0x400) != 0;
    let mut data = Vec::new();

    // Handle REP prefix
    if ctx.rep_prefix == Some(0xF3) || ctx.rep_prefix == Some(0xF2) {
        let count = vcpu.regs.rcx;
        for _ in 0..count {
            let val = vcpu.read_mem(vcpu.regs.rsi, size)?;
            for i in 0..size {
                data.push((val >> (i * 8)) as u8);
            }
            if df {
                vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(size as u64);
            } else {
                vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(size as u64);
            }
        }
        vcpu.regs.rcx = 0;
    } else {
        let val = vcpu.read_mem(vcpu.regs.rsi, size)?;
        for i in 0..size {
            data.push((val >> (i * 8)) as u8);
        }
        if df {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(size as u64);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(size as u64);
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoOut { port, data }))
}
