//! Bit test instructions: BT, BTS, BTR, BTC.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

/// BT r/m, r (0x0F 0xA3)
/// For memory operands, the bit offset can extend beyond the operand size,
/// causing the effective address to be adjusted.
pub fn bt_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size) as i64; // Signed for negative offsets

    let (value, bit_pos) = if is_memory {
        // For memory operands, adjust address based on bit offset
        let op_bits = (op_size * 8) as i64;
        // Calculate address adjustment (signed division for negative offsets)
        let addr_adjust = if bit_offset >= 0 {
            (bit_offset / op_bits) * op_size as i64
        } else {
            // For negative offsets, round towards negative infinity
            ((bit_offset - op_bits + 1) / op_bits) * op_size as i64
        };
        let effective_addr = (addr as i64).wrapping_add(addr_adjust) as u64;
        let bit_pos = bit_offset.rem_euclid(op_bits) as u64;
        (vcpu.read_mem(effective_addr, op_size)?, bit_pos)
    } else {
        let bit_pos = (bit_offset as u64) & ((op_size * 8 - 1) as u64);
        (vcpu.get_reg(rm, op_size), bit_pos)
    };

    let cf_bit = (value >> bit_pos) & 1;
    if cf_bit != 0 {
        vcpu.regs.rflags |= flags::bits::CF;
    } else {
        vcpu.regs.rflags &= !flags::bits::CF;
    }
    // Clear lazy flags since we explicitly set CF
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BTS r/m, r (0x0F 0xAB) - Bit Test and Set
/// For memory operands, the bit offset can extend beyond the operand size.
pub fn bts_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size) as i64;

    if is_memory {
        let op_bits = (op_size * 8) as i64;
        let addr_adjust = if bit_offset >= 0 {
            (bit_offset / op_bits) * op_size as i64
        } else {
            ((bit_offset - op_bits + 1) / op_bits) * op_size as i64
        };
        let effective_addr = (addr as i64).wrapping_add(addr_adjust) as u64;
        let bit_pos = bit_offset.rem_euclid(op_bits) as u64;
        let v = vcpu.read_mem(effective_addr, op_size)?;
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v | (1 << bit_pos);
        vcpu.write_mem(effective_addr, new_val, op_size)?;
    } else {
        let bit_pos = (bit_offset as u64) & ((op_size * 8 - 1) as u64);
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
    // Clear lazy flags since we explicitly set CF
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BTR r/m, r (0x0F 0xB3) - Bit Test and Reset
/// For memory operands, the bit offset can extend beyond the operand size.
pub fn btr_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size) as i64;

    if is_memory {
        let op_bits = (op_size * 8) as i64;
        let addr_adjust = if bit_offset >= 0 {
            (bit_offset / op_bits) * op_size as i64
        } else {
            ((bit_offset - op_bits + 1) / op_bits) * op_size as i64
        };
        let effective_addr = (addr as i64).wrapping_add(addr_adjust) as u64;
        let bit_pos = bit_offset.rem_euclid(op_bits) as u64;
        let v = vcpu.read_mem(effective_addr, op_size)?;
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v & !(1 << bit_pos);
        vcpu.write_mem(effective_addr, new_val, op_size)?;
    } else {
        let bit_pos = (bit_offset as u64) & ((op_size * 8 - 1) as u64);
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
    // Clear lazy flags since we explicitly set CF
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// BTC r/m, r (0x0F 0xBB) - Bit Test and Complement
/// For memory operands, the bit offset can extend beyond the operand size.
pub fn btc_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let bit_offset = vcpu.get_reg(reg, op_size) as i64;

    if is_memory {
        let op_bits = (op_size * 8) as i64;
        let addr_adjust = if bit_offset >= 0 {
            (bit_offset / op_bits) * op_size as i64
        } else {
            ((bit_offset - op_bits + 1) / op_bits) * op_size as i64
        };
        let effective_addr = (addr as i64).wrapping_add(addr_adjust) as u64;
        let bit_pos = bit_offset.rem_euclid(op_bits) as u64;
        let v = vcpu.read_mem(effective_addr, op_size)?;
        let cf_bit = (v >> bit_pos) & 1;
        if cf_bit != 0 {
            vcpu.regs.rflags |= flags::bits::CF;
        } else {
            vcpu.regs.rflags &= !flags::bits::CF;
        }
        let new_val = v ^ (1 << bit_pos);
        vcpu.write_mem(effective_addr, new_val, op_size)?;
    } else {
        let bit_pos = (bit_offset as u64) & ((op_size * 8 - 1) as u64);
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
    // Clear lazy flags since we explicitly set CF
    vcpu.clear_lazy_flags();
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
    // Clear lazy flags since we explicitly set CF
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
