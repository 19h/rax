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
