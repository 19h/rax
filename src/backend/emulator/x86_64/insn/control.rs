//! Control flow instructions: JMP, CALL, RET, Jcc, SETcc.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};

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

/// SETcc r/m8 (0x0F 0x90-0x9F)
pub fn setcc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext, cc: u8) -> Result<Option<VcpuExit>> {
    let (_, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let value = if vcpu.check_condition(cc) { 1u8 } else { 0u8 };

    if is_memory {
        vcpu.mmu.write_u8(addr, value, &vcpu.sregs)?;
    } else {
        vcpu.set_reg(rm, value as u64, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 5: INC/DEC/CALL/JMP/PUSH (0xFF)
pub fn group5(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    use super::super::flags;

    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let op_size = ctx.op_size;

    match op {
        0 => {
            // INC r/m
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, op_size);
                let result = val.wrapping_add(1);
                vcpu.set_reg(rm, result, op_size);
                flags::update_flags_add(&mut vcpu.regs.rflags, val, 1, result, op_size);
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.read_mem(addr, op_size)?;
                let result = val.wrapping_add(1);
                vcpu.write_mem(addr, result, op_size)?;
                flags::update_flags_add(&mut vcpu.regs.rflags, val, 1, result, op_size);
            }
            vcpu.regs.rip += ctx.cursor as u64;
        }
        1 => {
            // DEC r/m
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, op_size);
                let result = val.wrapping_sub(1);
                vcpu.set_reg(rm, result, op_size);
                flags::update_flags_sub(&mut vcpu.regs.rflags, val, 1, result, op_size);
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.read_mem(addr, op_size)?;
                let result = val.wrapping_sub(1);
                vcpu.write_mem(addr, result, op_size)?;
                flags::update_flags_sub(&mut vcpu.regs.rflags, val, 1, result, op_size);
            }
            vcpu.regs.rip += ctx.cursor as u64;
        }
        2 => {
            // CALL r/m64
            let target = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, 8)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, 8)?
            };
            let ret_addr = vcpu.regs.rip + ctx.cursor as u64;
            vcpu.push64(ret_addr)?;
            vcpu.regs.rip = target;
        }
        4 => {
            // JMP r/m64
            let target = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, 8)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, 8)?
            };
            vcpu.regs.rip = target;
        }
        6 => {
            // PUSH r/m64
            let val = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, 8)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, 8)?
            };
            vcpu.push64(val)?;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        _ => {
            return Err(Error::Emulator(format!("unimplemented 0xFF /{}", op)));
        }
    }
    Ok(None)
}

/// CMOVcc r, r/m (0x0F 0x40-0x4F) - Conditional Move
pub fn cmovcc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext, cc: u8) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Only perform the move if the condition is true
    if vcpu.check_condition(cc) {
        let value = if is_memory {
            vcpu.read_mem(addr, op_size)?
        } else {
            vcpu.get_reg(rm, op_size)
        };
        vcpu.set_reg(reg, value, op_size);
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
