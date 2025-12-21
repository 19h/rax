//! I/O instructions: IN, OUT.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::cpu::{InsnContext, X86_64Vcpu};

/// IN AL, imm8 (0xE4)
pub fn in_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = ctx.consume_u8()? as u16;
    vcpu.regs.rip += ctx.cursor as u64;
    vcpu.set_io_pending(1);
    Ok(Some(VcpuExit::IoIn { port, size: 1 }))
}

/// IN AX/EAX, imm8 (0xE5)
pub fn in_ax_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = ctx.consume_u8()? as u16;
    let size = if ctx.operand_size_override { 2 } else { 4 };
    vcpu.regs.rip += ctx.cursor as u64;
    vcpu.set_io_pending(size);
    Ok(Some(VcpuExit::IoIn { port, size }))
}

/// IN AL, DX (0xEC)
pub fn in_al_dx(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    vcpu.regs.rip += ctx.cursor as u64;
    vcpu.set_io_pending(1);
    Ok(Some(VcpuExit::IoIn { port, size: 1 }))
}

/// IN AX/EAX, DX (0xED)
pub fn in_ax_dx(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    let size = if ctx.operand_size_override { 2 } else { 4 };
    vcpu.regs.rip += ctx.cursor as u64;
    vcpu.set_io_pending(size);
    Ok(Some(VcpuExit::IoIn { port, size }))
}

/// OUT imm8, AL (0xE6)
pub fn out_imm8_al(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = ctx.consume_u8()? as u16;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoOut {
        port,
        data: vec![vcpu.regs.rax as u8],
    }))
}

/// OUT imm8, AX/EAX (0xE7)
pub fn out_imm8_ax(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = ctx.consume_u8()? as u16;
    let data = if ctx.operand_size_override {
        (vcpu.regs.rax as u16).to_le_bytes().to_vec()
    } else {
        (vcpu.regs.rax as u32).to_le_bytes().to_vec()
    };
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoOut { port, data }))
}

/// OUT DX, AL (0xEE)
pub fn out_dx_al(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoOut {
        port,
        data: vec![vcpu.regs.rax as u8],
    }))
}

/// OUT DX, AX/EAX (0xEF)
pub fn out_dx_ax(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let port = vcpu.regs.rdx as u16;
    let data = if ctx.operand_size_override {
        (vcpu.regs.rax as u16).to_le_bytes().to_vec()
    } else {
        (vcpu.regs.rax as u32).to_le_bytes().to_vec()
    };
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(Some(VcpuExit::IoOut { port, data }))
}

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
