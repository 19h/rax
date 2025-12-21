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

/// MOVD xmm, r/m32 or MOVQ xmm, r/m64 (66 0F 6E /r)
/// Move doubleword/quadword from GPR/memory to XMM register
pub fn movd_xmm_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_idx = reg as usize;

    // Check REX.W for MOVQ (64-bit) vs MOVD (32-bit)
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    let value = if is_memory {
        if is_64bit {
            vcpu.read_mem(addr, 8)?
        } else {
            vcpu.read_mem(addr, 4)?
        }
    } else {
        if is_64bit {
            vcpu.get_reg(rm, 8)
        } else {
            vcpu.get_reg(rm, 4)
        }
    };

    // Store in low part of XMM, zero-extend to 128 bits
    vcpu.regs.xmm[xmm_idx][0] = value;
    vcpu.regs.xmm[xmm_idx][1] = 0;

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVD r/m32, xmm or MOVQ r/m64, xmm (66 0F 7E /r)
/// Move doubleword/quadword from XMM register to GPR/memory
pub fn movd_rm_xmm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_idx = reg as usize;

    // Check REX.W for MOVQ (64-bit) vs MOVD (32-bit)
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    let value = vcpu.regs.xmm[xmm_idx][0];

    if is_memory {
        if is_64bit {
            vcpu.write_mem(addr, value, 8)?;
        } else {
            vcpu.write_mem(addr, value as u32 as u64, 4)?;
        }
    } else {
        if is_64bit {
            vcpu.set_reg(rm, value, 8);
        } else {
            vcpu.set_reg(rm, value as u32 as u64, 4);
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVD mm, r/m32 or MOVQ mm, r/m64 (NP 0F 6E /r)
/// Move doubleword/quadword from GPR/memory to MMX register
pub fn movd_mm_rm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let mm_idx = (reg & 0x07) as usize;

    // Check REX.W for MOVQ (64-bit) vs MOVD (32-bit)
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    let value = if is_memory {
        if is_64bit {
            vcpu.read_mem(addr, 8)?
        } else {
            vcpu.read_mem(addr, 4)?
        }
    } else {
        if is_64bit {
            vcpu.get_reg(rm, 8)
        } else {
            vcpu.get_reg(rm, 4)
        }
    };

    // Store in MMX register, zero-extended for MOVD
    vcpu.regs.mm[mm_idx] = value;

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVD r/m32, mm or MOVQ r/m64, mm (NP 0F 7E /r)
/// Move doubleword/quadword from MMX register to GPR/memory
pub fn movd_rm_mm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let mm_idx = (reg & 0x07) as usize;

    // Check REX.W for MOVQ (64-bit) vs MOVD (32-bit)
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    let value = vcpu.regs.mm[mm_idx];

    if is_memory {
        if is_64bit {
            vcpu.write_mem(addr, value, 8)?;
        } else {
            vcpu.write_mem(addr, value as u32 as u64, 4)?;
        }
    } else {
        if is_64bit {
            vcpu.set_reg(rm, value, 8);
        } else {
            vcpu.set_reg(rm, value as u32 as u64, 4);
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVQ xmm1, xmm2/m64 (F3 0F 7E /r)
/// Move quadword from xmm2/m64 to xmm1
pub fn movq_xmm_xmm_m64(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    let value = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    // Store in low part of XMM, zero high part
    vcpu.regs.xmm[xmm_dst][0] = value;
    vcpu.regs.xmm[xmm_dst][1] = 0;

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVQ xmm2/m64, xmm1 (66 0F D6 /r)
/// Move quadword from xmm1 to xmm2/m64
pub fn movq_xmm_m64_xmm(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_src = reg as usize;

    let value = vcpu.regs.xmm[xmm_src][0];

    if is_memory {
        vcpu.write_mem(addr, value, 8)?;
    } else {
        let xmm_dst = rm as usize;
        vcpu.regs.xmm[xmm_dst][0] = value;
        vcpu.regs.xmm[xmm_dst][1] = 0;
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

// =============================================================================
// SSE Conversion Instructions
// =============================================================================

/// CVTPS2PD xmm1, xmm2/m64 (NP 0F 5A /r)
/// Convert two packed single-precision FP values to packed double-precision FP values
pub fn cvtps2pd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 64 bits (two f32 values) from source
    let src_low = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    // Extract the two f32 values
    let f0 = f32::from_bits((src_low & 0xFFFFFFFF) as u32);
    let f1 = f32::from_bits(((src_low >> 32) & 0xFFFFFFFF) as u32);

    // Convert to f64
    let d0 = f0 as f64;
    let d1 = f1 as f64;

    // Store as two f64 values (128 bits total)
    vcpu.regs.xmm[xmm_dst][0] = d0.to_bits();
    vcpu.regs.xmm[xmm_dst][1] = d1.to_bits();

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTPD2PS xmm1, xmm2/m128 (66 0F 5A /r)
/// Convert two packed double-precision FP values to packed single-precision FP values
pub fn cvtpd2ps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 128 bits (two f64 values) from source
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Convert the two f64 values to f32
    let d0 = f64::from_bits(src_low);
    let d1 = f64::from_bits(src_high);
    let f0 = d0 as f32;
    let f1 = d1 as f32;

    // Store two f32 values in low 64 bits, zero high 64 bits
    vcpu.regs.xmm[xmm_dst][0] = (f0.to_bits() as u64) | ((f1.to_bits() as u64) << 32);
    vcpu.regs.xmm[xmm_dst][1] = 0;

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTSS2SD xmm1, xmm2/m32 (F3 0F 5A /r)
/// Convert scalar single-precision FP value to scalar double-precision FP value
pub fn cvtss2sd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 32-bit float from source
    let src = if is_memory {
        vcpu.read_mem(addr, 4)? as u32
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0] as u32
    };

    let f = f32::from_bits(src);
    let d = f as f64;

    // Store in low 64 bits, preserve high 64 bits of destination
    vcpu.regs.xmm[xmm_dst][0] = d.to_bits();
    // High part preserved (not modified)

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTSD2SS xmm1, xmm2/m64 (F2 0F 5A /r)
/// Convert scalar double-precision FP value to scalar single-precision FP value
pub fn cvtsd2ss(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 64-bit double from source
    let src = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    let d = f64::from_bits(src);
    let f = d as f32;

    // Store in low 32 bits of low qword, preserve bits 32-127 of destination
    let preserved = vcpu.regs.xmm[xmm_dst][0] & 0xFFFFFFFF00000000;
    vcpu.regs.xmm[xmm_dst][0] = preserved | (f.to_bits() as u64);
    // High part preserved (not modified)

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTDQ2PS xmm1, xmm2/m128 (NP 0F 5B /r)
/// Convert four packed signed doubleword integers to packed single-precision FP values
pub fn cvtdq2ps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 128 bits (four i32 values) from source
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Extract four i32 values
    let i0 = src_low as i32;
    let i1 = (src_low >> 32) as i32;
    let i2 = src_high as i32;
    let i3 = (src_high >> 32) as i32;

    // Convert to f32
    let f0 = i0 as f32;
    let f1 = i1 as f32;
    let f2 = i2 as f32;
    let f3 = i3 as f32;

    // Pack into destination
    vcpu.regs.xmm[xmm_dst][0] = (f0.to_bits() as u64) | ((f1.to_bits() as u64) << 32);
    vcpu.regs.xmm[xmm_dst][1] = (f2.to_bits() as u64) | ((f3.to_bits() as u64) << 32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTPS2DQ xmm1, xmm2/m128 (66 0F 5B /r)
/// Convert four packed single-precision FP values to packed signed doubleword integers
pub fn cvtps2dq(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 128 bits (four f32 values) from source
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Extract four f32 values
    let f0 = f32::from_bits((src_low & 0xFFFFFFFF) as u32);
    let f1 = f32::from_bits(((src_low >> 32) & 0xFFFFFFFF) as u32);
    let f2 = f32::from_bits((src_high & 0xFFFFFFFF) as u32);
    let f3 = f32::from_bits(((src_high >> 32) & 0xFFFFFFFF) as u32);

    // Convert to i32 with rounding (uses current MXCSR rounding mode, default: nearest even)
    let i0 = float_to_int32_round(f0);
    let i1 = float_to_int32_round(f1);
    let i2 = float_to_int32_round(f2);
    let i3 = float_to_int32_round(f3);

    // Pack into destination
    vcpu.regs.xmm[xmm_dst][0] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);
    vcpu.regs.xmm[xmm_dst][1] = (i2 as u32 as u64) | ((i3 as u32 as u64) << 32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTTPS2DQ xmm1, xmm2/m128 (F3 0F 5B /r)
/// Convert four packed single-precision FP values to packed signed doubleword integers (truncate)
pub fn cvttps2dq(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 128 bits (four f32 values) from source
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Extract four f32 values
    let f0 = f32::from_bits((src_low & 0xFFFFFFFF) as u32);
    let f1 = f32::from_bits(((src_low >> 32) & 0xFFFFFFFF) as u32);
    let f2 = f32::from_bits((src_high & 0xFFFFFFFF) as u32);
    let f3 = f32::from_bits(((src_high >> 32) & 0xFFFFFFFF) as u32);

    // Convert to i32 with truncation
    let i0 = float_to_int32_trunc(f0);
    let i1 = float_to_int32_trunc(f1);
    let i2 = float_to_int32_trunc(f2);
    let i3 = float_to_int32_trunc(f3);

    // Pack into destination
    vcpu.regs.xmm[xmm_dst][0] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);
    vcpu.regs.xmm[xmm_dst][1] = (i2 as u32 as u64) | ((i3 as u32 as u64) << 32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTSI2SS xmm1, r/m32 or r/m64 (F3 0F 2A /r)
/// Convert signed integer to scalar single-precision FP value
pub fn cvtsi2ss(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Check REX.W for 64-bit vs 32-bit source
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    let value = if is_memory {
        if is_64bit {
            vcpu.read_mem(addr, 8)? as i64
        } else {
            vcpu.read_mem(addr, 4)? as i32 as i64
        }
    } else {
        if is_64bit {
            vcpu.get_reg(rm, 8) as i64
        } else {
            vcpu.get_reg(rm, 4) as i32 as i64
        }
    };

    let f = value as f32;

    // Store in low 32 bits of low qword, preserve bits 32-127 of destination
    let preserved = vcpu.regs.xmm[xmm_dst][0] & 0xFFFFFFFF00000000;
    vcpu.regs.xmm[xmm_dst][0] = preserved | (f.to_bits() as u64);
    // High part preserved (not modified)

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTSI2SD xmm1, r/m32 or r/m64 (F2 0F 2A /r)
/// Convert signed integer to scalar double-precision FP value
pub fn cvtsi2sd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Check REX.W for 64-bit vs 32-bit source
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    let value = if is_memory {
        if is_64bit {
            vcpu.read_mem(addr, 8)? as i64
        } else {
            vcpu.read_mem(addr, 4)? as i32 as i64
        }
    } else {
        if is_64bit {
            vcpu.get_reg(rm, 8) as i64
        } else {
            vcpu.get_reg(rm, 4) as i32 as i64
        }
    };

    let d = value as f64;

    // Store in low 64 bits, preserve high 64 bits of destination
    vcpu.regs.xmm[xmm_dst][0] = d.to_bits();
    // High part preserved (not modified)

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTSS2SI r32/r64, xmm1/m32 (F3 0F 2D /r)
/// Convert scalar single-precision FP value to signed integer
pub fn cvtss2si(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Check REX.W for 64-bit vs 32-bit destination
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    // Read 32-bit float from source
    let src = if is_memory {
        vcpu.read_mem(addr, 4)? as u32
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0] as u32
    };

    let f = f32::from_bits(src);

    // Convert with rounding
    let result = if is_64bit {
        float_to_int64_round(f) as u64
    } else {
        float_to_int32_round(f) as u32 as u64
    };

    vcpu.set_reg(reg, result, if is_64bit { 8 } else { 4 });
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTSD2SI r32/r64, xmm1/m64 (F2 0F 2D /r)
/// Convert scalar double-precision FP value to signed integer
pub fn cvtsd2si(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Check REX.W for 64-bit vs 32-bit destination
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    // Read 64-bit double from source
    let src = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    let d = f64::from_bits(src);

    // Convert with rounding
    let result = if is_64bit {
        double_to_int64_round(d) as u64
    } else {
        double_to_int32_round(d) as u32 as u64
    };

    vcpu.set_reg(reg, result, if is_64bit { 8 } else { 4 });
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTTSS2SI r32/r64, xmm1/m32 (F3 0F 2C /r)
/// Convert scalar single-precision FP value to signed integer (truncate)
pub fn cvttss2si(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Check REX.W for 64-bit vs 32-bit destination
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    // Read 32-bit float from source
    let src = if is_memory {
        vcpu.read_mem(addr, 4)? as u32
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0] as u32
    };

    let f = f32::from_bits(src);

    // Convert with truncation
    let result = if is_64bit {
        float_to_int64_trunc(f) as u64
    } else {
        float_to_int32_trunc(f) as u32 as u64
    };

    vcpu.set_reg(reg, result, if is_64bit { 8 } else { 4 });
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTTSD2SI r32/r64, xmm1/m64 (F2 0F 2C /r)
/// Convert scalar double-precision FP value to signed integer (truncate)
pub fn cvttsd2si(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Check REX.W for 64-bit vs 32-bit destination
    let is_64bit = ctx.rex.map_or(false, |r| r & 0x08 != 0);

    // Read 64-bit double from source
    let src = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    let d = f64::from_bits(src);

    // Convert with truncation
    let result = if is_64bit {
        double_to_int64_trunc(d) as u64
    } else {
        double_to_int32_trunc(d) as u32 as u64
    };

    vcpu.set_reg(reg, result, if is_64bit { 8 } else { 4 });
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

// Helper functions for float-to-int conversions

/// Convert f32 to i32 with rounding to nearest even (default MXCSR mode)
fn float_to_int32_round(f: f32) -> i32 {
    if f.is_nan() || f.is_infinite() || f > i32::MAX as f32 || f < i32::MIN as f32 {
        // Return 0x80000000 for out-of-range/special values (per Intel spec)
        i32::MIN
    } else {
        // Round to nearest even
        f.round_ties_even() as i32
    }
}

/// Convert f32 to i32 with truncation
fn float_to_int32_trunc(f: f32) -> i32 {
    if f.is_nan() || f.is_infinite() || f > i32::MAX as f32 || f < i32::MIN as f32 {
        i32::MIN
    } else {
        f.trunc() as i32
    }
}

/// Convert f32 to i64 with rounding to nearest even
fn float_to_int64_round(f: f32) -> i64 {
    if f.is_nan() || f.is_infinite() || f > i64::MAX as f32 || f < i64::MIN as f32 {
        i64::MIN
    } else {
        f.round_ties_even() as i64
    }
}

/// Convert f32 to i64 with truncation
fn float_to_int64_trunc(f: f32) -> i64 {
    if f.is_nan() || f.is_infinite() || f > i64::MAX as f32 || f < i64::MIN as f32 {
        i64::MIN
    } else {
        f.trunc() as i64
    }
}

/// Convert f64 to i32 with rounding to nearest even
fn double_to_int32_round(d: f64) -> i32 {
    if d.is_nan() || d.is_infinite() || d > i32::MAX as f64 || d < i32::MIN as f64 {
        i32::MIN
    } else {
        d.round_ties_even() as i32
    }
}

/// Convert f64 to i32 with truncation
#[allow(dead_code)]
fn double_to_int32_trunc(d: f64) -> i32 {
    if d.is_nan() || d.is_infinite() || d > i32::MAX as f64 || d < i32::MIN as f64 {
        i32::MIN
    } else {
        d.trunc() as i32
    }
}

/// Convert f64 to i64 with rounding to nearest even
fn double_to_int64_round(d: f64) -> i64 {
    if d.is_nan() || d.is_infinite() || d > i64::MAX as f64 || d < i64::MIN as f64 {
        i64::MIN
    } else {
        d.round_ties_even() as i64
    }
}

/// Convert f64 to i64 with truncation
fn double_to_int64_trunc(d: f64) -> i64 {
    if d.is_nan() || d.is_infinite() || d > i64::MAX as f64 || d < i64::MIN as f64 {
        i64::MIN
    } else {
        d.trunc() as i64
    }
}

/// CVTDQ2PD xmm1, xmm2/m64 (F3 0F E6 /r)
/// Convert two packed signed doubleword integers to packed double-precision FP values
pub fn cvtdq2pd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 64 bits (two i32 values) from source
    let src_low = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    // Extract two i32 values from low 64 bits
    let i0 = src_low as i32;
    let i1 = (src_low >> 32) as i32;

    // Convert to f64
    let d0 = i0 as f64;
    let d1 = i1 as f64;

    // Store as two f64 values (128 bits total)
    vcpu.regs.xmm[xmm_dst][0] = d0.to_bits();
    vcpu.regs.xmm[xmm_dst][1] = d1.to_bits();

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTPD2DQ xmm1, xmm2/m128 (F2 0F E6 /r)
/// Convert two packed double-precision FP values to packed signed doubleword integers
pub fn cvtpd2dq(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 128 bits (two f64 values) from source
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Extract two f64 values
    let d0 = f64::from_bits(src_low);
    let d1 = f64::from_bits(src_high);

    // Convert to i32 with rounding
    let i0 = double_to_int32_round(d0);
    let i1 = double_to_int32_round(d1);

    // Pack into low 64 bits, zero high 64 bits
    vcpu.regs.xmm[xmm_dst][0] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);
    vcpu.regs.xmm[xmm_dst][1] = 0;

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTTPD2DQ xmm1, xmm2/m128 (66 0F E6 /r)
/// Convert two packed double-precision FP values to packed signed doubleword integers (truncate)
pub fn cvttpd2dq(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 128 bits (two f64 values) from source
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Extract two f64 values
    let d0 = f64::from_bits(src_low);
    let d1 = f64::from_bits(src_high);

    // Convert to i32 with truncation
    let i0 = double_to_int32_trunc_helper(d0);
    let i1 = double_to_int32_trunc_helper(d1);

    // Pack into low 64 bits, zero high 64 bits
    vcpu.regs.xmm[xmm_dst][0] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);
    vcpu.regs.xmm[xmm_dst][1] = 0;

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Convert f64 to i32 with truncation (used by CVTTPD2DQ)
fn double_to_int32_trunc_helper(d: f64) -> i32 {
    if d.is_nan() || d.is_infinite() || d > i32::MAX as f64 || d < i32::MIN as f64 {
        i32::MIN
    } else {
        d.trunc() as i32
    }
}

// =============================================================================
// MMX <-> SSE Conversion Instructions
// =============================================================================

/// CVTPI2PS xmm, mm/m64 (NP 0F 2A /r)
/// Convert two packed signed doubleword integers from MMX to packed single-precision FP
pub fn cvtpi2ps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 64 bits (two i32 values) from MMX or memory
    let src = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let mm_idx = (rm & 0x07) as usize;
        vcpu.regs.mm[mm_idx]
    };

    // Extract two i32 values
    let i0 = src as i32;
    let i1 = (src >> 32) as i32;

    // Convert to f32
    let f0 = i0 as f32;
    let f1 = i1 as f32;

    // Store in low 64 bits, preserve high 64 bits of destination
    vcpu.regs.xmm[xmm_dst][0] = (f0.to_bits() as u64) | ((f1.to_bits() as u64) << 32);
    // High part preserved

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTPS2PI mm, xmm/m64 (NP 0F 2D /r)
/// Convert two packed single-precision FP values to packed signed doubleword integers in MMX
pub fn cvtps2pi(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let mm_dst = (reg & 0x07) as usize;

    // Read 64 bits (two f32 values) from XMM or memory
    let src = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    // Extract two f32 values
    let f0 = f32::from_bits((src & 0xFFFFFFFF) as u32);
    let f1 = f32::from_bits(((src >> 32) & 0xFFFFFFFF) as u32);

    // Convert to i32 with rounding
    let i0 = float_to_int32_round(f0);
    let i1 = float_to_int32_round(f1);

    // Store in MMX register
    vcpu.regs.mm[mm_dst] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTTPS2PI mm, xmm/m64 (NP 0F 2C /r)
/// Convert two packed single-precision FP values to packed signed doubleword integers (truncate)
pub fn cvttps2pi(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let mm_dst = (reg & 0x07) as usize;

    // Read 64 bits (two f32 values) from XMM or memory
    let src = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_src][0]
    };

    // Extract two f32 values
    let f0 = f32::from_bits((src & 0xFFFFFFFF) as u32);
    let f1 = f32::from_bits(((src >> 32) & 0xFFFFFFFF) as u32);

    // Convert to i32 with truncation
    let i0 = float_to_int32_trunc(f0);
    let i1 = float_to_int32_trunc(f1);

    // Store in MMX register
    vcpu.regs.mm[mm_dst] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTPI2PD xmm, mm/m64 (66 0F 2A /r)
/// Convert two packed signed doubleword integers from MMX to packed double-precision FP
pub fn cvtpi2pd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    // Read 64 bits (two i32 values) from MMX or memory
    let src = if is_memory {
        vcpu.read_mem(addr, 8)?
    } else {
        let mm_idx = (rm & 0x07) as usize;
        vcpu.regs.mm[mm_idx]
    };

    // Extract two i32 values
    let i0 = src as i32;
    let i1 = (src >> 32) as i32;

    // Convert to f64
    let d0 = i0 as f64;
    let d1 = i1 as f64;

    // Store as two f64 values (128 bits total)
    vcpu.regs.xmm[xmm_dst][0] = d0.to_bits();
    vcpu.regs.xmm[xmm_dst][1] = d1.to_bits();

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTPD2PI mm, xmm/m128 (66 0F 2D /r)
/// Convert two packed double-precision FP values to packed signed doubleword integers in MMX
pub fn cvtpd2pi(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let mm_dst = (reg & 0x07) as usize;

    // Read 128 bits (two f64 values) from XMM or memory
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Convert two f64 values to i32 with rounding
    let d0 = f64::from_bits(src_low);
    let d1 = f64::from_bits(src_high);
    let i0 = double_to_int32_round(d0);
    let i1 = double_to_int32_round(d1);

    // Store in MMX register
    vcpu.regs.mm[mm_dst] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CVTTPD2PI mm, xmm/m128 (66 0F 2C /r)
/// Convert two packed double-precision FP values to packed signed doubleword integers (truncate)
pub fn cvttpd2pi(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let mm_dst = (reg & 0x07) as usize;

    // Read 128 bits (two f64 values) from XMM or memory
    let (src_low, src_high) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        let xmm_src = rm as usize;
        (vcpu.regs.xmm[xmm_src][0], vcpu.regs.xmm[xmm_src][1])
    };

    // Convert two f64 values to i32 with truncation
    let d0 = f64::from_bits(src_low);
    let d1 = f64::from_bits(src_high);
    let i0 = double_to_int32_trunc_helper(d0);
    let i1 = double_to_int32_trunc_helper(d1);

    // Store in MMX register
    vcpu.regs.mm[mm_dst] = (i0 as u32 as u64) | ((i1 as u32 as u64) << 32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
