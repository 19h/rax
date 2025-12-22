//! Compare string instructions: CMPSB, CMPSW, CMPSD, CMPSQ.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

/// CMPSB (0xA6)
pub fn cmpsb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val1 = vcpu.mmu.read_u8(vcpu.regs.rsi, &vcpu.sregs)? as u64;
        let val2 = vcpu.mmu.read_u8(vcpu.regs.rdi, &vcpu.sregs)? as u64;
        let result = val1.wrapping_sub(val2);
        flags::update_flags_sub(&mut vcpu.regs.rflags, val1, val2, result, 1);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
            let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;
            if ctx.rep_prefix == Some(0xF3) && !zf {
                break;
            }
            if ctx.rep_prefix == Some(0xF2) && zf {
                break;
            }
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMPSW/CMPSD/CMPSQ (0xA7)
pub fn cmps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
        let val1 = vcpu.read_mem(vcpu.regs.rsi, op_size)?;
        let val2 = vcpu.read_mem(vcpu.regs.rdi, op_size)?;
        let result = val1.wrapping_sub(val2);
        flags::update_flags_sub(&mut vcpu.regs.rflags, val1, val2, result, op_size);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(delta);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
            let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;
            if ctx.rep_prefix == Some(0xF3) && !zf {
                break;
            }
            if ctx.rep_prefix == Some(0xF2) && zf {
                break;
            }
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
