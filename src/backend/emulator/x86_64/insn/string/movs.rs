//! Move string instructions: MOVSB, MOVSW, MOVSD, MOVSQ.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

/// MOVSB (0xA4)
pub fn movsb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    
    // Debug: trace REP MOVSB copying to shell heap area
    let rip = vcpu.regs.rip;
    let rdi_start = vcpu.regs.rdi;
    if ctx.rep_prefix.is_some() && count > 0 && count < 100 && 
       rdi_start >= 0x3f000000 && rdi_start < 0x40000000 {
        eprintln!("[MOVSB] RIP={:#x} RSI={:#x} RDI={:#x} RCX={} copying...", 
                  rip, vcpu.regs.rsi, rdi_start, count);
    }
    
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.mmu.read_u8(vcpu.regs.rsi, &vcpu.sregs)?;
        vcpu.mmu.write_u8(vcpu.regs.rdi, val, &vcpu.sregs)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVSW/MOVSD/MOVSQ (0xA5)
pub fn movs(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let delta = op_size as u64;
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    
    // Debug: trace REP MOVS copying to shell heap area  
    let rip = vcpu.regs.rip;
    let rdi_start = vcpu.regs.rdi;
    if ctx.rep_prefix.is_some() && count > 0 && count < 100 &&
       rdi_start >= 0x30000000 && rdi_start < 0x50000000 {
        eprintln!("[MOVS{}] RIP={:#x} RSI={:#x} RDI={:#x} RCX={}", 
                  if op_size == 8 { "Q" } else if op_size == 4 { "D" } else { "W" },
                  rip, vcpu.regs.rsi, rdi_start, count);
    }
    
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.read_mem(vcpu.regs.rsi, op_size)?;
        vcpu.write_mem(vcpu.regs.rdi, val, op_size)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(delta);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
