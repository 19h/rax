//! Opcode group instructions: Group 4 (0xFE), Group 5 (0xFF).

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

/// Group 4: INC/DEC r/m8 (0xFE)
pub fn group4(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();

    match op {
        0 => {
            // INC r/m8
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, 1);
                let result = (val as u8).wrapping_add(1) as u64;
                vcpu.set_reg(rm, result, 1);
                // INC preserves CF
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_add(&mut vcpu.regs.rflags, val, 1, result, 1);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
                let result = (val as u8).wrapping_add(1) as u64;
                vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_add(&mut vcpu.regs.rflags, val, 1, result, 1);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            }
            vcpu.regs.rip += ctx.cursor as u64;
        }
        1 => {
            // DEC r/m8
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, 1);
                let result = (val as u8).wrapping_sub(1) as u64;
                vcpu.set_reg(rm, result, 1);
                // DEC preserves CF
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_sub(&mut vcpu.regs.rflags, val, 1, result, 1);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.mmu.read_u8(addr, &vcpu.sregs)? as u64;
                let result = (val as u8).wrapping_sub(1) as u64;
                vcpu.mmu.write_u8(addr, result as u8, &vcpu.sregs)?;
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_sub(&mut vcpu.regs.rflags, val, 1, result, 1);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            }
            vcpu.regs.rip += ctx.cursor as u64;
        }
        _ => {
            return Err(Error::Emulator(format!("unimplemented 0xFE /{}", op)));
        }
    }
    Ok(None)
}

/// Group 5: INC/DEC/CALL/JMP/PUSH (0xFF)
pub fn group5(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    let op = (modrm >> 3) & 0x07;
    let rm = (modrm & 0x07) | ctx.rex_b();
    let op_size = ctx.op_size;

    match op {
        0 => {
            // INC r/m - preserves CF
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, op_size);
                let result = val.wrapping_add(1);
                vcpu.set_reg(rm, result, op_size);
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_add(&mut vcpu.regs.rflags, val, 1, result, op_size);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.read_mem(addr, op_size)?;
                let result = val.wrapping_add(1);
                vcpu.write_mem(addr, result, op_size)?;
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_add(&mut vcpu.regs.rflags, val, 1, result, op_size);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            }
            vcpu.regs.rip += ctx.cursor as u64;
        }
        1 => {
            // DEC r/m - preserves CF
            if modrm >> 6 == 3 {
                let val = vcpu.get_reg(rm, op_size);
                let result = val.wrapping_sub(1);
                vcpu.set_reg(rm, result, op_size);
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_sub(&mut vcpu.regs.rflags, val, 1, result, op_size);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                let val = vcpu.read_mem(addr, op_size)?;
                let result = val.wrapping_sub(1);
                vcpu.write_mem(addr, result, op_size)?;
                let cf = vcpu.regs.rflags & flags::bits::CF;
                flags::update_flags_sub(&mut vcpu.regs.rflags, val, 1, result, op_size);
                vcpu.regs.rflags = (vcpu.regs.rflags & !flags::bits::CF) | cf;
            }
            vcpu.regs.rip += ctx.cursor as u64;
        }
        2 => {
            // CALL r/m64
            let target = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, 8)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, 8)?
            };
            let ret_addr = vcpu.regs.rip + ctx.cursor as u64;
            vcpu.push64(ret_addr)?;
            vcpu.regs.rip = target;
        }
        3 => {
            // CALL FAR m16:16/m16:32/m16:64
            if modrm >> 6 == 3 {
                return Err(Error::Emulator("CALL FAR requires memory operand".to_string()));
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;

            // Determine offset size using inverted logic (legacy compatibility)
            let offset_size: u8 = if ctx.rex_w() {
                8
            } else if ctx.operand_size_override {
                4
            } else {
                2
            };

            // Read offset and selector from memory
            let offset = vcpu.read_mem(addr, offset_size)?;
            let selector = vcpu.mmu.read_u16(addr + offset_size as u64, &vcpu.sregs)?;

            // Push return CS:IP
            let old_cs = vcpu.get_sreg(1);
            let ret_addr = vcpu.regs.rip + ctx.cursor as u64;

            vcpu.push64(old_cs as u64)?;
            vcpu.push64(ret_addr)?;

            // Load new CS:IP
            vcpu.set_sreg(1, selector);
            vcpu.regs.rip = offset;
        }
        4 => {
            // JMP r/m64
            let target = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, 8)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, 8)?
            };
            vcpu.regs.rip = target;
        }
        5 => {
            // JMP FAR m16:16/m16:32/m16:64
            if modrm >> 6 == 3 {
                return Err(Error::Emulator("JMP FAR requires memory operand".to_string()));
            }
            let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;

            // Determine offset size using inverted logic (legacy compatibility)
            let offset_size: u8 = if ctx.rex_w() {
                8
            } else if ctx.operand_size_override {
                4
            } else {
                2
            };

            // Read offset and selector from memory
            let offset = vcpu.read_mem(addr, offset_size)?;
            let selector = vcpu.mmu.read_u16(addr + offset_size as u64, &vcpu.sregs)?;

            // Load new CS:IP
            vcpu.set_sreg(1, selector);
            vcpu.regs.rip = offset;
        }
        6 => {
            // PUSH r/m64
            let val = if modrm >> 6 == 3 {
                vcpu.get_reg(rm, 8)
            } else {
                let (addr, extra) = vcpu.decode_modrm_addr(ctx, modrm_start)?;
                ctx.cursor = modrm_start + 1 + extra;
                vcpu.read_mem(addr, 8)?
            };
            vcpu.push64(val)?;
            vcpu.regs.rip += ctx.cursor as u64;
        }
        _ => {
            return Err(Error::Emulator(format!("unimplemented 0xFF /{}", op)));
        }
    }
    Ok(None)
}
