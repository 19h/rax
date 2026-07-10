//! DE escape - FADDP, FMULP, FCOMP, FSUBP, FSUBRP, FDIVP, FDIVRP

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use super::helpers::set_fpu_compare_flags;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

fn st_empty(vcpu: &X86_64Vcpu, i: u8) -> bool {
    let idx = vcpu.fpu.st_index(i);
    ((vcpu.fpu.tag_word >> ((idx as u16) * 2)) & 3) == 3
}

fn set_stack_compare_invalid(vcpu: &mut X86_64Vcpu) {
    vcpu.fpu.status_word &= !0x4780;
    vcpu.fpu.status_word |= 0x0001 | 0x0040 | 0x0100 | 0x0400 | 0x4000;
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
            0 => vcpu.fpu.set_st(0, st0 + val),         // FIADD m16int
            1 => vcpu.fpu.set_st(0, st0 * val),         // FIMUL m16int
            2 => set_fpu_compare_flags(vcpu, st0, val), // FICOM m16int
            3 => {
                // FICOMP m16int
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
            0xC0..=0xC7 => {
                // FADDP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti + st0);
                vcpu.fpu.pop();
            }
            0xC8..=0xCF => {
                // FMULP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti * st0);
                vcpu.fpu.pop();
            }
            0xD0..=0xD7 => {
                // Legacy FCOMP ST(i) alias.
                if st_empty(vcpu, 0) || st_empty(vcpu, rm) {
                    set_stack_compare_invalid(vcpu);
                } else {
                    set_fpu_compare_flags(vcpu, st0, sti);
                }
                vcpu.fpu.pop();
            }
            0xD9 => {
                // FCOMPP
                set_fpu_compare_flags(vcpu, st0, sti);
                vcpu.fpu.pop();
                vcpu.fpu.pop();
            }
            0xE0..=0xE7 => {
                // FSUBRP ST(i), ST(0)
                vcpu.fpu.set_st(rm, st0 - sti);
                vcpu.fpu.pop();
            }
            0xE8..=0xEF => {
                // FSUBP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti - st0);
                vcpu.fpu.pop();
            }
            0xF0..=0xF7 => {
                // FDIVRP ST(i), ST(0)
                vcpu.fpu.set_st(rm, st0 / sti);
                vcpu.fpu.pop();
            }
            0xF8..=0xFF => {
                // FDIVP ST(i), ST(0)
                vcpu.fpu.set_st(rm, sti / st0);
                vcpu.fpu.pop();
            }
            _ => {
                vcpu.inject_exception(6, None)?;
                return Ok(None);
            }
        }
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
