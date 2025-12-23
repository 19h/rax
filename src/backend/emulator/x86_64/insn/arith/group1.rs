//! Group 1 instructions (0x80, 0x81, 0x83).
//!
//! These opcodes handle multiple operations (ADD, OR, ADC, SBB, AND, SUB, XOR, CMP)
//! based on the ModR/M reg field.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

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
            let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
            let cf_val = if cf_in { 1u8 } else { 0 };
            let r = (dst as u8).wrapping_add(imm as u8).wrapping_add(cf_val) as u64;
            flags::update_flags_adc(&mut vcpu.regs.rflags, dst, imm, cf_in, r, 1);
            (r, true)
        }
        3 => {
            // SBB
            let cf_in = (vcpu.regs.rflags & flags::bits::CF) != 0;
            let cf_val = if cf_in { 1u8 } else { 0 };
            let r = (dst as u8).wrapping_sub(imm as u8).wrapping_sub(cf_val) as u64;
            flags::update_flags_sbb(&mut vcpu.regs.rflags, dst, imm, cf_in, r, 1);
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
