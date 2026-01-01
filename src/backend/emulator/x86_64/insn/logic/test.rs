//! TEST instructions.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// TEST r/m8, r8 (0x84)
pub fn test_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let has_rex = ctx.rex.is_some();
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg8(reg, has_rex) as u8;

    let dst = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg8(rm, has_rex) as u8
    };
    let result = (dst & src) as u64;

    vcpu.set_lazy_logic(result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// TEST r/m, r (0x85)
pub fn test_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst & src;

    vcpu.set_lazy_logic(result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// TEST AL, imm8 (0xA8)
pub fn test_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let al = vcpu.regs.rax & 0xFF;
    let result = al & imm;

    vcpu.set_lazy_logic(result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// TEST rAX, imm (0xA9)
pub fn test_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let result = vcpu.get_reg(0, op_size) & imm;
    // Use lazy flags to avoid stale flags when Jcc materializes
    vcpu.set_lazy_logic(result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
