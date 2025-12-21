//! Logic instructions: AND, OR, XOR, TEST, NOT.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;

/// OR r/m8, r8 (0x08)
pub fn or_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = (dst | src) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = (dst | src) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// OR r/m, r (0x09)
pub fn or_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst | src;
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst | src;
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// OR r8, r/m8 (0x0A)
pub fn or_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = (dst | src) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// OR r, r/m (0x0B)
pub fn or_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst | src;
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AND r/m8, r8 (0x20)
pub fn and_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = (dst & src) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = (dst & src) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AND r/m, r (0x21)
pub fn and_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst & src;
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst & src;
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AND r8, r/m8 (0x22)
pub fn and_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = (dst & src) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AND r, r/m (0x23)
pub fn and_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst & src;
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// OR AL, imm8 (0x0C)
pub fn or_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let result = (vcpu.regs.rax & 0xFF) | imm;
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | result;
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// OR rAX, imm (0x0D)
pub fn or_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let result = vcpu.get_reg(0, op_size) | imm;
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AND AL, imm8 (0x24)
pub fn and_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let result = (vcpu.regs.rax & 0xFF) & imm;
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | result;
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AND rAX, imm (0x25)
pub fn and_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let result = vcpu.get_reg(0, op_size) & imm;
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XOR AL, imm8 (0x34)
pub fn xor_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let result = (vcpu.regs.rax & 0xFF) ^ imm;
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | result;
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XOR rAX, imm (0x35)
pub fn xor_rax_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let imm_size = if op_size == 8 { 4 } else { op_size };
    let imm = ctx.consume_imm(imm_size)?;
    let imm = if op_size == 8 {
        imm as i32 as i64 as u64
    } else {
        imm
    };
    let result = vcpu.get_reg(0, op_size) ^ imm;
    vcpu.set_reg(0, result, op_size);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XOR r/m8, r8 (0x30)
pub fn xor_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1);

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
        let result = (dst ^ src) & 0xFF;
        vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1);
        let result = (dst ^ src) & 0xFF;
        vcpu.set_reg(rm, result, 1);
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XOR r/m, r (0x31)
pub fn xor_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let result = dst ^ src;
        vcpu.write_mem(addr, result, op_size)?;
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let result = dst ^ src;
        vcpu.set_reg(rm, result, op_size);
        flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XOR r8, r/m8 (0x32)
pub fn xor_r8_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, 1);

    let src = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1)
    };
    let result = (dst ^ src) & 0xFF;
    vcpu.set_reg(reg, result, 1);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XOR r, r/m (0x33)
pub fn xor_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let dst = vcpu.get_reg(reg, op_size);

    let src = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let result = dst ^ src;
    vcpu.set_reg(reg, result, op_size);
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// TEST r/m8, r8 (0x84)
pub fn test_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1) as u8;

    let dst = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 1) as u8
    };
    let result = (dst & src) as u64;
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
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
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// TEST AL, imm8 (0xA8)
pub fn test_al_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as u64;
    let result = (vcpu.regs.rax & 0xFF) & imm;
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
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
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 3: TEST/NOT/NEG r/m8 (0xF6)
pub fn group3_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let has_rex = ctx.rex.is_some();
    match op {
        0 | 1 => {
            // TEST r/m8, imm8
            let dst = if modrm >> 6 == 3 {
                vcpu.get_reg8(rm, has_rex)
            } else {
                ctx.rip_relative_offset = 1;
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
            };
            let imm = ctx.consume_u8()? as u64;
            let result = dst & imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, result, 1);
        }
        2 => {
            // NOT r/m8
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg8(rm, has_rex);
                vcpu.set_reg8(rm, !val, has_rex);
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.mmu.read_u8(addr, &vcpu.sregs)?;
                vcpu.mmu.write_u8(addr, !val, &vcpu.sregs)?;
            }
        }
        3 => {
            // NEG r/m8
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg8(rm, has_rex) as u8;
                let result = (val as i8).wrapping_neg() as u8;
                vcpu.set_reg8(rm, result as u64, has_rex);
                flags::update_flags_sub(&mut vcpu.regs.rflags, 0, val as u64, result as u64, 1);
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.mmu.read_u8(addr, &vcpu.sregs)?;
                let result = (val as i8).wrapping_neg() as u8;
                vcpu.mmu.write_u8(addr, result, &vcpu.sregs)?;
                flags::update_flags_sub(&mut vcpu.regs.rflags, 0, val as u64, result as u64, 1);
            }
        }
        4 => {
            // MUL r/m8 (unsigned)
            let src = if modrm >> 6 == 3 {
                vcpu.get_reg8(rm, has_rex) as u8
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u8(addr, &vcpu.sregs)?
            };
            let al = vcpu.regs.rax as u8;
            let result = (al as u16) * (src as u16);
            vcpu.set_reg(0, result as u64, 2);
            let overflow = (result >> 8) != 0;
            flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
        }
        5 => {
            // IMUL r/m8 (signed, one-operand)
            let src = if modrm >> 6 == 3 {
                vcpu.get_reg8(rm, has_rex) as u8
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u8(addr, &vcpu.sregs)?
            };
            let al = vcpu.regs.rax as u8;
            let result = (al as i8 as i16) * (src as i8 as i16);
            vcpu.set_reg(0, result as i16 as u16 as u64, 2);
            let overflow = result != (result as i8 as i16);
            flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
        }
        6 => {
            // DIV r/m8 (unsigned)
            let divisor = if modrm >> 6 == 3 {
                vcpu.get_reg8(rm, has_rex) as u8
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u8(addr, &vcpu.sregs)?
            };
            if divisor == 0 {
                return Err(Error::Emulator("divide by zero".to_string()));
            }
            let dividend = vcpu.regs.rax as u16;
            let quotient = dividend / divisor as u16;
            let remainder = dividend % divisor as u16;
            if quotient > 0xFF {
                return Err(Error::Emulator("divide overflow".to_string()));
            }
            let ax = ((remainder as u16) << 8) | (quotient as u16);
            vcpu.set_reg(0, ax as u64, 2);
        }
        7 => {
            // IDIV r/m8 (signed)
            let divisor = if modrm >> 6 == 3 {
                vcpu.get_reg8(rm, has_rex) as u8
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.mmu.read_u8(addr, &vcpu.sregs)?
            };
            if divisor == 0 {
                return Err(Error::Emulator("divide by zero".to_string()));
            }
            let dividend = vcpu.regs.rax as i16;
            let divisor = divisor as i8 as i16;
            let quotient = dividend / divisor;
            let remainder = dividend % divisor;
            if quotient < i8::MIN as i16 || quotient > i8::MAX as i16 {
                return Err(Error::Emulator("divide overflow".to_string()));
            }
            let ax = ((remainder as i8 as u8 as u16) << 8) | (quotient as i8 as u8 as u16);
            vcpu.set_reg(0, ax as u64, 2);
        }
        _ => return Err(Error::Emulator(format!("unimplemented 0xF6 /op: {}", op))),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 3: TEST/NOT/NEG r/m (0xF7)
pub fn group3_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    match op {
        0 | 1 => {
            // TEST r/m, imm
            let imm_size = if op_size == 8 { 4 } else { op_size };
            let dst = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, op_size)
            } else {
                ctx.rip_relative_offset = imm_size as usize;
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, op_size)?
            };
            let imm = ctx.consume_imm(imm_size)?;
            let imm = if op_size == 8 {
                imm as i32 as i64 as u64
            } else {
                imm
            };
            let result = dst & imm;
            flags::update_flags_logic(&mut vcpu.regs.rflags, result, op_size);
        }
        2 => {
            // NOT r/m
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, op_size);
                vcpu.set_reg(rm, !val, op_size);
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.read_mem(addr, op_size)?;
                vcpu.write_mem(addr, !val, op_size)?;
            }
        }
        3 => {
            // NEG r/m
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, op_size);
                let result = match op_size {
                    1 => (val as i8).wrapping_neg() as u8 as u64,
                    2 => (val as i16).wrapping_neg() as u16 as u64,
                    4 => (val as i32).wrapping_neg() as u32 as u64,
                    8 => (val as i64).wrapping_neg() as u64,
                    _ => val,
                };
                vcpu.set_reg(rm, result, op_size);
                flags::update_flags_sub(&mut vcpu.regs.rflags, 0, val, result, op_size);
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.read_mem(addr, op_size)?;
                let result = match op_size {
                    1 => (val as i8).wrapping_neg() as u8 as u64,
                    2 => (val as i16).wrapping_neg() as u16 as u64,
                    4 => (val as i32).wrapping_neg() as u32 as u64,
                    8 => (val as i64).wrapping_neg() as u64,
                    _ => val,
                };
                vcpu.write_mem(addr, result, op_size)?;
                flags::update_flags_sub(&mut vcpu.regs.rflags, 0, val, result, op_size);
            }
        }
        4 => {
            // MUL r/m (unsigned multiply)
            // DX:AX = AX * r/m (or 64-bit equivalent)
            let val = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, op_size)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, op_size)?
            };

            match op_size {
                2 => {
                    let result = (vcpu.regs.rax as u16 as u32) * (val as u16 as u32);
                    vcpu.set_reg(0, (result & 0xFFFF) as u64, 2);
                    vcpu.set_reg(2, ((result >> 16) & 0xFFFF) as u64, 2);
                    let overflow = (result >> 16) != 0;
                    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
                }
                4 => {
                    let result = (vcpu.regs.rax as u32 as u64) * (val as u32 as u64);
                    vcpu.set_reg(0, (result & 0xFFFFFFFF) as u64, 4);
                    vcpu.set_reg(2, ((result >> 32) & 0xFFFFFFFF) as u64, 4);
                    let overflow = (result >> 32) != 0;
                    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
                }
                8 => {
                    let result = (vcpu.regs.rax as u128) * (val as u128);
                    vcpu.set_reg(0, result as u64, 8);
                    vcpu.set_reg(2, (result >> 64) as u64, 8);
                    let overflow = (result >> 64) != 0;
                    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
                }
                _ => {}
            }
        }
        5 => {
            // IMUL r/m (signed multiply, one-operand form)
            let val = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, op_size)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, op_size)?
            };

            match op_size {
                2 => {
                    let result = (vcpu.regs.rax as i16 as i32) * (val as i16 as i32);
                    vcpu.set_reg(0, result as i16 as u16 as u64, 2);
                    vcpu.set_reg(2, (result >> 16) as i16 as u16 as u64, 2);
                    let overflow = result as i16 as i32 != result;
                    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
                }
                4 => {
                    let result = (vcpu.regs.rax as i32 as i64) * (val as i32 as i64);
                    vcpu.set_reg(0, result as u32 as u64, 4);
                    vcpu.set_reg(2, (result >> 32) as u32 as u64, 4);
                    let overflow = result as i32 as i64 != result;
                    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
                }
                8 => {
                    let result = (vcpu.regs.rax as i64 as i128) * (val as i64 as i128);
                    vcpu.set_reg(0, result as u64, 8);
                    vcpu.set_reg(2, (result >> 64) as u64, 8);
                    let overflow = result as i64 as i128 != result;
                    flags::set_cf_of(&mut vcpu.regs.rflags, overflow, overflow);
                }
                _ => {}
            }
        }
        6 => {
            // DIV r/m (unsigned divide)
            let divisor = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, op_size)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, op_size)?
            };

            if divisor == 0 {
                return Err(Error::Emulator("divide by zero".to_string()));
            }

            match op_size {
                2 => {
                    let dividend = ((vcpu.regs.rdx as u16 as u32) << 16) | (vcpu.regs.rax as u16 as u32);
                    let divisor = divisor as u16 as u32;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient > 0xFFFF {
                        return Err(Error::Emulator("divide overflow".to_string()));
                    }
                    vcpu.set_reg(0, quotient as u64, 2);
                    vcpu.set_reg(2, remainder as u64, 2);
                }
                4 => {
                    let dividend = ((vcpu.regs.rdx as u32 as u64) << 32) | (vcpu.regs.rax as u32 as u64);
                    let divisor = divisor as u32 as u64;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient > 0xFFFFFFFF {
                        return Err(Error::Emulator("divide overflow".to_string()));
                    }
                    vcpu.set_reg(0, quotient as u32 as u64, 4);
                    vcpu.set_reg(2, remainder as u32 as u64, 4);
                }
                8 => {
                    let dividend = ((vcpu.regs.rdx as u128) << 64) | (vcpu.regs.rax as u128);
                    let divisor = divisor as u128;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient > u64::MAX as u128 {
                        return Err(Error::Emulator("divide overflow".to_string()));
                    }
                    vcpu.set_reg(0, quotient as u64, 8);
                    vcpu.set_reg(2, remainder as u64, 8);
                }
                _ => {}
            }
        }
        7 => {
            // IDIV r/m (signed divide)
            let divisor = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, op_size)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, op_size)?
            };

            if divisor == 0 {
                return Err(Error::Emulator("divide by zero".to_string()));
            }

            match op_size {
                2 => {
                    let dividend = (((vcpu.regs.rdx as u16 as u32) << 16) | (vcpu.regs.rax as u16 as u32)) as i32;
                    let divisor = divisor as i16 as i32;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient < i16::MIN as i32 || quotient > i16::MAX as i32 {
                        return Err(Error::Emulator("divide overflow".to_string()));
                    }
                    vcpu.set_reg(0, quotient as u16 as u64, 2);
                    vcpu.set_reg(2, remainder as u16 as u64, 2);
                }
                4 => {
                    let dividend = (((vcpu.regs.rdx as u32 as u64) << 32) | (vcpu.regs.rax as u32 as u64)) as i64;
                    let divisor = divisor as i32 as i64;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient < i32::MIN as i64 || quotient > i32::MAX as i64 {
                        return Err(Error::Emulator("divide overflow".to_string()));
                    }
                    vcpu.set_reg(0, quotient as u32 as u64, 4);
                    vcpu.set_reg(2, remainder as u32 as u64, 4);
                }
                8 => {
                    let dividend = (((vcpu.regs.rdx as u128) << 64) | (vcpu.regs.rax as u128)) as i128;
                    let divisor = divisor as i64 as i128;
                    let quotient = dividend / divisor;
                    let remainder = dividend % divisor;
                    if quotient < i64::MIN as i128 || quotient > i64::MAX as i128 {
                        return Err(Error::Emulator("divide overflow".to_string()));
                    }
                    vcpu.set_reg(0, quotient as u64, 8);
                    vcpu.set_reg(2, remainder as u64, 8);
                }
                _ => {}
            }
        }
        _ => return Err(Error::Emulator(format!("unimplemented 0xF7 /op: {}", op))),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
