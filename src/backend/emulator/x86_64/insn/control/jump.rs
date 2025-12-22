//! Jump instructions: JMP, Jcc.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// JMP rel8 (0xEB)
pub fn jmp_rel8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u8()? as i8 as i64;
    vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    Ok(None)
}

/// JMP rel32 (0xE9)
pub fn jmp_rel32(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u32()? as i32 as i64;
    vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    Ok(None)
}

/// Jcc rel8 (0x70-0x7F)
pub fn jcc_rel8(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    cc: u8,
) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u8()? as i8 as i64;
    if vcpu.check_condition(cc) {
        vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    } else {
        vcpu.regs.rip += ctx.cursor as u64;
    }
    Ok(None)
}

/// Jcc rel32 (0x0F 0x80-0x8F)
pub fn jcc_rel32(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    cc: u8,
) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u32()? as i32 as i64;
    if vcpu.check_condition(cc) {
        vcpu.regs.rip = (vcpu.regs.rip as i64 + ctx.cursor as i64 + disp) as u64;
    } else {
        vcpu.regs.rip += ctx.cursor as u64;
    }
    Ok(None)
}
