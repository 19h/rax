//! SSE packed operations: MOVUPS, MOVAPS, ANDPS, ORPS, XORPS, etc.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

// =============================================================================
// Packed Move Instructions (MOVUPS, MOVAPS, MOVSS, MOVSD)
// =============================================================================

/// MOVUPS/MOVUPD/MOVSS/MOVSD xmm, xmm/m (0F 10)
/// NP: MOVUPS, 66: MOVUPD, F3: MOVSS, F2: MOVSD
pub fn movups_load(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    if ctx.rep_prefix == Some(0xF3) {
        // MOVSS - move scalar single
        let value = if is_memory {
            vcpu.read_mem(addr, 4)?
        } else {
            vcpu.regs.xmm[rm as usize][0] & 0xFFFFFFFF
        };
        if is_memory {
            vcpu.regs.xmm[xmm_dst][0] = value;
            vcpu.regs.xmm[xmm_dst][1] = 0;
        } else {
            // Reg-to-reg: merge low 32 bits, keep rest
            vcpu.regs.xmm[xmm_dst][0] = (vcpu.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | value;
        }
    } else if ctx.rep_prefix == Some(0xF2) {
        // MOVSD - move scalar double
        let value = if is_memory {
            vcpu.read_mem(addr, 8)?
        } else {
            vcpu.regs.xmm[rm as usize][0]
        };
        if is_memory {
            vcpu.regs.xmm[xmm_dst][0] = value;
            vcpu.regs.xmm[xmm_dst][1] = 0;
        } else {
            vcpu.regs.xmm[xmm_dst][0] = value;
        }
    } else {
        // MOVUPS/MOVUPD - move packed (unaligned OK)
        if is_memory {
            vcpu.regs.xmm[xmm_dst][0] = vcpu.read_mem(addr, 8)?;
            vcpu.regs.xmm[xmm_dst][1] = vcpu.read_mem(addr + 8, 8)?;
        } else {
            let xmm_src = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVUPS/MOVUPD/MOVSS/MOVSD xmm/m, xmm (0F 11)
/// NP: MOVUPS, 66: MOVUPD, F3: MOVSS, F2: MOVSD
pub fn movups_store(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_src = reg as usize;

    if ctx.rep_prefix == Some(0xF3) {
        // MOVSS store
        let value = vcpu.regs.xmm[xmm_src][0] & 0xFFFFFFFF;
        if is_memory {
            vcpu.write_mem(addr, value, 4)?;
        } else {
            let xmm_dst = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = (vcpu.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | value;
        }
    } else if ctx.rep_prefix == Some(0xF2) {
        // MOVSD store
        let value = vcpu.regs.xmm[xmm_src][0];
        if is_memory {
            vcpu.write_mem(addr, value, 8)?;
        } else {
            let xmm_dst = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = value;
        }
    } else {
        // MOVUPS/MOVUPD store
        if is_memory {
            vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][0], 8)?;
            vcpu.write_mem(addr + 8, vcpu.regs.xmm[xmm_src][1], 8)?;
        } else {
            let xmm_dst = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVAPS/MOVAPD xmm, xmm/m128 (0F 28)
pub fn movaps_load(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    if is_memory {
        if addr & 0xF != 0 {
            return Err(Error::Emulator(format!(
                "MOVAPS/MOVAPD: unaligned memory access at {:#x}",
                addr
            )));
        }
        vcpu.regs.xmm[xmm_dst][0] = vcpu.read_mem(addr, 8)?;
        vcpu.regs.xmm[xmm_dst][1] = vcpu.read_mem(addr + 8, 8)?;
    } else {
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
        vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVAPS/MOVAPD xmm/m128, xmm (0F 29)
pub fn movaps_store(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_src = reg as usize;

    if is_memory {
        if addr & 0xF != 0 {
            return Err(Error::Emulator(format!(
                "MOVAPS/MOVAPD: unaligned memory access at {:#x}",
                addr
            )));
        }
        vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][0], 8)?;
        vcpu.write_mem(addr + 8, vcpu.regs.xmm[xmm_src][1], 8)?;
    } else {
        let xmm_dst = rm as usize;
        vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
        vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

// =============================================================================
// Packed Logical Operations (ANDPS, ANDNPS, ORPS, XORPS)
// =============================================================================

/// ANDPS/ANDPD xmm, xmm/m128 (0F 54)
pub fn andps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    vcpu.regs.xmm[xmm_dst][0] &= src_lo;
    vcpu.regs.xmm[xmm_dst][1] &= src_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ANDNPS/ANDNPD xmm, xmm/m128 (0F 55)
pub fn andnps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    vcpu.regs.xmm[xmm_dst][0] = (!vcpu.regs.xmm[xmm_dst][0]) & src_lo;
    vcpu.regs.xmm[xmm_dst][1] = (!vcpu.regs.xmm[xmm_dst][1]) & src_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// ORPS/ORPD xmm, xmm/m128 (0F 56)
pub fn orps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    vcpu.regs.xmm[xmm_dst][0] |= src_lo;
    vcpu.regs.xmm[xmm_dst][1] |= src_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XORPS/XORPD xmm, xmm/m128 (0F 57)
pub fn xorps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    vcpu.regs.xmm[xmm_dst][0] ^= src_lo;
    vcpu.regs.xmm[xmm_dst][1] ^= src_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

// =============================================================================
// Prefetch Hints (PREFETCHNTA/PREFETCHT0/PREFETCHT1/PREFETCHT2)
// =============================================================================

/// PREFETCHh m8 (0F 18 /0-3) - cache prefetch hints, treated as NOP in emulator
pub fn prefetchh(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let hint = (modrm >> 3) & 0x07;

    if hint > 3 {
        return Err(Error::Emulator(format!(
            "unimplemented PREFETCHh hint /{} at RIP={:#x}",
            hint, vcpu.regs.rip
        )));
    }

    let (_, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PREFETCHW/PREFETCHWT1 m8 (0F 0D /1-2) - prefetch with intent to write, treated as NOP
pub fn prefetchw(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let hint = (modrm >> 3) & 0x07;

    if hint != 1 && hint != 2 {
        return Err(Error::Emulator(format!(
            "unimplemented PREFETCHW hint /{} at RIP={:#x}",
            hint, vcpu.regs.rip
        )));
    }

    let (_, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

// =============================================================================
// Packed Integer Subtract (PSUB* family)
// =============================================================================

/// PSUB* packed integer subtract (SSE2, 66 0F xx)
pub fn psub_packed(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext, opcode: u8) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override {
        return Err(Error::Emulator(format!(
            "PSUB* requires 66 prefix at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };

    let dst_lo = vcpu.regs.xmm[xmm_dst][0];
    let dst_hi = vcpu.regs.xmm[xmm_dst][1];

    let (res_lo, res_hi) = match opcode {
        0xD8 => (sub_u8_saturate(dst_lo, src_lo), sub_u8_saturate(dst_hi, src_hi)), // PSUBUSB
        0xD9 => (sub_u16_saturate(dst_lo, src_lo), sub_u16_saturate(dst_hi, src_hi)), // PSUBUSW
        0xE8 => (sub_i8_saturate(dst_lo, src_lo), sub_i8_saturate(dst_hi, src_hi)), // PSUBSB
        0xE9 => (sub_i16_saturate(dst_lo, src_lo), sub_i16_saturate(dst_hi, src_hi)), // PSUBSW
        0xF8 => (sub_u8_wrap(dst_lo, src_lo), sub_u8_wrap(dst_hi, src_hi)), // PSUBB
        0xF9 => (sub_u16_wrap(dst_lo, src_lo), sub_u16_wrap(dst_hi, src_hi)), // PSUBW
        0xFA => (sub_u32_wrap(dst_lo, src_lo), sub_u32_wrap(dst_hi, src_hi)), // PSUBD
        0xFB => (dst_lo.wrapping_sub(src_lo), dst_hi.wrapping_sub(src_hi)), // PSUBQ
        _ => {
            return Err(Error::Emulator(format!(
                "unimplemented PSUB opcode {:#x} at RIP={:#x}",
                opcode, vcpu.regs.rip
            )))
        }
    };

    vcpu.regs.xmm[xmm_dst][0] = res_lo;
    vcpu.regs.xmm[xmm_dst][1] = res_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

fn sub_u8_wrap(a: u64, b: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..8 {
        let va = ((a >> (i * 8)) & 0xFF) as u8;
        let vb = ((b >> (i * 8)) & 0xFF) as u8;
        let diff = va.wrapping_sub(vb);
        result |= (diff as u64) << (i * 8);
    }
    result
}

fn sub_u16_wrap(a: u64, b: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..4 {
        let va = ((a >> (i * 16)) & 0xFFFF) as u16;
        let vb = ((b >> (i * 16)) & 0xFFFF) as u16;
        let diff = va.wrapping_sub(vb);
        result |= (diff as u64) << (i * 16);
    }
    result
}

fn sub_u32_wrap(a: u64, b: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..2 {
        let va = ((a >> (i * 32)) & 0xFFFF_FFFF) as u32;
        let vb = ((b >> (i * 32)) & 0xFFFF_FFFF) as u32;
        let diff = va.wrapping_sub(vb);
        result |= (diff as u64) << (i * 32);
    }
    result
}

fn sub_u8_saturate(a: u64, b: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..8 {
        let va = ((a >> (i * 8)) & 0xFF) as u8;
        let vb = ((b >> (i * 8)) & 0xFF) as u8;
        let diff = va.saturating_sub(vb);
        result |= (diff as u64) << (i * 8);
    }
    result
}

fn sub_u16_saturate(a: u64, b: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..4 {
        let va = ((a >> (i * 16)) & 0xFFFF) as u16;
        let vb = ((b >> (i * 16)) & 0xFFFF) as u16;
        let diff = va.saturating_sub(vb);
        result |= (diff as u64) << (i * 16);
    }
    result
}

fn sub_i8_saturate(a: u64, b: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..8 {
        let va = ((a >> (i * 8)) & 0xFF) as i8;
        let vb = ((b >> (i * 8)) & 0xFF) as i8;
        let diff = va.saturating_sub(vb) as u8;
        result |= (diff as u64) << (i * 8);
    }
    result
}

fn sub_i16_saturate(a: u64, b: u64) -> u64 {
    let mut result = 0u64;
    for i in 0..4 {
        let va = ((a >> (i * 16)) & 0xFFFF) as i16;
        let vb = ((b >> (i * 16)) & 0xFFFF) as i16;
        let diff = va.saturating_sub(vb) as u16;
        result |= (diff as u64) << (i * 16);
    }
    result
}

// =============================================================================
// Packed Integer Misc (PMADDWD/PMAX*/PMIN*/PMOVMSKB)
// =============================================================================

/// PMADDWD - Multiply and Add Packed Integers (0xF5)
pub fn pmaddwd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override {
        return Err(Error::Emulator(format!(
            "PMADDWD requires 66 prefix at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    let dst_lo = vcpu.regs.xmm[xmm_dst][0];
    let dst_hi = vcpu.regs.xmm[xmm_dst][1];

    let mut d_words = [0i16; 8];
    let mut s_words = [0i16; 8];
    for i in 0..4 {
        d_words[i] = ((dst_lo >> (i * 16)) & 0xFFFF) as i16;
        s_words[i] = ((src_lo >> (i * 16)) & 0xFFFF) as i16;
    }
    for i in 0..4 {
        d_words[i + 4] = ((dst_hi >> (i * 16)) & 0xFFFF) as i16;
        s_words[i + 4] = ((src_hi >> (i * 16)) & 0xFFFF) as i16;
    }

    let mut result_lo = 0u64;
    let mut result_hi = 0u64;
    for i in 0..4 {
        let a0 = d_words[i * 2] as i32;
        let a1 = d_words[i * 2 + 1] as i32;
        let b0 = s_words[i * 2] as i32;
        let b1 = s_words[i * 2 + 1] as i32;
        let sum = a0.wrapping_mul(b0).wrapping_add(a1.wrapping_mul(b1));
        let val = sum as u32 as u64;
        if i < 2 {
            result_lo |= val << (i * 32);
        } else {
            result_hi |= val << ((i - 2) * 32);
        }
    }

    vcpu.regs.xmm[xmm_dst][0] = result_lo;
    vcpu.regs.xmm[xmm_dst][1] = result_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PMAXSW - Maximum of Packed Signed Words (0xEE)
pub fn pmaxsw(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override {
        return Err(Error::Emulator(format!(
            "PMAXSW requires 66 prefix at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    let dst_lo = vcpu.regs.xmm[xmm_dst][0];
    let dst_hi = vcpu.regs.xmm[xmm_dst][1];

    let mut result_lo = 0u64;
    let mut result_hi = 0u64;
    for i in 0..4 {
        let d = ((dst_lo >> (i * 16)) & 0xFFFF) as i16;
        let s = ((src_lo >> (i * 16)) & 0xFFFF) as i16;
        result_lo |= ((d.max(s) as u16) as u64) << (i * 16);
    }
    for i in 0..4 {
        let d = ((dst_hi >> (i * 16)) & 0xFFFF) as i16;
        let s = ((src_hi >> (i * 16)) & 0xFFFF) as i16;
        result_hi |= ((d.max(s) as u16) as u64) << (i * 16);
    }
    vcpu.regs.xmm[xmm_dst][0] = result_lo;
    vcpu.regs.xmm[xmm_dst][1] = result_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PMINSW - Minimum of Packed Signed Words (0xEA)
pub fn pminsw(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override {
        return Err(Error::Emulator(format!(
            "PMINSW requires 66 prefix at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    let dst_lo = vcpu.regs.xmm[xmm_dst][0];
    let dst_hi = vcpu.regs.xmm[xmm_dst][1];

    let mut result_lo = 0u64;
    let mut result_hi = 0u64;
    for i in 0..4 {
        let d = ((dst_lo >> (i * 16)) & 0xFFFF) as i16;
        let s = ((src_lo >> (i * 16)) & 0xFFFF) as i16;
        result_lo |= ((d.min(s) as u16) as u64) << (i * 16);
    }
    for i in 0..4 {
        let d = ((dst_hi >> (i * 16)) & 0xFFFF) as i16;
        let s = ((src_hi >> (i * 16)) & 0xFFFF) as i16;
        result_hi |= ((d.min(s) as u16) as u64) << (i * 16);
    }
    vcpu.regs.xmm[xmm_dst][0] = result_lo;
    vcpu.regs.xmm[xmm_dst][1] = result_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PMAXUB - Maximum of Packed Unsigned Bytes (0xDE)
pub fn pmaxub(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override {
        return Err(Error::Emulator(format!(
            "PMAXUB requires 66 prefix at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    let dst_lo = vcpu.regs.xmm[xmm_dst][0];
    let dst_hi = vcpu.regs.xmm[xmm_dst][1];

    let mut result_lo = 0u64;
    let mut result_hi = 0u64;
    for i in 0..8 {
        let d = ((dst_lo >> (i * 8)) & 0xFF) as u8;
        let s = ((src_lo >> (i * 8)) & 0xFF) as u8;
        result_lo |= (d.max(s) as u64) << (i * 8);
    }
    for i in 0..8 {
        let d = ((dst_hi >> (i * 8)) & 0xFF) as u8;
        let s = ((src_hi >> (i * 8)) & 0xFF) as u8;
        result_hi |= (d.max(s) as u64) << (i * 8);
    }
    vcpu.regs.xmm[xmm_dst][0] = result_lo;
    vcpu.regs.xmm[xmm_dst][1] = result_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PMINUB - Minimum of Packed Unsigned Bytes (0xDA)
pub fn pminub(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override {
        return Err(Error::Emulator(format!(
            "PMINUB requires 66 prefix at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };
    let dst_lo = vcpu.regs.xmm[xmm_dst][0];
    let dst_hi = vcpu.regs.xmm[xmm_dst][1];

    let mut result_lo = 0u64;
    let mut result_hi = 0u64;
    for i in 0..8 {
        let d = ((dst_lo >> (i * 8)) & 0xFF) as u8;
        let s = ((src_lo >> (i * 8)) & 0xFF) as u8;
        result_lo |= (d.min(s) as u64) << (i * 8);
    }
    for i in 0..8 {
        let d = ((dst_hi >> (i * 8)) & 0xFF) as u8;
        let s = ((src_hi >> (i * 8)) & 0xFF) as u8;
        result_hi |= (d.min(s) as u64) << (i * 8);
    }
    vcpu.regs.xmm[xmm_dst][0] = result_lo;
    vcpu.regs.xmm[xmm_dst][1] = result_hi;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// PMOVMSKB - Move Byte Mask (0xD7)
pub fn pmovmskb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !ctx.operand_size_override {
        return Err(Error::Emulator(format!(
            "PMOVMSKB requires 66 prefix at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let (src_lo, src_hi) = if is_memory {
        (vcpu.read_mem(addr, 8)?, vcpu.read_mem(addr + 8, 8)?)
    } else {
        (vcpu.regs.xmm[rm as usize][0], vcpu.regs.xmm[rm as usize][1])
    };

    let mut mask = 0u64;
    for i in 0..8 {
        let byte = ((src_lo >> (i * 8)) & 0xFF) as u8;
        if byte & 0x80 != 0 {
            mask |= 1u64 << i;
        }
    }
    for i in 0..8 {
        let byte = ((src_hi >> (i * 8)) & 0xFF) as u8;
        if byte & 0x80 != 0 {
            mask |= 1u64 << (i + 8);
        }
    }

    let dst_size = if ctx.rex_w() { 8 } else { 4 };
    vcpu.set_reg(reg, mask, dst_size);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

// =============================================================================
// MOVLPS/MOVHPS - Move Low/High Packed Single-Precision
// =============================================================================

/// MOVLPS xmm, m64 / MOVHLPS xmm, xmm (0F 12)
pub fn movlps_load(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    if is_memory {
        // MOVLPS: Load 64 bits from memory to low qword
        vcpu.regs.xmm[xmm_dst][0] = vcpu.read_mem(addr, 8)?;
    } else {
        // MOVHLPS: Move high qword to low qword
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][1];
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVLPS m64, xmm (0F 13)
pub fn movlps_store(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_src = reg as usize;

    if is_memory {
        vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][0], 8)?;
    } else {
        return Err(Error::Emulator(format!(
            "MOVLPS store requires memory operand at RIP={:#x}",
            vcpu.regs.rip
        )));
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVHPS xmm, m64 / MOVLHPS xmm, xmm (0F 16)
pub fn movhps_load(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    if is_memory {
        // MOVHPS: Load 64 bits from memory to high qword
        vcpu.regs.xmm[xmm_dst][1] = vcpu.read_mem(addr, 8)?;
    } else {
        // MOVLHPS: Move low qword to high qword
        let xmm_src = rm as usize;
        vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][0];
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVHPS m64, xmm (0F 17)
pub fn movhps_store(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_src = reg as usize;

    if is_memory {
        vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][1], 8)?;
    } else {
        return Err(Error::Emulator(format!(
            "MOVHPS store requires memory operand at RIP={:#x}",
            vcpu.regs.rip
        )));
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
