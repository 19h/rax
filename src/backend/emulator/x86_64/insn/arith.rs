//! Arithmetic instructions: ADD, SUB, ADC, SBB, CMP, INC, DEC, NEG, IMUL.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;

/// ADD r/m8, r8 (0x00)
pub fn add_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = dst.wrapping_add(src) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = dst.wrapping_add(src) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADD r/m, r (0x01)
pub fn add_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst.wrapping_add(src);
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst.wrapping_add(src);
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADD r8, r/m8 (0x02)
pub fn add_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = dst.wrapping_add(src) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADD r, r/m (0x03)
pub fn add_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst.wrapping_add(src);
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADD AL, imm8 (0x04)
pub fn add_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let al = vcpu.regs.rax & 0xFF;
    let result = al.wrapping_add(imm);
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (result & 0xFF);
    flags::update_flags_add(&mut vcpu.regs.rflags, al, imm, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADD rAX, imm (0x05)
pub fn add_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let rax = vcpu.get_reg(0, op_size);
    let result = rax.wrapping_add(imm);
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_add(&mut vcpu.regs.rflags, rax, imm, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SUB r/m8, r8 (0x28)
pub fn sub_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = dst.wrapping_sub(src) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = dst.wrapping_sub(src) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SUB r/m, r (0x29)
pub fn sub_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst.wrapping_sub(src);
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst.wrapping_sub(src);
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SUB r8, r/m8 (0x2A)
pub fn sub_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = dst.wrapping_sub(src) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SUB r, r/m (0x2B)
pub fn sub_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst.wrapping_sub(src);
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SUB AL, imm8 (0x2C)
pub fn sub_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let al = vcpu.regs.rax & 0xFF;
    let result = al.wrapping_sub(imm);
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (result & 0xFF);
    flags::update_flags_sub(&mut vcpu.regs.rflags, al, imm, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SUB rAX, imm (0x2D)
pub fn sub_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let rax = vcpu.get_reg(0, op_size);
    let result = rax.wrapping_sub(imm);
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_sub(&mut vcpu.regs.rflags, rax, imm, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r/m8, r8 (0x18) - Subtract with Borrow
pub fn sbb_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);
    let cf = if vcpu.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = dst.wrapping_sub(src).wrapping_sub(cf) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src.wrapping_add(cf), result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = dst.wrapping_sub(src).wrapping_sub(cf) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src.wrapping_add(cf), result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r/m, r (0x19) - Subtract with Borrow
pub fn sbb_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);
    let cf = if vcpu.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst.wrapping_sub(src).wrapping_sub(cf);
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src.wrapping_add(cf), result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst.wrapping_sub(src).wrapping_sub(cf);
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src.wrapping_add(cf), result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r8, r/m8 (0x1A) - Subtract with Borrow
pub fn sbb_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);
    let cf = if vcpu.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = dst.wrapping_sub(src).wrapping_sub(cf) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src.wrapping_add(cf), result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r, r/m (0x1B) - Subtract with Borrow
pub fn sbb_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);
    let cf = if vcpu.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst.wrapping_sub(src).wrapping_sub(cf);
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src.wrapping_add(cf), result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB AL, imm8 (0x1C) - Subtract with Borrow
pub fn sbb_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let al = vcpu.regs.rax & 0xFF;
    let cf = if vcpu.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };
    let result = al.wrapping_sub(imm).wrapping_sub(cf);
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (result & 0xFF);
    flags::update_flags_sub(&mut vcpu.regs.rflags, al, imm.wrapping_add(cf), result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB rAX, imm (0x1D) - Subtract with Borrow
pub fn sbb_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let rax = vcpu.get_reg(0, op_size);
    let cf = if vcpu.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };
    let result = rax.wrapping_sub(imm).wrapping_sub(cf);
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_sub(&mut vcpu.regs.rflags, rax, imm.wrapping_add(cf), result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMP r/m8, r8 (0x38)
pub fn cmp_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1) as u8;

    let dst = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 1) as u8
    };
    let result = dst.wrapping_sub(src) as u64;
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst as u64, src as u64, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMP r/m, r (0x39)
pub fn cmp_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst.wrapping_sub(src);
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMP r8, r/m8 (0x3A)
pub fn cmp_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1) as u8;

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 1) as u8
    };
    let result = dst.wrapping_sub(src) as u64;
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst as u64, src as u64, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMP r, r/m (0x3B)
pub fn cmp_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst.wrapping_sub(src);
    flags::update_flags_sub(&mut vcpu.regs.rflags, dst, src, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMP AL, imm8 (0x3C)
pub fn cmp_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let al = vcpu.regs.rax & 0xFF;
    let result = al.wrapping_sub(imm);
    flags::update_flags_sub(&mut vcpu.regs.rflags, al, imm, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMP rAX, imm (0x3D)
pub fn cmp_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let rax = vcpu.get_reg(0, op_size);
    let result = rax.wrapping_sub(imm);
    flags::update_flags_sub(&mut vcpu.regs.rflags, rax, imm, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 1: r/m8, imm8 (0x80)
pub fn group1_rm8_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (dst, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, 1), None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64, Some(addr))
    };

    let imm = ctx.consume_u8()? as u64;

    let (result, update_dest) = match op {
        0 => {
            // ADD
            let r = (dst as u8).wrapping_add(imm as u8) as u64;
            flags::update_flags_add(&mut vcpu.regs.rflags, dst, imm, r, 1);
            (r, true)
        }
        1 => {
            // OR
            let r = (dst | imm) & 0xFF;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, 1);
            (r, true)
        }
        2 => {
            // ADC
            let cf = if vcpu.regs.rflags & flags::bits::CF != 0 {
                1u8
            } else {
                0
            };
            let r = (dst as u8).wrapping_add(imm as u8).wrapping_add(cf) as u64;
            flags::update_flags_add(&mut vcpu.regs.rflags, dst, imm.wrapping_add(cf as u64), r, 1);
            (r, true)
        }
        3 => {
            // SBB
            let cf = if vcpu.regs.rflags & flags::bits::CF != 0 {
                1u8
            } else {
                0
            };
            let r = (dst as u8).wrapping_sub(imm as u8).wrapping_sub(cf) as u64;
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm.wrapping_add(cf as u64), r, 1);
            (r, true)
        }
        4 => {
            // AND
            let r = (dst & imm) & 0xFF;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, 1);
            (r, true)
        }
        5 => {
            // SUB
            let r = (dst as u8).wrapping_sub(imm as u8) as u64;
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm, r, 1);
            (r, true)
        }
        6 => {
            // XOR
            let r = (dst ^ imm) & 0xFF;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, 1);
            (r, true)
        }
        7 => {
            // CMP
            let r = (dst as u8).wrapping_sub(imm as u8) as u64;
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm, r, 1);
            (r, false)
        }
        _ => return Err(Error::Emulator(format!("invalid 0x80 /op: {}", op))),
    };

    if update_dest {
        if let Some(addr) = addr_opt {
            vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        } else {
            vcpu.set_reg(rm, result, 1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 1: r/m, imm32 (0x81)
pub fn group1_rm_imm32(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (dst, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, op_size), None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.read_mem(addr, op_size)?, Some(addr))
    };

    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };

    let (result, update_dest) = match op {
        0 => {
            // ADD
            let r = dst.wrapping_add(imm);
            flags::update_flags_add(&mut vcpu.regs.rflags, dst, imm, r, op_size);
            (r, true)
        }
        1 => {
            // OR
            let r = dst | imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, op_size);
            (r, true)
        }
        4 => {
            // AND
            let r = dst & imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, op_size);
            (r, true)
        }
        5 => {
            // SUB
            let r = dst.wrapping_sub(imm);
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm, r, op_size);
            (r, true)
        }
        6 => {
            // XOR
            let r = dst ^ imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, op_size);
            (r, true)
        }
        7 => {
            // CMP
            let r = dst.wrapping_sub(imm);
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm, r, op_size);
            (r, false)
        }
        _ => return Err(Error::Emulator(format!("unimplemented 0x81 /op: {}", op))),
    };

    if update_dest {
        if let Some(addr) = addr_opt {
            vcpu.write_mem(addr, result, op_size)?;
        } else {
            vcpu.set_reg(rm, result, op_size);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 1: r/m, imm8 sign-extended (0x83)
pub fn group1_rm_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (dst, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, op_size), None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.read_mem(addr, op_size)?, Some(addr))
    };

    let imm = ctx.consume_u8()? as i8 as i64 as u64;

    let (result, update_dest) = match op {
        0 => {
            // ADD
            let r = dst.wrapping_add(imm);
            flags::update_flags_add(&mut vcpu.regs.rflags, dst, imm, r, op_size);
            (r, true)
        }
        1 => {
            // OR
            let r = dst | imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, op_size);
            (r, true)
        }
        2 => {
            // ADC
            let cf = if vcpu.regs.rflags & flags::bits::CF != 0 {
                1u64
            } else {
                0
            };
            let r = dst.wrapping_add(imm).wrapping_add(cf);
            flags::update_flags_add(&mut vcpu.regs.rflags, dst, imm.wrapping_add(cf), r, op_size);
            (r, true)
        }
        3 => {
            // SBB
            let cf = if vcpu.regs.rflags & flags::bits::CF != 0 {
                1u64
            } else {
                0
            };
            let r = dst.wrapping_sub(imm).wrapping_sub(cf);
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm.wrapping_add(cf), r, op_size);
            (r, true)
        }
        4 => {
            // AND
            let r = dst & imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, op_size);
            (r, true)
        }
        5 => {
            // SUB
            let r = dst.wrapping_sub(imm);
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm, r, op_size);
            (r, true)
        }
        6 => {
            // XOR
            let r = dst ^ imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, r, op_size);
            (r, true)
        }
        7 => {
            // CMP
            let r = dst.wrapping_sub(imm);
            flags::update_flags_sub(&mut vcpu.regs.rflags, dst, imm, r, op_size);
            (r, false)
        }
        _ => return Err(Error::Emulator(format!("invalid 0x83 /op: {}", op))),
    };

    if update_dest {
        if let Some(addr) = addr_opt {
            vcpu.write_mem(addr, result, op_size)?;
        } else {
            vcpu.set_reg(rm, result, op_size);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// IMUL r, r/m (0x0F 0xAF)
pub fn imul_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    let result = match op_size {
        2 => ((dst as i16).wrapping_mul(src as i16)) as u16 as u64,
        4 => ((dst as i32).wrapping_mul(src as i32)) as u32 as u64,
        8 => ((dst as i64).wrapping_mul(src as i64)) as u64,
        _ => dst.wrapping_mul(src),
    };
    vcpu.set_reg(reg, result, op_size);
    // TODO: Update CF and OF for overflow detection
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
