//! AVX (VEX-encoded) SIMD instruction implementations.
//!
//! VEX-encoded move operations for 128-bit (XMM) and 256-bit (YMM) registers.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

// =============================================================================
// VEX-encoded Move Operations
// =============================================================================

/// VMOVDQA load - VEX.66.0F 6F /r (aligned)
pub fn vmovdqa_load(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    vex_l: u8,
) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let xmm_dst = reg as usize;

    if vex_l == 0 {
        // 128-bit XMM
        if is_memory {
            if addr & 0xF != 0 {
                return Err(Error::Emulator(format!(
                    "VMOVDQA: unaligned memory access at {:#x}",
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
        // VEX clears upper bits
        vcpu.regs.ymm_high[xmm_dst][0] = 0;
        vcpu.regs.ymm_high[xmm_dst][1] = 0;
    } else {
        // 256-bit YMM
        if is_memory {
            if addr & 0x1F != 0 {
                return Err(Error::Emulator(format!(
                    "VMOVDQA: unaligned memory access at {:#x}",
                    addr
                )));
            }
            vcpu.regs.xmm[xmm_dst][0] = vcpu.read_mem(addr, 8)?;
            vcpu.regs.xmm[xmm_dst][1] = vcpu.read_mem(addr + 8, 8)?;
            vcpu.regs.ymm_high[xmm_dst][0] = vcpu.read_mem(addr + 16, 8)?;
            vcpu.regs.ymm_high[xmm_dst][1] = vcpu.read_mem(addr + 24, 8)?;
        } else {
            let xmm_src = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
            vcpu.regs.ymm_high[xmm_dst][0] = vcpu.regs.ymm_high[xmm_src][0];
            vcpu.regs.ymm_high[xmm_dst][1] = vcpu.regs.ymm_high[xmm_src][1];
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// VMOVDQU load - VEX.F3.0F 6F /r (unaligned)
pub fn vmovdqu_load(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    vex_l: u8,
) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_dst = reg as usize;

    if vex_l == 0 {
        // 128-bit XMM
        if is_memory {
            vcpu.regs.xmm[xmm_dst][0] = vcpu.read_mem(addr, 8)?;
            vcpu.regs.xmm[xmm_dst][1] = vcpu.read_mem(addr + 8, 8)?;
        } else {
            let xmm_src = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
        }
        vcpu.regs.ymm_high[xmm_dst][0] = 0;
        vcpu.regs.ymm_high[xmm_dst][1] = 0;
    } else {
        // 256-bit YMM
        if is_memory {
            vcpu.regs.xmm[xmm_dst][0] = vcpu.read_mem(addr, 8)?;
            vcpu.regs.xmm[xmm_dst][1] = vcpu.read_mem(addr + 8, 8)?;
            vcpu.regs.ymm_high[xmm_dst][0] = vcpu.read_mem(addr + 16, 8)?;
            vcpu.regs.ymm_high[xmm_dst][1] = vcpu.read_mem(addr + 24, 8)?;
        } else {
            let xmm_src = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
            vcpu.regs.ymm_high[xmm_dst][0] = vcpu.regs.ymm_high[xmm_src][0];
            vcpu.regs.ymm_high[xmm_dst][1] = vcpu.regs.ymm_high[xmm_src][1];
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// VMOVDQA store - VEX.66.0F 7F /r (aligned)
pub fn vmovdqa_store(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    vex_l: u8,
) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_src = reg as usize;

    if vex_l == 0 {
        // 128-bit XMM
        if is_memory {
            if addr & 0xF != 0 {
                return Err(Error::Emulator(format!(
                    "VMOVDQA: unaligned memory access at {:#x}",
                    addr
                )));
            }
            vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][0], 8)?;
            vcpu.write_mem(addr + 8, vcpu.regs.xmm[xmm_src][1], 8)?;
        } else {
            let xmm_dst = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
            vcpu.regs.ymm_high[xmm_dst][0] = 0;
            vcpu.regs.ymm_high[xmm_dst][1] = 0;
        }
    } else {
        // 256-bit YMM
        if is_memory {
            if addr & 0x1F != 0 {
                return Err(Error::Emulator(format!(
                    "VMOVDQA: unaligned memory access at {:#x}",
                    addr
                )));
            }
            vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][0], 8)?;
            vcpu.write_mem(addr + 8, vcpu.regs.xmm[xmm_src][1], 8)?;
            vcpu.write_mem(addr + 16, vcpu.regs.ymm_high[xmm_src][0], 8)?;
            vcpu.write_mem(addr + 24, vcpu.regs.ymm_high[xmm_src][1], 8)?;
        } else {
            let xmm_dst = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
            vcpu.regs.ymm_high[xmm_dst][0] = vcpu.regs.ymm_high[xmm_src][0];
            vcpu.regs.ymm_high[xmm_dst][1] = vcpu.regs.ymm_high[xmm_src][1];
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// VMOVDQU store - VEX.F3.0F 7F /r (unaligned)
pub fn vmovdqu_store(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    vex_l: u8,
) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let xmm_src = reg as usize;

    if vex_l == 0 {
        // 128-bit XMM
        if is_memory {
            vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][0], 8)?;
            vcpu.write_mem(addr + 8, vcpu.regs.xmm[xmm_src][1], 8)?;
        } else {
            let xmm_dst = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
            vcpu.regs.ymm_high[xmm_dst][0] = 0;
            vcpu.regs.ymm_high[xmm_dst][1] = 0;
        }
    } else {
        // 256-bit YMM
        if is_memory {
            vcpu.write_mem(addr, vcpu.regs.xmm[xmm_src][0], 8)?;
            vcpu.write_mem(addr + 8, vcpu.regs.xmm[xmm_src][1], 8)?;
            vcpu.write_mem(addr + 16, vcpu.regs.ymm_high[xmm_src][0], 8)?;
            vcpu.write_mem(addr + 24, vcpu.regs.ymm_high[xmm_src][1], 8)?;
        } else {
            let xmm_dst = rm as usize;
            vcpu.regs.xmm[xmm_dst][0] = vcpu.regs.xmm[xmm_src][0];
            vcpu.regs.xmm[xmm_dst][1] = vcpu.regs.xmm[xmm_src][1];
            vcpu.regs.ymm_high[xmm_dst][0] = vcpu.regs.ymm_high[xmm_src][0];
            vcpu.regs.ymm_high[xmm_dst][1] = vcpu.regs.ymm_high[xmm_src][1];
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
