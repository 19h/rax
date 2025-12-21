//! x87 FPU instruction implementations.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};

/// D8 escape - FADD, FMUL, FCOM, FCOMP, FSUB, FSUBR, FDIV, FDIVR
pub fn escape_d8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        // Memory operand - float32
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        let val = vcpu.read_f32(addr)?;
        match reg {
            0 => { // FADD m32
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0 + val as f64);
            }
            1 => { // FMUL m32
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0 * val as f64);
            }
            2 => { // FCOM m32
                let st0 = vcpu.fpu.get_st(0);
                set_fpu_compare_flags(vcpu, st0, val as f64);
            }
            3 => { // FCOMP m32
                let st0 = vcpu.fpu.get_st(0);
                set_fpu_compare_flags(vcpu, st0, val as f64);
                vcpu.fpu.pop();
            }
            4 => { // FSUB m32
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0 - val as f64);
            }
            5 => { // FSUBR m32
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, val as f64 - st0);
            }
            6 => { // FDIV m32
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0 / val as f64);
            }
            7 => { // FDIVR m32
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, val as f64 / st0);
            }
            _ => unreachable!(),
        }
    } else {
        // Register operand - ST(i)
        let sti = vcpu.fpu.get_st(rm);
        let st0 = vcpu.fpu.get_st(0);
        match reg {
            0 => vcpu.fpu.set_st(0, st0 + sti), // FADD ST(0), ST(i)
            1 => vcpu.fpu.set_st(0, st0 * sti), // FMUL ST(0), ST(i)
            2 => set_fpu_compare_flags(vcpu, st0, sti), // FCOM ST(i)
            3 => { // FCOMP ST(i)
                set_fpu_compare_flags(vcpu, st0, sti);
                vcpu.fpu.pop();
            }
            4 => vcpu.fpu.set_st(0, st0 - sti), // FSUB ST(0), ST(i)
            5 => vcpu.fpu.set_st(0, sti - st0), // FSUBR ST(0), ST(i)
            6 => vcpu.fpu.set_st(0, st0 / sti), // FDIV ST(0), ST(i)
            7 => vcpu.fpu.set_st(0, sti / st0), // FDIVR ST(0), ST(i)
            _ => unreachable!(),
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// D9 escape - FLD, FST, FSTP, FLDENV, FLDCW, FNSTENV, FNSTCW, etc.
pub fn escape_d9(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        match reg {
            0 => { // FLD m32
                let val = vcpu.read_f32(addr)?;
                vcpu.fpu.push(val as f64);
            }
            2 => { // FST m32
                let val = vcpu.fpu.get_st(0) as f32;
                vcpu.write_f32(addr, val)?;
            }
            3 => { // FSTP m32
                let val = vcpu.fpu.pop() as f32;
                vcpu.write_f32(addr, val)?;
            }
            4 => { // FLDENV m14/28byte
                fldenv(vcpu, addr)?;
            }
            5 => { // FLDCW m16
                let cw = vcpu.read_mem16(addr)?;
                vcpu.fpu.control_word = cw;
            }
            6 => { // FNSTENV m14/28byte
                fnstenv(vcpu, addr)?;
            }
            7 => { // FNSTCW m16
                vcpu.write_mem16(addr, vcpu.fpu.control_word)?;
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented D9 memory opcode reg={} at RIP={:#x}",
                    reg, vcpu.regs.rip
                )));
            }
        }
    } else {
        // Register or special opcodes
        match modrm {
            0xC0..=0xC7 => { // FLD ST(i)
                let val = vcpu.fpu.get_st(rm);
                vcpu.fpu.push(val);
            }
            0xC8..=0xCF => { // FXCH ST(i)
                let st0 = vcpu.fpu.get_st(0);
                let sti = vcpu.fpu.get_st(rm);
                vcpu.fpu.set_st(0, sti);
                vcpu.fpu.set_st(rm, st0);
            }
            0xD0 => {} // FNOP
            0xE0 => { // FCHS
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, -st0);
            }
            0xE1 => { // FABS
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0.abs());
            }
            0xE4 => { // FTST
                let st0 = vcpu.fpu.get_st(0);
                set_fpu_compare_flags(vcpu, st0, 0.0);
            }
            0xE5 => { // FXAM
                fxam(vcpu);
            }
            0xE8 => vcpu.fpu.push(1.0), // FLD1
            0xE9 => vcpu.fpu.push(std::f64::consts::LOG2_10), // FLDL2T
            0xEA => vcpu.fpu.push(std::f64::consts::LOG2_E), // FLDL2E
            0xEB => vcpu.fpu.push(std::f64::consts::PI), // FLDPI
            0xEC => vcpu.fpu.push(std::f64::consts::LN_2 / std::f64::consts::LN_10), // FLDLG2
            0xED => vcpu.fpu.push(std::f64::consts::LN_2), // FLDLN2
            0xEE => vcpu.fpu.push(0.0), // FLDZ
            0xF0 => { // F2XM1
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, (2.0_f64).powf(st0) - 1.0);
            }
            0xF1 => { // FYL2X
                let st0 = vcpu.fpu.pop();
                let st1 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st1 * st0.log2());
            }
            0xF2 => { // FPTAN
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0.tan());
                vcpu.fpu.push(1.0);
            }
            0xF3 => { // FPATAN
                let st0 = vcpu.fpu.pop();
                let st1 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st1.atan2(st0));
            }
            0xF4 => { // FXTRACT
                let st0 = vcpu.fpu.get_st(0);
                let (mantissa, exponent, _) = decode_f64(st0);
                vcpu.fpu.set_st(0, (exponent - 1023) as f64); // exponent
                vcpu.fpu.push(mantissa); // significand
            }
            0xF5 => { // FPREM1
                let st0 = vcpu.fpu.get_st(0);
                let st1 = vcpu.fpu.get_st(1);
                vcpu.fpu.set_st(0, st0 % st1);
                // C2 = 0 (complete)
                vcpu.fpu.status_word &= !0x0400;
            }
            0xF6 => { // FDECSTP
                vcpu.fpu.top = vcpu.fpu.top.wrapping_sub(1) & 7;
                vcpu.fpu.status_word = (vcpu.fpu.status_word & !0x3800) | ((vcpu.fpu.top as u16) << 11);
            }
            0xF7 => { // FINCSTP
                vcpu.fpu.top = vcpu.fpu.top.wrapping_add(1) & 7;
                vcpu.fpu.status_word = (vcpu.fpu.status_word & !0x3800) | ((vcpu.fpu.top as u16) << 11);
            }
            0xF8 => { // FPREM
                let st0 = vcpu.fpu.get_st(0);
                let st1 = vcpu.fpu.get_st(1);
                vcpu.fpu.set_st(0, st0 % st1);
                // C2 = 0 (complete)
                vcpu.fpu.status_word &= !0x0400;
            }
            0xF9 => { // FYL2XP1
                let st0 = vcpu.fpu.pop();
                let st1 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st1 * (1.0 + st0).log2());
            }
            0xFA => { // FSQRT
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0.sqrt());
            }
            0xFB => { // FSINCOS
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0.sin());
                vcpu.fpu.push(st0.cos());
            }
            0xFC => { // FRNDINT
                let st0 = vcpu.fpu.get_st(0);
                let rounded = fpu_round(vcpu.fpu.control_word, st0);
                vcpu.fpu.set_st(0, rounded);
            }
            0xFD => { // FSCALE
                let st0 = vcpu.fpu.get_st(0);
                let st1 = vcpu.fpu.get_st(1);
                vcpu.fpu.set_st(0, st0 * (2.0_f64).powf(st1.trunc()));
            }
            0xFE => { // FSIN
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0.sin());
            }
            0xFF => { // FCOS
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(0, st0.cos());
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented D9 opcode modrm={:#x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
            }
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// DA escape - FIADD, FIMUL, FICOM, FICOMP, FISUB, FISUBR, FIDIV, FIDIVR (dword int)
pub fn escape_da(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        let val = vcpu.read_mem32(addr)? as i32 as f64;
        let st0 = vcpu.fpu.get_st(0);
        match reg {
            0 => vcpu.fpu.set_st(0, st0 + val), // FIADD m32int
            1 => vcpu.fpu.set_st(0, st0 * val), // FIMUL m32int
            2 => set_fpu_compare_flags(vcpu, st0, val), // FICOM m32int
            3 => { // FICOMP m32int
                set_fpu_compare_flags(vcpu, st0, val);
                vcpu.fpu.pop();
            }
            4 => vcpu.fpu.set_st(0, st0 - val), // FISUB m32int
            5 => vcpu.fpu.set_st(0, val - st0), // FISUBR m32int
            6 => vcpu.fpu.set_st(0, st0 / val), // FIDIV m32int
            7 => vcpu.fpu.set_st(0, val / st0), // FIDIVR m32int
            _ => unreachable!(),
        }
    } else {
        // Register forms - FCMOV
        let st0 = vcpu.fpu.get_st(0);
        let sti = vcpu.fpu.get_st(rm);
        let rflags = vcpu.regs.rflags;
        let cf = rflags & 1 != 0;
        let zf = rflags & 0x40 != 0;
        let pf = rflags & 4 != 0;

        match reg {
            0 if cf => vcpu.fpu.set_st(0, sti), // FCMOVB
            1 if zf => vcpu.fpu.set_st(0, sti), // FCMOVE
            2 if cf || zf => vcpu.fpu.set_st(0, sti), // FCMOVBE
            3 if pf => vcpu.fpu.set_st(0, sti), // FCMOVU
            _ => {} // condition not met
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// DB escape - FILD, FIST, FISTP, FCLEX, FINIT, etc.
pub fn escape_db(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        match reg {
            0 => { // FILD m32int
                let val = vcpu.read_mem32(addr)? as i32 as f64;
                vcpu.fpu.push(val);
            }
            1 => { // FISTTP m32int
                let val = vcpu.fpu.pop().trunc() as i32;
                vcpu.write_mem32(addr, val as u32)?;
            }
            2 => { // FIST m32int
                let val = fpu_round(vcpu.fpu.control_word, vcpu.fpu.get_st(0)) as i32;
                vcpu.write_mem32(addr, val as u32)?;
            }
            3 => { // FISTP m32int
                let val = fpu_round(vcpu.fpu.control_word, vcpu.fpu.pop()) as i32;
                vcpu.write_mem32(addr, val as u32)?;
            }
            5 => { // FLD m80fp (extended precision)
                let bytes = vcpu.read_bytes(addr, 10)?;
                let val = f80_to_f64(&bytes);
                vcpu.fpu.push(val);
            }
            7 => { // FSTP m80fp (extended precision)
                let val = vcpu.fpu.pop();
                let bytes = f64_to_f80(val);
                vcpu.write_bytes(addr, &bytes)?;
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented DB memory opcode reg={} at RIP={:#x}",
                    reg, vcpu.regs.rip
                )));
            }
        }
    } else {
        match modrm {
            0xC0..=0xC7 => { // FCMOVNB ST(0), ST(i)
                if vcpu.regs.rflags & 1 == 0 {
                    let sti = vcpu.fpu.get_st(rm);
                    vcpu.fpu.set_st(0, sti);
                }
            }
            0xC8..=0xCF => { // FCMOVNE ST(0), ST(i)
                if vcpu.regs.rflags & 0x40 == 0 {
                    let sti = vcpu.fpu.get_st(rm);
                    vcpu.fpu.set_st(0, sti);
                }
            }
            0xD0..=0xD7 => { // FCMOVNBE ST(0), ST(i)
                if vcpu.regs.rflags & 0x41 == 0 {
                    let sti = vcpu.fpu.get_st(rm);
                    vcpu.fpu.set_st(0, sti);
                }
            }
            0xD8..=0xDF => { // FCMOVNU ST(0), ST(i)
                if vcpu.regs.rflags & 4 == 0 {
                    let sti = vcpu.fpu.get_st(rm);
                    vcpu.fpu.set_st(0, sti);
                }
            }
            0xE2 => { // FNCLEX - clear exceptions
                vcpu.fpu.status_word &= !0x80FF; // Clear exception flags and ES
            }
            0xE3 => { // FNINIT - initialize FPU
                vcpu.fpu.init();
            }
            0xE8..=0xEF => { // FUCOMI ST(0), ST(i)
                let st0 = vcpu.fpu.get_st(0);
                let sti = vcpu.fpu.get_st(rm);
                set_fcomi_flags(vcpu, st0, sti);
            }
            0xF0..=0xF7 => { // FCOMI ST(0), ST(i)
                let st0 = vcpu.fpu.get_st(0);
                let sti = vcpu.fpu.get_st(rm);
                set_fcomi_flags(vcpu, st0, sti);
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented DB opcode modrm={:#x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
            }
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// DC escape - FADD, FMUL, FCOM, FCOMP, FSUB, FSUBR, FDIV, FDIVR (m64)
pub fn escape_dc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        let val = vcpu.read_f64(addr)?;
        let st0 = vcpu.fpu.get_st(0);
        match reg {
            0 => vcpu.fpu.set_st(0, st0 + val), // FADD m64
            1 => vcpu.fpu.set_st(0, st0 * val), // FMUL m64
            2 => set_fpu_compare_flags(vcpu, st0, val), // FCOM m64
            3 => { // FCOMP m64
                set_fpu_compare_flags(vcpu, st0, val);
                vcpu.fpu.pop();
            }
            4 => vcpu.fpu.set_st(0, st0 - val), // FSUB m64
            5 => vcpu.fpu.set_st(0, val - st0), // FSUBR m64
            6 => vcpu.fpu.set_st(0, st0 / val), // FDIV m64
            7 => vcpu.fpu.set_st(0, val / st0), // FDIVR m64
            _ => unreachable!(),
        }
    } else {
        // Register form: op ST(i), ST(0)
        let st0 = vcpu.fpu.get_st(0);
        let sti = vcpu.fpu.get_st(rm);
        match reg {
            0 => vcpu.fpu.set_st(rm, sti + st0), // FADD ST(i), ST(0)
            1 => vcpu.fpu.set_st(rm, sti * st0), // FMUL ST(i), ST(0)
            4 => vcpu.fpu.set_st(rm, sti - st0), // FSUB ST(i), ST(0)
            5 => vcpu.fpu.set_st(rm, st0 - sti), // FSUBR ST(i), ST(0)
            6 => vcpu.fpu.set_st(rm, sti / st0), // FDIV ST(i), ST(0)
            7 => vcpu.fpu.set_st(rm, st0 / sti), // FDIVR ST(i), ST(0)
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented DC register opcode reg={} at RIP={:#x}",
                    reg, vcpu.regs.rip
                )));
            }
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// DD escape - FLD, FST, FSTP, FRSTOR, FNSAVE, FUCOM, FUCOMP, FFREE
pub fn escape_dd(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        match reg {
            0 => { // FLD m64
                let val = vcpu.read_f64(addr)?;
                vcpu.fpu.push(val);
            }
            1 => { // FISTTP m64int
                let val = vcpu.fpu.pop().trunc() as i64;
                vcpu.write_mem64(addr, val as u64)?;
            }
            2 => { // FST m64
                let val = vcpu.fpu.get_st(0);
                vcpu.write_f64(addr, val)?;
            }
            3 => { // FSTP m64
                let val = vcpu.fpu.pop();
                vcpu.write_f64(addr, val)?;
            }
            4 => { // FRSTOR m94/108byte
                frstor(vcpu, addr)?;
            }
            6 => { // FNSAVE m94/108byte
                fnsave(vcpu, addr)?;
            }
            7 => { // FNSTSW m16
                vcpu.write_mem16(addr, vcpu.fpu.status_word)?;
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented DD memory opcode reg={} at RIP={:#x}",
                    reg, vcpu.regs.rip
                )));
            }
        }
    } else {
        match modrm {
            0xC0..=0xC7 => { // FFREE ST(i)
                let tag_shift = (vcpu.fpu.st_index(rm) as u16) * 2;
                vcpu.fpu.tag_word |= 3 << tag_shift; // Mark as empty
            }
            0xD0..=0xD7 => { // FST ST(i)
                let st0 = vcpu.fpu.get_st(0);
                vcpu.fpu.set_st(rm, st0);
            }
            0xD8..=0xDF => { // FSTP ST(i)
                let st0 = vcpu.fpu.pop();
                vcpu.fpu.set_st(rm.wrapping_sub(1) & 7, st0);
            }
            0xE0..=0xE7 => { // FUCOM ST(i)
                let st0 = vcpu.fpu.get_st(0);
                let sti = vcpu.fpu.get_st(rm);
                set_fpu_compare_flags(vcpu, st0, sti);
            }
            0xE8..=0xEF => { // FUCOMP ST(i)
                let st0 = vcpu.fpu.get_st(0);
                let sti = vcpu.fpu.get_st(rm);
                set_fpu_compare_flags(vcpu, st0, sti);
                vcpu.fpu.pop();
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented DD opcode modrm={:#x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
            }
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// DE escape - FADDP, FMULP, FCOMP, FSUBP, FSUBRP, FDIVP, FDIVRP
pub fn escape_de(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        // Integer operations with m16int
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        let val = vcpu.read_mem16(addr)? as i16 as f64;
        let st0 = vcpu.fpu.get_st(0);
        match reg {
            0 => vcpu.fpu.set_st(0, st0 + val), // FIADD m16int
            1 => vcpu.fpu.set_st(0, st0 * val), // FIMUL m16int
            2 => set_fpu_compare_flags(vcpu, st0, val), // FICOM m16int
            3 => { // FICOMP m16int
                set_fpu_compare_flags(vcpu, st0, val);
                vcpu.fpu.pop();
            }
            4 => vcpu.fpu.set_st(0, st0 - val), // FISUB m16int
            5 => vcpu.fpu.set_st(0, val - st0), // FISUBR m16int
            6 => vcpu.fpu.set_st(0, st0 / val), // FIDIV m16int
            7 => vcpu.fpu.set_st(0, val / st0), // FIDIVR m16int
            _ => unreachable!(),
        }
    } else {
        // Register forms with pop
        let st0 = vcpu.fpu.get_st(0);
        let sti = vcpu.fpu.get_st(rm);
        match modrm {
            0xC0..=0xC7 => { // FADDP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti + st0);
                vcpu.fpu.pop();
            }
            0xC8..=0xCF => { // FMULP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti * st0);
                vcpu.fpu.pop();
            }
            0xD9 => { // FCOMPP
                set_fpu_compare_flags(vcpu, st0, sti);
                vcpu.fpu.pop();
                vcpu.fpu.pop();
            }
            0xE0..=0xE7 => { // FSUBRP ST(i), ST(0)
                vcpu.fpu.set_st(rm, st0 - sti);
                vcpu.fpu.pop();
            }
            0xE8..=0xEF => { // FSUBP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti - st0);
                vcpu.fpu.pop();
            }
            0xF0..=0xF7 => { // FDIVRP ST(i), ST(0)
                vcpu.fpu.set_st(rm, st0 / sti);
                vcpu.fpu.pop();
            }
            0xF8..=0xFF => { // FDIVP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti / st0);
                vcpu.fpu.pop();
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented DE opcode modrm={:#x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
            }
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// DF escape - FILD, FIST, FISTP (word/qword), FBLD, FBSTP, FNSTSW AX
pub fn escape_df(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let modrm = ctx.consume_u8()?;
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let is_memory = (modrm >> 6) != 3;

    if is_memory {
        let addr = vcpu.decode_fpu_modrm_addr(ctx, modrm)?;
        match reg {
            0 => { // FILD m16int
                let val = vcpu.read_mem16(addr)? as i16 as f64;
                vcpu.fpu.push(val);
            }
            1 => { // FISTTP m16int
                let val = vcpu.fpu.pop().trunc() as i16;
                vcpu.write_mem16(addr, val as u16)?;
            }
            2 => { // FIST m16int
                let val = fpu_round(vcpu.fpu.control_word, vcpu.fpu.get_st(0)) as i16;
                vcpu.write_mem16(addr, val as u16)?;
            }
            3 => { // FISTP m16int
                let val = fpu_round(vcpu.fpu.control_word, vcpu.fpu.pop()) as i16;
                vcpu.write_mem16(addr, val as u16)?;
            }
            4 => { // FBLD m80bcd
                let bytes = vcpu.read_bytes(addr, 10)?;
                let val = bcd_to_f64(&bytes);
                vcpu.fpu.push(val);
            }
            5 => { // FILD m64int
                let val = vcpu.read_mem64(addr)? as i64 as f64;
                vcpu.fpu.push(val);
            }
            6 => { // FBSTP m80bcd
                let val = vcpu.fpu.pop();
                let bytes = f64_to_bcd(val);
                vcpu.write_bytes(addr, &bytes)?;
            }
            7 => { // FISTP m64int
                let val = fpu_round(vcpu.fpu.control_word, vcpu.fpu.pop()) as i64;
                vcpu.write_mem64(addr, val as u64)?;
            }
            _ => unreachable!(),
        }
    } else {
        match modrm {
            0xE0 => { // FNSTSW AX
                vcpu.regs.rax = (vcpu.regs.rax & !0xFFFF) | vcpu.fpu.status_word as u64;
            }
            0xE8..=0xEF => { // FUCOMIP ST(0), ST(i)
                let st0 = vcpu.fpu.get_st(0);
                let sti = vcpu.fpu.get_st(rm);
                set_fcomi_flags(vcpu, st0, sti);
                vcpu.fpu.pop();
            }
            0xF0..=0xF7 => { // FCOMIP ST(0), ST(i)
                let st0 = vcpu.fpu.get_st(0);
                let sti = vcpu.fpu.get_st(rm);
                set_fcomi_flags(vcpu, st0, sti);
                vcpu.fpu.pop();
            }
            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented DF opcode modrm={:#x} at RIP={:#x}",
                    modrm, vcpu.regs.rip
                )));
            }
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

// Helper functions

/// Set FPU condition codes for comparison (C3, C2, C0)
fn set_fpu_compare_flags(vcpu: &mut X86_64Vcpu, a: f64, b: f64) {
    // C3 C2 C0 meaning:
    // 0  0  0  ST(0) > source
    // 0  0  1  ST(0) < source
    // 1  0  0  ST(0) = source
    // 1  1  1  unordered (NaN)
    let (c3, c2, c0) = if a.is_nan() || b.is_nan() {
        (true, true, true)
    } else if a > b {
        (false, false, false)
    } else if a < b {
        (false, false, true)
    } else {
        (true, false, false)
    };

    vcpu.fpu.status_word &= !0x4500; // Clear C3, C2, C0
    if c3 { vcpu.fpu.status_word |= 0x4000; }
    if c2 { vcpu.fpu.status_word |= 0x0400; }
    if c0 { vcpu.fpu.status_word |= 0x0100; }
}

/// Set RFLAGS for FCOMI/FUCOMI instructions
fn set_fcomi_flags(vcpu: &mut X86_64Vcpu, a: f64, b: f64) {
    // ZF PF CF meaning:
    // 0  0  0  ST(0) > ST(i)
    // 0  0  1  ST(0) < ST(i)
    // 1  0  0  ST(0) = ST(i)
    // 1  1  1  unordered
    let (zf, pf, cf) = if a.is_nan() || b.is_nan() {
        (true, true, true)
    } else if a > b {
        (false, false, false)
    } else if a < b {
        (false, false, true)
    } else {
        (true, false, false)
    };

    vcpu.regs.rflags &= !0x45; // Clear ZF, PF, CF
    if zf { vcpu.regs.rflags |= 0x40; }
    if pf { vcpu.regs.rflags |= 0x04; }
    if cf { vcpu.regs.rflags |= 0x01; }
}

/// FXAM - examine ST(0)
fn fxam(vcpu: &mut X86_64Vcpu) {
    let st0 = vcpu.fpu.get_st(0);
    // C1 = sign bit
    let c1 = st0.is_sign_negative();

    // C3 C2 C0 = class
    let (c3, c2, c0) = if st0.is_nan() {
        (false, false, true) // NaN
    } else if st0.is_infinite() {
        (false, true, true) // Infinity
    } else if st0 == 0.0 {
        (true, false, false) // Zero
    } else if st0.is_subnormal() {
        (true, true, false) // Denormal
    } else {
        (false, true, false) // Normal
    };

    vcpu.fpu.status_word &= !0x4700;
    if c3 { vcpu.fpu.status_word |= 0x4000; }
    if c2 { vcpu.fpu.status_word |= 0x0400; }
    if c1 { vcpu.fpu.status_word |= 0x0200; }
    if c0 { vcpu.fpu.status_word |= 0x0100; }
}

/// FPU rounding based on control word
fn fpu_round(cw: u16, val: f64) -> f64 {
    let rc = (cw >> 10) & 3;
    match rc {
        0 => val.round(), // Round to nearest
        1 => val.floor(), // Round down (toward -infinity)
        2 => val.ceil(),  // Round up (toward +infinity)
        3 => val.trunc(), // Truncate (toward zero)
        _ => unreachable!(),
    }
}

/// Decode f64 to mantissa, exponent, sign
fn decode_f64(val: f64) -> (f64, i32, bool) {
    let bits = val.to_bits();
    let sign = bits >> 63 != 0;
    let exponent = ((bits >> 52) & 0x7FF) as i32;
    let mantissa_bits = bits & 0x000F_FFFF_FFFF_FFFF;
    let mantissa = if exponent == 0 {
        (mantissa_bits as f64) / (1u64 << 52) as f64
    } else {
        1.0 + (mantissa_bits as f64) / (1u64 << 52) as f64
    };
    (if sign { -mantissa } else { mantissa }, exponent, sign)
}

/// Convert 80-bit extended precision to f64
fn f80_to_f64(bytes: &[u8]) -> f64 {
    // Simple conversion - just extract the value
    let mantissa = u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    let exp_sign = u16::from_le_bytes([bytes[8], bytes[9]]);
    let sign = (exp_sign >> 15) != 0;
    let exponent = (exp_sign & 0x7FFF) as i32;

    if exponent == 0 && mantissa == 0 {
        return if sign { -0.0 } else { 0.0 };
    }
    if exponent == 0x7FFF {
        if mantissa == 0 {
            return if sign { f64::NEG_INFINITY } else { f64::INFINITY };
        } else {
            return f64::NAN;
        }
    }

    // Convert to f64 - bias difference is 16383 - 1023 = 15360
    let f64_exp = exponent - 15360;
    let f64_mantissa = mantissa >> 11; // Drop precision

    if f64_exp <= 0 || f64_exp >= 2047 {
        // Out of range - just approximate
        let val = (mantissa as f64) * (2.0_f64).powi(exponent - 16383 - 63);
        return if sign { -val } else { val };
    }

    let bits = ((sign as u64) << 63) | ((f64_exp as u64) << 52) | (f64_mantissa & 0x000F_FFFF_FFFF_FFFF);
    f64::from_bits(bits)
}

/// Convert f64 to 80-bit extended precision
fn f64_to_f80(val: f64) -> [u8; 10] {
    let mut bytes = [0u8; 10];

    if val == 0.0 {
        if val.is_sign_negative() {
            bytes[9] = 0x80;
        }
        return bytes;
    }
    if val.is_nan() {
        bytes[7] = 0xC0;
        bytes[8] = 0xFF;
        bytes[9] = 0x7F;
        return bytes;
    }
    if val.is_infinite() {
        bytes[7] = 0x80;
        bytes[8] = 0xFF;
        bytes[9] = if val.is_sign_negative() { 0xFF } else { 0x7F };
        return bytes;
    }

    let bits = val.to_bits();
    let sign = bits >> 63;
    let exp = ((bits >> 52) & 0x7FF) as i32;
    let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;

    // Convert exponent (bias 1023 -> 16383)
    let f80_exp = (exp + 15360) as u16;
    // Add explicit integer bit
    let f80_mantissa = (1u64 << 63) | (mantissa << 11);

    bytes[0..8].copy_from_slice(&f80_mantissa.to_le_bytes());
    let exp_sign = f80_exp | ((sign as u16) << 15);
    bytes[8..10].copy_from_slice(&exp_sign.to_le_bytes());

    bytes
}

/// Convert BCD to f64
fn bcd_to_f64(bytes: &[u8]) -> f64 {
    let sign = (bytes[9] & 0x80) != 0;
    let mut val = 0i64;
    for i in 0..9 {
        let lo = (bytes[i] & 0x0F) as i64;
        let hi = ((bytes[i] >> 4) & 0x0F) as i64;
        val = val * 100 + hi * 10 + lo;
    }
    if sign { -(val as f64) } else { val as f64 }
}

/// Convert f64 to BCD
fn f64_to_bcd(val: f64) -> [u8; 10] {
    let mut bytes = [0u8; 10];
    let sign = val < 0.0;
    let mut n = val.abs() as u64;

    for i in 0..9 {
        let lo = (n % 10) as u8;
        n /= 10;
        let hi = (n % 10) as u8;
        n /= 10;
        bytes[i] = lo | (hi << 4);
    }
    if sign {
        bytes[9] = 0x80;
    }

    bytes
}

/// FLDENV - load FPU environment
fn fldenv(vcpu: &mut X86_64Vcpu, addr: u64) -> Result<()> {
    // Format (28 bytes):
    // 0-1: FCW, 2-3: FSW, 4-5: FTW, 6-7: FIP, 8-9: FCS, 10-11: FDP, 12-13: FDS
    // 14-27: reserved
    vcpu.fpu.control_word = vcpu.read_mem16(addr)?;
    vcpu.fpu.status_word = vcpu.read_mem16(addr + 2)?;
    vcpu.fpu.tag_word = vcpu.read_mem16(addr + 4)?;
    vcpu.fpu.instr_ptr = vcpu.read_mem16(addr + 6)? as u64;
    // FCS at offset 8 (code segment, ignored in 64-bit mode)
    vcpu.fpu.data_ptr = vcpu.read_mem16(addr + 10)? as u64;
    // FDS at offset 12 (data segment, ignored in 64-bit mode)
    vcpu.fpu.top = ((vcpu.fpu.status_word >> 11) & 7) as u8;
    Ok(())
}

/// FNSTENV - store FPU environment
fn fnstenv(vcpu: &mut X86_64Vcpu, addr: u64) -> Result<()> {
    // Format (28 bytes):
    // 0-1: FCW, 2-3: FSW, 4-5: FTW, 6-7: FIP, 8-9: FCS, 10-11: FDP, 12-13: FDS
    // 14-27: reserved
    vcpu.write_mem16(addr, vcpu.fpu.control_word)?;
    vcpu.write_mem16(addr + 2, vcpu.fpu.status_word)?;
    vcpu.write_mem16(addr + 4, vcpu.fpu.tag_word)?;
    vcpu.write_mem16(addr + 6, vcpu.fpu.instr_ptr as u16)?;
    vcpu.write_mem16(addr + 8, 0)?; // FCS (code segment)
    vcpu.write_mem16(addr + 10, vcpu.fpu.data_ptr as u16)?;
    vcpu.write_mem16(addr + 12, 0)?; // FDS (data segment)
    // Remaining 14 bytes are reserved/unused
    for i in 0..7 {
        vcpu.write_mem16(addr + 14 + i * 2, 0)?;
    }
    Ok(())
}

/// FRSTOR - restore FPU state
fn frstor(vcpu: &mut X86_64Vcpu, addr: u64) -> Result<()> {
    // Load environment first
    fldenv(vcpu, addr)?;
    // Load registers (28 bytes env + 8 * 10 bytes regs = 108 bytes)
    for i in 0usize..8 {
        let bytes = vcpu.read_bytes(addr + 28 + (i as u64) * 10, 10)?;
        vcpu.fpu.st[i] = f80_to_f64(&bytes);
    }
    Ok(())
}

/// FNSAVE - save FPU state and reinitialize
fn fnsave(vcpu: &mut X86_64Vcpu, addr: u64) -> Result<()> {
    // Store environment first
    fnstenv(vcpu, addr)?;
    // Store registers (28 bytes env + 8 * 10 bytes regs = 108 bytes)
    for i in 0usize..8 {
        let bytes = f64_to_f80(vcpu.fpu.st[i]);
        vcpu.write_bytes(addr + 28 + i as u64 * 10, &bytes)?;
    }
    // Reinitialize FPU
    vcpu.fpu.init();
    Ok(())
}

// Public wrappers for FXSAVE/FXRSTOR in cpu.rs
pub fn f80_to_f64_pub(bytes: &[u8]) -> f64 {
    f80_to_f64(bytes)
}

pub fn f64_to_f80_pub(val: f64) -> [u8; 10] {
    f64_to_f80(val)
}
