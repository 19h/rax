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

/// ADC r/m8, r8 (0x10) - Add with Carry
pub fn adc_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = dst.wrapping_add(src).wrapping_add(cf_val) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_adc(&mut vcpu.regs.rflags, dst, src, cf_in, result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = dst.wrapping_add(src).wrapping_add(cf_val) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_adc(&mut vcpu.regs.rflags, dst, src, cf_in, result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADC r/m, r (0x11) - Add with Carry
pub fn adc_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst.wrapping_add(src).wrapping_add(cf_val);
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_adc(&mut vcpu.regs.rflags, dst, src, cf_in, result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst.wrapping_add(src).wrapping_add(cf_val);
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_adc(&mut vcpu.regs.rflags, dst, src, cf_in, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADC r8, r/m8 (0x12) - Add with Carry
pub fn adc_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = dst.wrapping_add(src).wrapping_add(cf_val) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_adc(&mut vcpu.regs.rflags, dst, src, cf_in, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADC r, r/m (0x13) - Add with Carry
pub fn adc_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst.wrapping_add(src).wrapping_add(cf_val);
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_adc(&mut vcpu.regs.rflags, dst, src, cf_in, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADC AL, imm8 (0x14) - Add with Carry
pub fn adc_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let al = vcpu.regs.rax & 0xFF;
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };
    let result = al.wrapping_add(imm).wrapping_add(cf_val);
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (result & 0xFF);
    flags::update_flags_adc(&mut vcpu.regs.rflags, al, imm, cf_in, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ADC rAX, imm (0x15) - Add with Carry
pub fn adc_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let rax = vcpu.get_reg(0, op_size);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };
    let result = rax.wrapping_add(imm).wrapping_add(cf_val);
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_adc(&mut vcpu.regs.rflags, rax, imm, cf_in, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r/m8, r8 (0x18) - Subtract with Borrow
pub fn sbb_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = dst.wrapping_sub(src).wrapping_sub(cf_val) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, src, cf_in, result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = dst.wrapping_sub(src).wrapping_sub(cf_val) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, src, cf_in, result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r/m, r (0x19) - Subtract with Borrow
pub fn sbb_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst.wrapping_sub(src).wrapping_sub(cf_val);
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, src, cf_in, result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst.wrapping_sub(src).wrapping_sub(cf_val);
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, src, cf_in, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r8, r/m8 (0x1A) - Subtract with Borrow
pub fn sbb_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = dst.wrapping_sub(src).wrapping_sub(cf_val) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, src, cf_in, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB r, r/m (0x1B) - Subtract with Borrow
pub fn sbb_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst.wrapping_sub(src).wrapping_sub(cf_val);
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, src, cf_in, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SBB AL, imm8 (0x1C) - Subtract with Borrow
pub fn sbb_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let al = vcpu.regs.rax & 0xFF;
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };
    let result = al.wrapping_sub(imm).wrapping_sub(cf_val);
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (result & 0xFF);
    flags::update_flags_sbb(&mut vcpu.regs.rflags, al, imm, cf_in, result, 1);
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
    let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let cf_val = if cf_in { 1u64 } else { 0 };
    let result = rax.wrapping_sub(imm).wrapping_sub(cf_val);
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_sbb(&mut vcpu.regs.rflags, rax, imm, cf_in, result, op_size);
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
    ctx.rip_relative_offset = 1;
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
    let imm_size = if op_size == 8 { 4 } else { op_size };
    ctx.rip_relative_offset = imm_size as usize;
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
        2 => {
            // ADC
            let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
            let cf_val = if cf_in { 1u64 } else { 0 };
            let r = dst.wrapping_add(imm).wrapping_add(cf_val);
            flags::update_flags_adc(&mut vcpu.regs.rflags, dst, imm, cf_in, r, op_size);
            (r, true)
        }
        3 => {
            // SBB
            let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
            let cf_val = if cf_in { 1u64 } else { 0 };
            let r = dst.wrapping_sub(imm).wrapping_sub(cf_val);
            flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, imm, cf_in, r, op_size);
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
    ctx.rip_relative_offset = 1;
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

    let (result, overflow) = match op_size {
        2 => {
            let r = (dst as i16 as i32).wrapping_mul(src as i16 as i32);
            (r as u16 as u64, r as i16 as i32 != r)
        }
        4 => {
            let r = (dst as i32 as i64).wrapping_mul(src as i32 as i64);
            (r as u32 as u64, r as i32 as i64 != r)
        }
        8 => {
            let r = (dst as i64 as i128).wrapping_mul(src as i64 as i128);
            (r as u64, r as i64 as i128 != r)
        }
        _ => (dst.wrapping_mul(src), false),
    };
    vcpu.set_reg(reg, result, op_size);
    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// IMUL r, r/m, imm (0x69) - 3-operand with 16/32-bit immediate
pub fn imul_r_rm_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    ctx.rip_relative_offset = imm_size as usize;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    // Immediate is sign-extended to operand size
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };

    let (result, overflow) = match op_size {
        2 => {
            let r = (src as i16 as i32).wrapping_mul(imm as i16 as i32);
            (r as u16 as u64, r as i16 as i32 != r)
        }
        4 => {
            let r = (src as i32 as i64).wrapping_mul(imm as i32 as i64);
            (r as u32 as u64, r as i32 as i64 != r)
        }
        8 => {
            let r = (src as i64 as i128).wrapping_mul(imm as i64 as i128);
            (r as u64, r as i64 as i128 != r)
        }
        _ => (src.wrapping_mul(imm), false),
    };
    vcpu.set_reg(reg, result, op_size);
    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// IMUL r, r/m, imm8 (0x6B) - 3-operand with 8-bit immediate
pub fn imul_r_rm_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    ctx.rip_relative_offset = 1;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    // Immediate is 8-bit, sign-extended to operand size
    let imm = ctx.consume_imm(1)? as i8 as i64 as u64;

    let (result, overflow) = match op_size {
        2 => {
            let r = (src as i16 as i32).wrapping_mul(imm as i16 as i32);
            (r as u16 as u64, r as i16 as i32 != r)
        }
        4 => {
            let r = (src as i32 as i64).wrapping_mul(imm as i32 as i64);
            (r as u32 as u64, r as i32 as i64 != r)
        }
        8 => {
            let r = (src as i64 as i128).wrapping_mul(imm as i64 as i128);
            (r as u64, r as i64 as i128 != r)
        }
        _ => (src.wrapping_mul(imm), false),
    };
    vcpu.set_reg(reg, result, op_size);
    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CBW/CWDE/CDQE (0x98) - sign-extend AL to AX, AX to EAX, or EAX to RAX
pub fn cbw_cwde_cdqe(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    match ctx.op_size {
        2 => {
            // CBW: AL -> AX
            let val = vcpu.regs.rax as i8 as i16 as u16;
            vcpu.regs.rax = (vcpu.regs.rax & !0xFFFF) | val as u64;
        }
        4 => {
            // CWDE: AX -> EAX
            let val = vcpu.regs.rax as i16 as i32 as u32;
            vcpu.regs.rax = val as u64;
        }
        8 => {
            // CDQE: EAX -> RAX
            let val = vcpu.regs.rax as i32 as i64 as u64;
            vcpu.regs.rax = val;
        }
        _ => {}
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CWD/CDQ/CQO (0x99) - sign-extend AX to DX:AX, EAX to EDX:EAX, or RAX to RDX:RAX
pub fn cwd_cdq_cqo(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    match ctx.op_size {
        2 => {
            // CWD: AX -> DX:AX
            let sign = if (vcpu.regs.rax as i16) < 0 { 0xFFFF } else { 0 };
            vcpu.regs.rdx = (vcpu.regs.rdx & !0xFFFF) | sign;
        }
        4 => {
            // CDQ: EAX -> EDX:EAX
            let sign = if (vcpu.regs.rax as i32) < 0 { 0xFFFFFFFF } else { 0 };
            vcpu.regs.rdx = sign;
        }
        8 => {
            // CQO: RAX -> RDX:RAX
            let sign = if (vcpu.regs.rax as i64) < 0 { u64::MAX } else { 0 };
            vcpu.regs.rdx = sign;
        }
        _ => {}
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
