//! Data movement instructions: MOV, LEA, PUSH, POP, XCHG, MOVZX, MOVSX, XADD, CMPXCHG.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;

/// MOV r8, imm8 (0xB0-0xB7)
pub fn mov_r8_imm8(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
) -> Result<Option<VcpuExit>> {
    let reg = (opcode - 0xB0) | ctx.rex_b();
    let imm = ctx.consume_u8()?;
    vcpu.set_reg(reg, imm as u64, 1);
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

/// LEA r, m (0x8D)
pub fn lea(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg = ((modrm >> 3) & 0x07) | ctx.rex_r();

    let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;
    vcpu.set_reg(reg, addr, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOV r/m8, imm8 (0xC6 /0)
pub fn mov_rm8_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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

/// MOV r/m, imm (0xC7 /0)
pub fn mov_rm_imm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (_, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let imm_size = if op_size == 8 { 4 } else { op_size };
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

/// PUSH r64 (0x50-0x57)
pub fn push_r64(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
) -> Result<Option<VcpuExit>> {
    let reg = (opcode - 0x50) | ctx.rex_b();
    let value = vcpu.get_reg(reg, 8);
    vcpu.push64(value)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PUSH imm8 (0x6A)
pub fn push_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u8()? as i8 as i64 as u64;
    vcpu.push64(imm)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PUSH imm32 (0x68)
pub fn push_imm32(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm = ctx.consume_u32()? as i32 as i64 as u64;
    vcpu.push64(imm)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// POP r64 (0x58-0x5F)
pub fn pop_r64(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
) -> Result<Option<VcpuExit>> {
    let reg = (opcode - 0x58) | ctx.rex_b();
    let value = vcpu.pop64()?;
    vcpu.set_reg(reg, value, 8);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XCHG r, r/m (0x87)
pub fn xchg_r_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let reg_val = vcpu.get_reg(reg, op_size);

    if is_memory {
        let mem_val = vcpu.read_mem(addr, op_size)?;
        vcpu.set_reg(reg, mem_val, op_size);
        vcpu.write_mem(addr, reg_val, op_size)?;
    } else {
        let rm_val = vcpu.get_reg(rm, op_size);
        vcpu.set_reg(reg, rm_val, op_size);
        vcpu.set_reg(rm, reg_val, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XCHG rAX, r (0x91-0x97)
pub fn xchg_rax_r(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode: u8,
) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let reg = (opcode - 0x90) | ctx.rex_b();
    let rax_val = vcpu.get_reg(0, op_size);
    let reg_val = vcpu.get_reg(reg, op_size);
    vcpu.set_reg(0, reg_val, op_size);
    vcpu.set_reg(reg, rax_val, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVSXD r64, r/m32 (0x63 with REX.W)
pub fn movsxd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.rex_w() {
        return Err(Error::Emulator(
            "MOVSXD without REX.W not supported".to_string(),
        ));
    }
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u32(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 4) as u32
    };
    // Sign-extend 32-bit to 64-bit
    let extended = value as i32 as i64 as u64;
    vcpu.set_reg(reg, extended, 8);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVZX r, r/m8 (0x0F 0xB6)
pub fn movzx_r_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 1) & 0xFF
    };
    vcpu.set_reg(reg, value, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVZX r, r/m16 (0x0F 0xB7)
pub fn movzx_r_rm16(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u16(addr, &vcpu.sregs)? as u64
    } else {
        vcpu.get_reg(rm, 2) & 0xFFFF
    };
    vcpu.set_reg(reg, value, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVSX r, r/m8 (0x0F 0xBE)
pub fn movsx_r_rm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 1) as u8
    };
    // Sign-extend
    let extended = value as i8 as i64 as u64;
    vcpu.set_reg(reg, extended, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVSX r, r/m16 (0x0F 0xBF)
pub fn movsx_r_rm16(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u16(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 2) as u16
    };
    // Sign-extend
    let extended = value as i16 as i64 as u64;
    vcpu.set_reg(reg, extended, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BSWAP r32/r64 (0x0F 0xC8-0xCF)
pub fn bswap(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    opcode2: u8,
) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let reg = (opcode2 - 0xC8) | ctx.rex_b();
    let value = vcpu.get_reg(reg, op_size);
    let swapped = match op_size {
        4 => (value as u32).swap_bytes() as u64,
        8 => value.swap_bytes(),
        _ => value,
    };
    vcpu.set_reg(reg, swapped, op_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LEAVE (0xC9)
pub fn leave(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rsp = vcpu.regs.rbp;
    vcpu.regs.rbp = vcpu.pop64()?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BOUND (32-bit) or EVEX prefix (64-bit) (0x62)
pub fn bound_or_evex(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Check if we're in 64-bit mode by looking at EFER.LMA
    let in_long_mode = (vcpu.sregs.efer & 0x400) != 0; // EFER.LMA = bit 10

    if in_long_mode {
        // In 64-bit mode, 0x62 is EVEX prefix (AVX-512)
        let context_bytes: Vec<u8> = ctx.bytes[ctx.cursor..].iter().take(8).cloned().collect();
        return Err(Error::Emulator(format!(
            "EVEX at RIP={:#x}, bytes after 0x62: {:02x?}",
            vcpu.regs.rip, context_bytes
        )));
    } else {
        // In 32-bit mode, this is BOUND (bounds check)
        let modrm_start = ctx.cursor;
        let modrm = ctx.consume_u8()?;
        // Skip memory operand (BOUND always uses memory)
        if modrm >> 6 != 3 {
            let (_, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;
        }
        // BOUND doesn't do anything if bounds are OK (we assume they are)
        vcpu.regs.rip += ctx.cursor as u64;
    }
    Ok(None)
}

/// XADD r/m8, r8 (0x0F 0xC0) - Exchange and Add
pub fn xadd_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1) as u8;

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)?;
        let sum = dst.wrapping_add(src);
        // DEST = DEST + SRC, SRC = old DEST
        vcpu.mmu.write_u8(addr, sum, &vcpu.sregs)?;
        vcpu.set_reg(reg, dst as u64, 1);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst as u64, src as u64, sum as u64, 1);
    } else {
        let dst = vcpu.get_reg(rm, 1) as u8;
        let sum = dst.wrapping_add(src);
        vcpu.set_reg(rm, sum as u64, 1);
        vcpu.set_reg(reg, dst as u64, 1);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst as u64, src as u64, sum as u64, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XADD r/m, r (0x0F 0xC1) - Exchange and Add
pub fn xadd_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let sum = dst.wrapping_add(src);
        // DEST = DEST + SRC, SRC = old DEST
        vcpu.write_mem(addr, sum, op_size)?;
        vcpu.set_reg(reg, dst, op_size);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, sum, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let sum = dst.wrapping_add(src);
        vcpu.set_reg(rm, sum, op_size);
        vcpu.set_reg(reg, dst, op_size);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, sum, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMPXCHG r/m8, r8 (0x0F 0xB0) - Compare and Exchange
pub fn cmpxchg_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, 1) as u8;
    let al = (vcpu.regs.rax & 0xFF) as u8;

    let dst = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg(rm, 1) as u8
    };

    // Compare AL with destination
    let cmp_result = al.wrapping_sub(dst);
    flags::update_flags_sub(&mut vcpu.regs.rflags, al as u64, dst as u64, cmp_result as u64, 1);

    if al == dst {
        // ZF is set, store source into destination
        if is_memory {
            vcpu.mmu.write_u8(addr, src, &vcpu.sregs)?;
        } else {
            vcpu.set_reg(rm, src as u64, 1);
        }
    } else {
        // ZF is clear, load destination into AL
        vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (dst as u64);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMPXCHG r/m, r (0x0F 0xB1) - Compare and Exchange
pub fn cmpxchg_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);
    let rax = vcpu.get_reg(0, op_size);

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    // Compare rAX with destination
    let cmp_result = rax.wrapping_sub(dst);
    flags::update_flags_sub(&mut vcpu.regs.rflags, rax, dst, cmp_result, op_size);

    if rax == dst {
        // ZF is set, store source into destination
        if is_memory {
            vcpu.write_mem(addr, src, op_size)?;
        } else {
            vcpu.set_reg(rm, src, op_size);
        }
    } else {
        // ZF is clear, load destination into rAX
        vcpu.set_reg(0, dst, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
