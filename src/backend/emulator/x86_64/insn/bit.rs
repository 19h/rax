//! Bit manipulation instructions: BT, BTS, BTR, BTC, BSF, BSR.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;

/// BT r/m, r (0x0F 0xA3)
pub fn bt_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size);

    let value = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);
    let cf_bit = (value >> bit_pos) & 1;
    if cf_bit != 0 {
        vcpu.regs.rflags |= flags::bits::CF;
    } else {
        vcpu.regs.rflags &= !flags::bits::CF;
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BTS r/m, r (0x0F 0xAB)
pub fn bts_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size);
    let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);

    if is_memory {
        let v = vcpu.read_mem(addr, op_size)?;
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v | (1 << bit_pos);
        vcpu.write_mem(addr, new_val, op_size)?;
    } else {
        let v = vcpu.get_reg(rm, op_size);
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v | (1 << bit_pos);
        vcpu.set_reg(rm, new_val, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BTR r/m, r (0x0F 0xB3)
pub fn btr_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size);
    let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);

    if is_memory {
        let v = vcpu.read_mem(addr, op_size)?;
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v & !(1 << bit_pos);
        vcpu.write_mem(addr, new_val, op_size)?;
    } else {
        let v = vcpu.get_reg(rm, op_size);
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v & !(1 << bit_pos);
        vcpu.set_reg(rm, new_val, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BTC r/m, r (0x0F 0xBB)
pub fn btc_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size);
    let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);

    if is_memory {
        let v = vcpu.read_mem(addr, op_size)?;
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v ^ (1 << bit_pos);
        vcpu.write_mem(addr, new_val, op_size)?;
    } else {
        let v = vcpu.get_reg(rm, op_size);
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v ^ (1 << bit_pos);
        vcpu.set_reg(rm, new_val, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BSF r, r/m (0x0F 0xBC)
pub fn bsf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    if value == 0 {
        vcpu.regs.rflags |= flags::bits::ZF;
        // Destination is undefined when source is 0
    } else {
        vcpu.regs.rflags &= !flags::bits::ZF;
        let bit_index = value.trailing_zeros() as u64;
        vcpu.set_reg(reg, bit_index, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BSR r, r/m (0x0F 0xBD)
pub fn bsr(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    if value == 0 {
        vcpu.regs.rflags |= flags::bits::ZF;
        // Destination is undefined when source is 0
    } else {
        vcpu.regs.rflags &= !flags::bits::ZF;
        // Count leading zeros for the specific operand size
        let bit_index = match op_size {
            2 => 15 - (value as u16).leading_zeros(),
            4 => 31 - (value as u32).leading_zeros(),
            8 => 63 - value.leading_zeros(),
            _ => 63 - value.leading_zeros(),
        };
        vcpu.set_reg(reg, bit_index as u64, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 8: BT/BTS/BTR/BTC with immediate (0x0F 0xBA)
pub fn group8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    ctx.rip_relative_offset = 1;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let reg_op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (value, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, op_size), None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.read_mem(addr, op_size)?, Some(addr))
    };

    let bit_pos = (ctx.consume_u8()? & ((op_size * 8 - 1) as u8)) as u64;

    let cf_bit = (value >> bit_pos) & 1;
    if cf_bit != 0 {
        vcpu.regs.rflags |= flags::bits::CF;
    } else {
        vcpu.regs.rflags &= !flags::bits::CF;
    }

    match reg_op {
        4 => {} // BT - just test
        5 => {
            // BTS - test and set
            let new_val = value | (1 << bit_pos);
            if let Some(addr) = addr_opt {
                vcpu.write_mem(addr, new_val, op_size)?;
            } else {
                vcpu.set_reg(rm, new_val, op_size);
            }
        }
        6 => {
            // BTR - test and reset
            let new_val = value & !(1 << bit_pos);
            if let Some(addr) = addr_opt {
                vcpu.write_mem(addr, new_val, op_size)?;
            } else {
                vcpu.set_reg(rm, new_val, op_size);
            }
        }
        7 => {
            // BTC - test and complement
            let new_val = value ^ (1 << bit_pos);
            if let Some(addr) = addr_opt {
                vcpu.write_mem(addr, new_val, op_size)?;
            } else {
                vcpu.set_reg(rm, new_val, op_size);
            }
        }
        _ => {
            return Err(Error::Emulator(format!(
                "unimplemented 0F BA /{} at RIP={:#x}",
                reg_op, vcpu.regs.rip
            )));
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// TZCNT r, r/m (F3 0F BC) - Count trailing zeros
pub fn tzcnt(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    let bit_count = (op_size * 8) as u64;
    let result = if value == 0 {
        bit_count
    } else {
        value.trailing_zeros() as u64
    };

    vcpu.set_reg(reg, result, op_size);

    // TZCNT flags: CF=1 if source is 0, ZF=1 if result is 0
    vcpu.regs.rflags &= !(flags::bits::CF | flags::bits::ZF);
    if value == 0 {
        vcpu.regs.rflags |= flags::bits::CF;
    }
    if result == 0 {
        vcpu.regs.rflags |= flags::bits::ZF;
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LZCNT r, r/m (F3 0F BD) - Count leading zeros
pub fn lzcnt(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    let bit_count = (op_size * 8) as u32;

    // For LZCNT, we count leading zeros within the operand size
    let result = if value == 0 {
        bit_count as u64
    } else {
        // We need to count leading zeros for the specific operand size
        let leading_zeros = match op_size {
            2 => (value as u16).leading_zeros(),
            4 => (value as u32).leading_zeros(),
            8 => value.leading_zeros(),
            _ => value.leading_zeros(),
        };
        leading_zeros as u64
    };

    vcpu.set_reg(reg, result, op_size);

    // LZCNT flags: CF=1 if source is 0, ZF=1 if result is 0
    vcpu.regs.rflags &= !(flags::bits::CF | flags::bits::ZF);
    if value == 0 {
        vcpu.regs.rflags |= flags::bits::CF;
    }
    if result == 0 {
        vcpu.regs.rflags |= flags::bits::ZF;
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// POPCNT r, r/m (F3 0F B8)
pub fn popcnt(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    let value = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    let count = match op_size {
        2 => (value as u16).count_ones() as u64,
        4 => (value as u32).count_ones() as u64,
        8 => value.count_ones() as u64,
        _ => value.count_ones() as u64,
    };

    vcpu.set_reg(reg, count, op_size);

    // POPCNT clears OF, SF, AF, CF, PF and sets ZF if result is zero
    vcpu.regs.rflags &= !(flags::bits::OF | flags::bits::SF | flags::bits::AF | flags::bits::CF | flags::bits::PF | flags::bits::ZF);
    if count == 0 {
        vcpu.regs.rflags |= flags::bits::ZF;
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
