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

/// RETF imm16 - far return with stack pop (0xCA)
pub fn retf_imm16(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u16()?;
    let ret_addr = vcpu.pop64()?;
    let cs = vcpu.pop64()? as u16;
    vcpu.regs.rsp = vcpu.regs.rsp.wrapping_add(imm as u64);
    vcpu.regs.rip = ret_addr;
    vcpu.set_sreg(1, cs); // CS is segment register 1
    Ok(None)
}

/// CALL FAR ptr16:16/ptr16:32 (0x9A)
/// Far call with immediate pointer - pushes CS:IP then jumps
/// Note: This opcode is invalid in 64-bit mode but we emulate it for compatibility.
/// Uses inverted operand size: 16-bit default, 66h prefix gives 32-bit.
pub fn call_far_ptr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // 0x9A is a legacy instruction - use inverted operand size logic
    let offset = if ctx.operand_size_override {
        ctx.consume_u32()? as u64
    } else {
        ctx.consume_u16()? as u64
    };
    let selector = ctx.consume_u16()?;

    // Push return CS:IP
    let old_cs = vcpu.get_sreg(1);
    let ret_addr = vcpu.regs.rip + ctx.cursor as u64;

    // Push 64-bit values for proper stack alignment
    vcpu.push64(old_cs as u64)?;
    vcpu.push64(ret_addr)?;

    // Load new CS:IP
    vcpu.set_sreg(1, selector); // CS is segment register index 1
    vcpu.regs.rip = offset;
    Ok(None)
}

/// CALL FAR m16:16/m16:32/m16:64 (0xFF /3)
/// Far call with memory indirect - pushes CS:IP then jumps to address from memory
/// Uses inverted operand size for legacy compatibility:
/// - Without prefix: 16-bit offset
/// - With 66h prefix: 32-bit offset
/// - With REX.W: 64-bit offset
pub fn call_far_mem(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let _modrm = ctx.consume_u8()?;

    // Get memory address
    let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;

    // Determine offset size using inverted logic
    let offset_size: u8 = if ctx.rex_w() {
        8
    } else if ctx.operand_size_override {
        4
    } else {
        2
    };

    // Read offset and selector from memory
    let offset = vcpu.read_mem(addr, offset_size)?;
    let selector = vcpu.mmu.read_u16(addr + offset_size as u64, &vcpu.sregs)?;

    // Push return CS:IP
    let old_cs = vcpu.get_sreg(1);
    let ret_addr = vcpu.regs.rip + ctx.cursor as u64;

    vcpu.push64(old_cs as u64)?;
    vcpu.push64(ret_addr)?;

    // Load new CS:IP
    vcpu.set_sreg(1, selector); // CS is segment register index 1
    vcpu.regs.rip = offset;
    Ok(None)
}
