//! MOV instructions (GPR data movement).

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// MOV r8, imm8 (0xB0-0xB7)
pub fn mov_r8_imm8(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
) -> Result<Option<VcpuExit>> {
    let reg = (opcode - 0xB0) | ctx.rex_b();
    let imm = ctx.consume_u8()?;
    let has_rex = ctx.rex.is_some();
    vcpu.set_reg8(reg, imm as u64, has_rex);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r16/32/64, imm16/32/64 (0xB8-0xBF)
pub fn mov_r_imm(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
) -> Result<Option<VcpuExit>> {
    let reg = (opcode - 0xB8) | ctx.rex_b();
    let imm = ctx.consume_imm(ctx.op_size)?;
    vcpu.set_reg(reg, imm, ctx.op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r/m8, r8 (0x88)
pub fn mov_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let value = vcpu.get_reg(reg, 1);

    if is_memory {
        vcpu.mmu.write_u8(addr, value as u8, &vcpu.sregs)?;
    } else {
        vcpu.set_reg(rm, value, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r/m, r (0x89)
pub fn mov_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let value = vcpu.get_reg(reg, op_size);

    if is_memory {
        vcpu.write_mem(addr, value, op_size)?;
    } else {
        vcpu.set_reg(rm, value, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r8, r/m8 (0x8A)
pub fn mov_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    vcpu.set_reg(reg, value, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r, r/m (0x8B)
pub fn mov_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    vcpu.set_reg(reg, value, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r/m, Sreg (0x8C)
pub fn mov_rm_sreg(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (sreg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let value = vcpu.get_sreg(sreg);

    if is_memory {
        vcpu.mmu.write_u16(addr, value, &vcpu.sregs)?;
    } else {
        vcpu.set_reg(rm, value as u64, 2);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV Sreg, r/m16 (0x8E)
pub fn mov_sreg_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (sreg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u16(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 2) as u16
    };
    vcpu.set_sreg(sreg, value);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r/m8, imm8 (0xC6 /0) or XABORT (0xC6 F8 imm8)
pub fn mov_rm8_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Check for XABORT (C6 F8 imm8) - ModRM F8 has reg=7
    let modrm = ctx.peek_u8()?;
    let reg = (modrm >> 3) & 0x07;

    if reg == 7 {
        // XABORT - abort transaction with status
        ctx.consume_u8()?; // consume ModRM
        let _status = ctx.consume_u8()?; // status code
        // TSX not supported - XABORT has no effect outside transaction
        // In a real transaction, this would jump to fallback
        vcpu.regs.rip += ctx.cursor as u64;
        return Ok(None);
    }

    ctx.rip_relative_offset = 1;
    let (_, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let imm = ctx.consume_u8()?;

    if is_memory {
        vcpu.mmu.write_u8(addr, imm, &vcpu.sregs)?;
    } else {
        vcpu.set_reg(rm, imm as u64, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r/m, imm (0xC7 /0) or XBEGIN (0xC7 F8 rel32)
pub fn mov_rm_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Check for XBEGIN (C7 F8 rel32) - ModRM F8 has mod=11, reg=7, r/m=0
    let modrm = ctx.peek_u8()?;
    let reg = (modrm >> 3) & 0x07;

    if reg == 7 {
        // XBEGIN - begin transaction
        ctx.consume_u8()?; // consume ModRM
        let rel32 = ctx.consume_u32()? as i32;
        // TSX not supported - always abort immediately (this is valid behavior)
        // Set EAX to abort status code: we use 0 (capacity abort, retry may help)
        vcpu.regs.rax = 0;
        // Jump to fallback address
        let fallback = (vcpu.regs.rip as i64)
            .wrapping_add(ctx.cursor as i64)
            .wrapping_add(rel32 as i64) as u64;
        vcpu.regs.rip = fallback;
        return Ok(None);
    }

    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    ctx.rip_relative_offset = imm_size as usize;
    let (_, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };

    if is_memory {
        vcpu.write_mem(addr, imm, op_size)?;
    } else {
        vcpu.set_reg(rm, imm, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
