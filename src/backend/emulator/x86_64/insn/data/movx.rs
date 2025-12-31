//! MOVZX, MOVSX, MOVSXD instructions.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

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
    let has_rex = ctx.rex.is_some();
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64
    } else {
        // Use get_reg8 to properly handle high-byte registers (AH, BH, CH, DH)
        vcpu.get_reg8(rm, has_rex)
    };
    
    // Debug: trace byte loads in shell expansion area 
    let rip = vcpu.regs.rip;
    // Trace 0x87 loads to understand the string flow
    if is_memory && value == 0x87 && rip >= 0x560000 && rip < 0x580000 {
        // Dump 64 bytes starting from address
        let mut dump = String::new();
        let mut ascii = String::new();
        for i in 0..32u64 {
            if let Ok(b) = vcpu.mmu.read_u8(addr.wrapping_add(i), &vcpu.sregs) {
                dump.push_str(&format!("{:02x} ", b));
                if b >= 0x20 && b <= 0x7e {
                    ascii.push(b as char);
                } else if b == 0 {
                    ascii.push('.');
                } else {
                    ascii.push('?');
                }
            } else {
                dump.push_str("?? ");
                ascii.push('X');
            }
        }
        eprintln!("[LOAD-87] RIP={:#x} addr={:#x} R15={:#x}", rip, addr, vcpu.regs.r15);
        eprintln!("  hex:   {}", dump);
        eprintln!("  ascii: {}", ascii);
    }
    
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
    let has_rex = ctx.rex.is_some();
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        // Use get_reg8 to properly handle high-byte registers (AH, BH, CH, DH)
        vcpu.get_reg8(rm, has_rex) as u8
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
