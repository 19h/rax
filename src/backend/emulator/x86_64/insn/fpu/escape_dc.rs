//! DC escape - FADD, FMUL, FCOM, FCOMP, FSUB, FSUBR, FDIV, FDIVR (m64)

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::helpers::set_fpu_compare_flags;

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
