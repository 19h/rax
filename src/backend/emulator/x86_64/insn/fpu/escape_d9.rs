//! D9 escape - FLD, FST, FSTP, FLDENV, FLDCW, FNSTENV, FNSTCW, etc.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::helpers::{decode_f64, fldenv, fnstenv, fpu_round, fxam, set_fpu_compare_flags};

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
