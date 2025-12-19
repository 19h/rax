//! Shift and rotate instructions: SHL, SHR, SAR, ROL, ROR.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;

/// Execute 8-bit shift/rotate operation.
fn execute_shift8(vcpu: &mut X86_64Vcpu, op: u8, val: u8, count: u8) -> Result<u8> {
    if count == 0 {
        return Ok(val);
    }
    let count = count & 0x1F;
    let cf_bit = flags::bits::CF;
    let of_bit = flags::bits::OF;
    let old_cf = (vcpu.regs.rflags & cf_bit) != 0;

    let (result, cf, of) = match op {
        0 => {
            // ROL
            let result = val.rotate_left(count as u32);
            let cf = (result & 1) != 0;
            let of = if count == 1 {
                ((result >> 7) ^ (result & 1)) != 0
            } else {
                false
            };
            (result, cf, of)
        }
        1 => {
            // ROR
            let result = val.rotate_right(count as u32);
            let cf = (result >> 7) != 0;
            let of = if count == 1 {
                ((result >> 7) ^ ((result >> 6) & 1)) != 0
            } else {
                false
            };
            (result, cf, of)
        }
        2 => {
            // RCL - Rotate through carry left
            // 9-bit value: [CF:val], rotate left by count
            let count = count % 9; // 9-bit rotation period
            if count == 0 {
                return Ok(val);
            }
            // Build 9-bit value: bit 8 = old CF, bits 0-7 = val
            let mut wide = ((old_cf as u16) << 8) | (val as u16);
            // Rotate left by count within 9 bits
            for _ in 0..count {
                let msb = (wide >> 8) & 1;
                wide = ((wide << 1) & 0x1FF) | msb;
            }
            let result = (wide & 0xFF) as u8;
            let new_cf = (wide >> 8) & 1 != 0;
            let of = if count == 1 {
                ((result >> 7) != 0) ^ new_cf
            } else {
                false
            };
            (result, new_cf, of)
        }
        3 => {
            // RCR - Rotate through carry right
            // 9-bit value: [CF:val], rotate right by count
            let count = count % 9; // 9-bit rotation period
            if count == 0 {
                return Ok(val);
            }
            // Build 9-bit value: bit 8 = old CF, bits 0-7 = val
            let mut wide = ((old_cf as u16) << 8) | (val as u16);
            // Rotate right by count within 9 bits
            for _ in 0..count {
                let lsb = wide & 1;
                wide = (wide >> 1) | (lsb << 8);
            }
            let result = (wide & 0xFF) as u8;
            let new_cf = (wide >> 8) & 1 != 0;
            let of = if count == 1 {
                ((result >> 7) ^ ((result >> 6) & 1)) != 0
            } else {
                false
            };
            (result, new_cf, of)
        }
        4 => {
            // SHL/SAL
            let result = if count >= 8 { 0 } else { val << count };
            let cf = if count > 0 && count <= 8 {
                (val >> (8 - count)) & 1 != 0
            } else {
                false
            };
            let of = if count == 1 {
                ((result >> 7) ^ (cf as u8)) != 0
            } else {
                false
            };
            (result, cf, of)
        }
        5 => {
            // SHR
            let result = if count >= 8 { 0 } else { val >> count };
            let cf = if count > 0 && count <= 8 {
                (val >> (count - 1)) & 1 != 0
            } else {
                false
            };
            let of = if count == 1 { (val >> 7) != 0 } else { false };
            (result, cf, of)
        }
        7 => {
            // SAR
            let result = if count >= 8 {
                if (val as i8) < 0 {
                    0xFF
                } else {
                    0
                }
            } else {
                ((val as i8) >> count) as u8
            };
            let cf = if count > 0 && count <= 8 {
                (val >> (count - 1)) & 1 != 0
            } else {
                false
            };
            let of = false;
            (result, cf, of)
        }
        _ => return Err(Error::Emulator(format!("unimplemented shift8 op: {}", op))),
    };

    // Update ZF, SF, PF first (this clears CF and OF)
    flags::update_flags_logic(&mut vcpu.regs.rflags, result as u64, 1);

    // Now set CF and OF based on the shift/rotate operation
    if cf {
        vcpu.regs.rflags |= cf_bit;
    } else {
        vcpu.regs.rflags &= !cf_bit;
    }
    if of {
        vcpu.regs.rflags |= of_bit;
    } else {
        vcpu.regs.rflags &= !of_bit;
    }

    Ok(result)
}

/// Execute shift/rotate operation for 16/32/64-bit operands.
fn execute_shift(vcpu: &mut X86_64Vcpu, op: u8, val: u64, count: u8, size: u8) -> Result<u64> {
    if count == 0 {
        return Ok(val);
    }
    let bits = size as u32 * 8;
    let mask = if bits == 64 {
        !0u64
    } else {
        (1u64 << bits) - 1
    };
    let cf_bit = flags::bits::CF;
    let of_bit = flags::bits::OF;
    let old_cf = (vcpu.regs.rflags & cf_bit) != 0;

    let (result, cf, of) = match op {
        0 => {
            // ROL
            let count = count as u32 % bits;
            let result = if count == 0 {
                val & mask
            } else {
                ((val << count) | (val >> (bits - count))) & mask
            };
            let cf = (result & 1) != 0;
            let of = if count == 1 {
                (((result >> (bits - 1)) ^ result) & 1) != 0
            } else {
                false
            };
            (result, cf, of)
        }
        1 => {
            // ROR
            let count = count as u32 % bits;
            let result = if count == 0 {
                val & mask
            } else {
                ((val >> count) | (val << (bits - count))) & mask
            };
            let cf = (result >> (bits - 1)) != 0;
            let of = if count == 1 {
                ((result >> (bits - 1)) ^ ((result >> (bits - 2)) & 1)) != 0
            } else {
                false
            };
            (result, cf, of)
        }
        2 => {
            // RCL - Rotate through carry left
            // (bits+1)-bit rotation through carry
            let rotate_size = bits + 1;
            let count = (count as u32) % rotate_size;
            if count == 0 {
                return Ok(val & mask);
            }

            // We need to perform a (bits+1)-bit rotation through carry
            // For simplicity, use a loop-based approach
            let mut result = val & mask;
            let mut carry = old_cf;

            for _ in 0..count {
                let msb = (result >> (bits - 1)) & 1 != 0;
                result = ((result << 1) | (carry as u64)) & mask;
                carry = msb;
            }

            let new_cf = carry;
            let of = if count == 1 {
                ((result >> (bits - 1)) & 1 != 0) ^ new_cf
            } else {
                false
            };
            (result, new_cf, of)
        }
        3 => {
            // RCR - Rotate through carry right
            // (bits+1)-bit rotation through carry
            let rotate_size = bits + 1;
            let count = (count as u32) % rotate_size;
            if count == 0 {
                return Ok(val & mask);
            }

            let mut result = val & mask;
            let mut carry = old_cf;

            for _ in 0..count {
                let lsb = result & 1 != 0;
                result = (result >> 1) | ((carry as u64) << (bits - 1));
                carry = lsb;
            }

            let new_cf = carry;
            let of = if count == 1 {
                // OF = MSB XOR (MSB-1)
                ((result >> (bits - 1)) ^ (result >> (bits - 2))) & 1 != 0
            } else {
                false
            };
            (result & mask, new_cf, of)
        }
        4 => {
            // SHL/SAL
            let result = if count as u32 >= bits {
                0
            } else {
                (val << count) & mask
            };
            let cf = if count > 0 && (count as u32) <= bits {
                (val >> (bits - count as u32)) & 1 != 0
            } else {
                false
            };
            let of = if count == 1 {
                ((result >> (bits - 1)) ^ (cf as u64)) != 0
            } else {
                false
            };
            (result, cf, of)
        }
        5 => {
            // SHR
            let result = if count as u32 >= bits {
                0
            } else {
                (val >> count) & mask
            };
            let cf = if count > 0 && (count as u32) <= bits {
                (val >> (count - 1)) & 1 != 0
            } else {
                false
            };
            let of = if count == 1 {
                (val >> (bits - 1)) != 0
            } else {
                false
            };
            (result, cf, of)
        }
        7 => {
            // SAR
            let result = if count as u32 >= bits {
                let sign = (val >> (bits - 1)) & 1;
                if sign != 0 {
                    mask
                } else {
                    0
                }
            } else {
                match size {
                    2 => (((val & 0xFFFF) as i16 >> count) as u16) as u64,
                    4 => (((val & 0xFFFF_FFFF) as i32 >> count) as u32) as u64,
                    8 => ((val as i64) >> count) as u64,
                    _ => val >> count,
                }
            };
            let cf = if count > 0 && (count as u32) <= bits {
                (val >> (count - 1)) & 1 != 0
            } else {
                false
            };
            let of = false;
            (result, cf, of)
        }
        _ => return Err(Error::Emulator(format!("unimplemented shift op: {}", op))),
    };

    // Update ZF, SF, PF first (this clears CF and OF)
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, size);

    // Now set CF and OF based on the shift/rotate operation
    if cf {
        vcpu.regs.rflags |= cf_bit;
    } else {
        vcpu.regs.rflags &= !cf_bit;
    }
    if of {
        vcpu.regs.rflags |= of_bit;
    } else {
        vcpu.regs.rflags &= !of_bit;
    }

    Ok(result)
}

/// Group 2: r/m8, imm8 (0xC0)
pub fn group2_rm8_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (val, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, 1) as u8, None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.mmu.read_u8(addr, &vcpu.sregs)?, Some(addr))
    };

    let count = ctx.consume_u8()? & 0x1F;
    let result = execute_shift8(vcpu, op, val, count)?;

    if let Some(addr) = addr_opt {
        vcpu.mmu.write_u8(addr, result, &vcpu.sregs)?;
    } else {
        vcpu.set_reg(rm, result as u64, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 2: r/m, imm8 (0xC1)
pub fn group2_rm_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (val, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, op_size), None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.read_mem(addr, op_size)?, Some(addr))
    };

    // Mask count: 6 bits for 64-bit ops, 5 bits for 16/32-bit ops
    let count_mask = if op_size == 8 { 0x3F } else { 0x1F };
    let count = ctx.consume_u8()? & count_mask;
    let result = execute_shift(vcpu, op, val, count, op_size)?;

    if let Some(addr) = addr_opt {
        vcpu.write_mem(addr, result, op_size)?;
    } else {
        vcpu.set_reg(rm, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 2: r/m8, 1 (0xD0)
pub fn group2_rm8_1(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (val, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, 1) as u8, None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.mmu.read_u8(addr, &vcpu.sregs)?, Some(addr))
    };

    let result = execute_shift8(vcpu, op, val, 1)?;

    if let Some(addr) = addr_opt {
        vcpu.mmu.write_u8(addr, result, &vcpu.sregs)?;
    } else {
        vcpu.set_reg(rm, result as u64, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 2: r/m, 1 (0xD1)
pub fn group2_rm_1(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    let (val, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, op_size), None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.read_mem(addr, op_size)?, Some(addr))
    };

    let result = execute_shift(vcpu, op, val, 1, op_size)?;

    if let Some(addr) = addr_opt {
        vcpu.write_mem(addr, result, op_size)?;
    } else {
        vcpu.set_reg(rm, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 2: r/m8, CL (0xD2)
pub fn group2_rm8_cl(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let count = (vcpu.regs.rcx & 0x1F) as u8;

    let (val, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, 1) as u8, None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.mmu.read_u8(addr, &vcpu.sregs)?, Some(addr))
    };

    let result = execute_shift8(vcpu, op, val, count)?;

    if let Some(addr) = addr_opt {
        vcpu.mmu.write_u8(addr, result, &vcpu.sregs)?;
    } else {
        vcpu.set_reg(rm, result as u64, 1);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Group 2: r/m, CL (0xD3)
pub fn group2_rm_cl(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    // Mask count: 6 bits for 64-bit ops, 5 bits for 16/32-bit ops
    let count_mask = if op_size == 8 { 0x3F } else { 0x1F };
    let count = (vcpu.regs.rcx & count_mask) as u8;

    let (val, addr_opt) = if modrm >> 6 == 3 {
        (vcpu.get_reg(rm, op_size), None)
    } else {
        let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
        ctx.cursor = modrm_start + 1 + extra;
        (vcpu.read_mem(addr, op_size)?, Some(addr))
    };

    let result = execute_shift(vcpu, op, val, count, op_size)?;

    if let Some(addr) = addr_opt {
        vcpu.write_mem(addr, result, op_size)?;
    } else {
        vcpu.set_reg(rm, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SHLD r/m, r, imm8 (0x0F 0xA4) - Double precision shift left
pub fn shld_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    // Mask count: 6 bits for 64-bit ops, 5 bits for 16/32-bit ops
    let count_mask = if op_size == 8 { 0x3F } else { 0x1F };
    let count = ctx.consume_u8()? & count_mask;

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let src = vcpu.get_reg(reg, op_size);

    let result = execute_shld(vcpu, dst, src, count, op_size);

    if is_memory {
        vcpu.write_mem(addr, result, op_size)?;
    } else {
        vcpu.set_reg(rm, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SHLD r/m, r, CL (0x0F 0xA5) - Double precision shift left
pub fn shld_cl(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    // Mask count: 6 bits for 64-bit ops, 5 bits for 16/32-bit ops
    let count_mask = if op_size == 8 { 0x3F } else { 0x1F };
    let count = (vcpu.regs.rcx & count_mask) as u8;

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let src = vcpu.get_reg(reg, op_size);

    let result = execute_shld(vcpu, dst, src, count, op_size);

    if is_memory {
        vcpu.write_mem(addr, result, op_size)?;
    } else {
        vcpu.set_reg(rm, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SHRD r/m, r, imm8 (0x0F 0xAC) - Double precision shift right
pub fn shrd_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    // Mask count: 6 bits for 64-bit ops, 5 bits for 16/32-bit ops
    let count_mask = if op_size == 8 { 0x3F } else { 0x1F };
    let count = ctx.consume_u8()? & count_mask;

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let src = vcpu.get_reg(reg, op_size);

    let result = execute_shrd(vcpu, dst, src, count, op_size);

    if is_memory {
        vcpu.write_mem(addr, result, op_size)?;
    } else {
        vcpu.set_reg(rm, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SHRD r/m, r, CL (0x0F 0xAD) - Double precision shift right
pub fn shrd_cl(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    // Mask count: 6 bits for 64-bit ops, 5 bits for 16/32-bit ops
    let count_mask = if op_size == 8 { 0x3F } else { 0x1F };
    let count = (vcpu.regs.rcx & count_mask) as u8;

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };
    let src = vcpu.get_reg(reg, op_size);

    let result = execute_shrd(vcpu, dst, src, count, op_size);

    if is_memory {
        vcpu.write_mem(addr, result, op_size)?;
    } else {
        vcpu.set_reg(rm, result, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// Execute SHLD: shift dst left, filling in from src
fn execute_shld(vcpu: &mut X86_64Vcpu, dst: u64, src: u64, count: u8, size: u8) -> u64 {
    let bits = (size * 8) as u32;
    let mask = if bits == 64 { !0u64 } else { (1u64 << bits) - 1 };
    let count = (count as u32) % bits;

    if count == 0 {
        return dst & mask;
    }

    // SHLD: shift dst left by count, bring in high bits from src
    // Result = (dst << count) | (src >> (bits - count))
    let result = ((dst << count) | (src >> (bits - count))) & mask;

    // Update flags
    // CF = last bit shifted out of dst
    let cf = if count <= bits {
        (dst >> (bits - count)) & 1 != 0
    } else {
        false
    };

    // OF = sign bit changed (only defined for count == 1)
    let of = if count == 1 {
        ((result >> (bits - 1)) ^ (dst >> (bits - 1))) & 1 != 0
    } else {
        false
    };

    if cf {
        vcpu.regs.rflags |= flags::bits::CF;
    } else {
        vcpu.regs.rflags &= !flags::bits::CF;
    }
    if of {
        vcpu.regs.rflags |= flags::bits::OF;
    } else {
        vcpu.regs.rflags &= !flags::bits::OF;
    }
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, size);

    result
}

/// Execute SHRD: shift dst right, filling in from src
fn execute_shrd(vcpu: &mut X86_64Vcpu, dst: u64, src: u64, count: u8, size: u8) -> u64 {
    let bits = (size * 8) as u32;
    let mask = if bits == 64 { !0u64 } else { (1u64 << bits) - 1 };
    let count = (count as u32) % bits;

    if count == 0 {
        return dst & mask;
    }

    // SHRD: shift dst right by count, bring in low bits from src
    // Result = (dst >> count) | (src << (bits - count))
    let result = ((dst >> count) | ((src & mask) << (bits - count))) & mask;

    // Update flags
    // CF = last bit shifted out of dst
    let cf = if count > 0 && count <= bits {
        (dst >> (count - 1)) & 1 != 0
    } else {
        false
    };

    // OF = sign bit changed (only defined for count == 1)
    let of = if count == 1 {
        ((result >> (bits - 1)) ^ (dst >> (bits - 1))) & 1 != 0
    } else {
        false
    };

    if cf {
        vcpu.regs.rflags |= flags::bits::CF;
    } else {
        vcpu.regs.rflags &= !flags::bits::CF;
    }
    if of {
        vcpu.regs.rflags |= flags::bits::OF;
    } else {
        vcpu.regs.rflags &= !flags::bits::OF;
    }
    flags::update_flags_logic(&mut vcpu.regs.rflags, result, size);

    result
}
