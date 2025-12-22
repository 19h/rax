//! Store string instructions: STOSB, STOSW, STOSD, STOSQ.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

/// STOSB (0xAA)
pub fn stosb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        vcpu.mmu
            .write_u8(vcpu.regs.rdi, vcpu.regs.rax as u8, &vcpu.sregs)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STOSW/STOSD/STOSQ (0xAB)
pub fn stos(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
        vcpu.write_mem(vcpu.regs.rdi, vcpu.regs.rax, op_size)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(delta);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
