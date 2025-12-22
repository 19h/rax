//! ADD and ADC instructions.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

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
