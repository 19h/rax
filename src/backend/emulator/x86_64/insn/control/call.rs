//! Call and return instructions: CALL, RET, RETF.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// CALL rel32 (0xE8)
pub fn call_rel32(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u32()? as i32 as i64;
    let ret_addr = vcpu.regs.rip + ctx.cursor as u64;
    vcpu.push64(ret_addr)?;
    vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    Ok(None)
}

/// RET (0xC3)
pub fn ret(vcpu: &mut X86_64Vcpu, _ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let ret_addr = vcpu.pop64()?;
    vcpu.regs.rip = ret_addr;
    Ok(None)
}

/// RET imm16 (0xC2)
pub fn ret_imm16(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u16()?;
    let ret_addr = vcpu.pop64()?;
    vcpu.regs.rsp = vcpu.regs.rsp.wrapping_add(imm as u64);
    vcpu.regs.rip = ret_addr;
    Ok(None)
}

/// RETF - far return (0xCB)
pub fn retf(vcpu: &mut X86_64Vcpu, _ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let ret_addr = vcpu.pop64()?;
    let cs = vcpu.pop64()? as u16;
    vcpu.regs.rip = ret_addr;
    vcpu.set_sreg(1, cs); // CS is segment register 1
    Ok(None)
}
