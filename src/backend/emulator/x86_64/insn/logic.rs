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

    match op {
        0 | 1 => {
            // TEST r/m8, imm8
            let dst = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, 1)
            } else {
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
                let val = vcpu.get_reg(rm, 1);
                vcpu.set_reg(rm, !val, 1);
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
                let val = vcpu.get_reg(rm, 1) as u8;
                let result = (val as i8).wrapping_neg() as u8;
                vcpu.set_reg(rm, result as u64, 1);
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
            let dst = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, op_size)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, op_size)?
            };
            let imm_size = if op_size == 8 { 4 } else { op_size };
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
        _ => return Err(Error::Emulator(format!("unimplemented 0xF7 /op: {}", op))),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
