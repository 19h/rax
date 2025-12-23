//! Stack frame instructions: ENTER, LEAVE, BOUND, and EVEX dispatch.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{EvexPrefix, InsnContext, X86_64Vcpu};

/// ENTER imm16, imm8 (0xC8) - Create stack frame
pub fn enter(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let alloc_size = ctx.consume_u16()? as u64;
    let nesting_level = (ctx.consume_u8()? & 0x1F) as u64;

    // Push RBP
    vcpu.push64(vcpu.regs.rbp)?;
    let frame_ptr = vcpu.regs.rsp;

    if nesting_level > 0 {
        // Push nested frame pointers
        for i in 1..nesting_level {
            vcpu.regs.rbp = vcpu.regs.rbp.wrapping_sub(8);
            let ptr = vcpu.read_mem(vcpu.regs.rbp, 8)?;
            vcpu.push64(ptr)?;
            let _ = i;
        }
        vcpu.push64(frame_ptr)?;
    }

    vcpu.regs.rbp = frame_ptr;
    vcpu.regs.rsp = vcpu.regs.rsp.wrapping_sub(alloc_size);
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
    // Check if we're in 64-bit mode by looking at EFER.LMA AND CS.L
    // EFER.LMA = 1 and CS.L = 1 means 64-bit mode (EVEX)
    // EFER.LMA = 1 and CS.L = 0 means compatibility mode (BOUND)
    // EFER.LMA = 0 means legacy/real mode (BOUND)
    let in_long_mode = (vcpu.sregs.efer & 0x400) != 0; // EFER.LMA = bit 10
    let in_64bit_mode = in_long_mode && vcpu.sregs.cs.l;

    if in_64bit_mode {
        // In 64-bit mode, 0x62 is EVEX prefix (AVX-512)
        let context_bytes: Vec<u8> = ctx.bytes[ctx.cursor..].iter().take(8).cloned().collect();
        return Err(Error::Emulator(format!(
            "EVEX at RIP={:#x}, bytes after 0x62: {:02x?}",
            vcpu.regs.rip, context_bytes
        )));
    } else {
        // In 32-bit/compatibility mode, this is BOUND (bounds check)
        let modrm_start = ctx.cursor;
        let modrm = ctx.consume_u8()?;
        let reg = (modrm >> 3) & 7;

        // BOUND requires memory operand (mod != 11)
        if modrm >> 6 == 3 {
            return Err(Error::Emulator(format!(
                "BOUND requires memory operand at RIP={:#x}",
                vcpu.regs.rip
            )));
        }

        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;

        // Determine operand size (16-bit or 32-bit)
        // CS.D (db flag) determines default: D=0 means 16-bit default, D=1 means 32-bit default
        let default_16bit = !vcpu.sregs.cs.db;
        let is_16bit = default_16bit ^ ctx.operand_size_override;

        // Read the index from the register
        // Read bounds from memory: [addr] = lower, [addr+size] = upper
        if is_16bit {
            let index = vcpu.get_reg(reg, 2) as i16;
            let lower = vcpu.read_mem16(addr)? as i16;
            let upper = vcpu.read_mem16(addr + 2)? as i16;

            // Check: lower <= index <= upper
            if index < lower || index > upper {
                // #BR exception - for now just return error
                return Err(Error::Emulator(format!(
                    "BOUND range exceeded: index {} not in [{}, {}] at RIP={:#x}",
                    index, lower, upper, vcpu.regs.rip
                )));
            }
        } else {
            let index = vcpu.get_reg(reg, 4) as i32;
            let lower = vcpu.read_mem32(addr)? as i32;
            let upper = vcpu.read_mem32(addr + 4)? as i32;

            // Check: lower <= index <= upper
            if index < lower || index > upper {
                // #BR exception - for now just return error
                return Err(Error::Emulator(format!(
                    "BOUND range exceeded: index {} not in [{}, {}] at RIP={:#x}",
                    index, lower, upper, vcpu.regs.rip
                )));
            }
        }

        vcpu.regs.rip += ctx.cursor as u64;
    }
    Ok(None)
}
