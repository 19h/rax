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
