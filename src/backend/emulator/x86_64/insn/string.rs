//! String instructions: MOVS, STOS, LODS, SCAS, CMPS with REP prefix support.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;

/// MOVSB (0xA4)
pub fn movsb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.mmu.read_u8(vcpu.regs.rsi, &vcpu.sregs)?;
        vcpu.mmu.write_u8(vcpu.regs.rdi, val, &vcpu.sregs)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// MOVSW/MOVSD/MOVSQ (0xA5)
pub fn movs(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let delta = op_size as u64;
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.read_mem(vcpu.regs.rsi, op_size)?;
        vcpu.write_mem(vcpu.regs.rdi, val, op_size)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(delta);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STOSB (0xAA)
pub fn stosb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        vcpu.mmu
            .write_u8(vcpu.regs.rdi, vcpu.regs.rax as u8, &vcpu.sregs)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STOSW/STOSD/STOSQ (0xAB)
pub fn stos(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let delta = op_size as u64;
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        vcpu.write_mem(vcpu.regs.rdi, vcpu.regs.rax, op_size)?;
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(delta);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LODSB (0xAC)
pub fn lodsb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.mmu.read_u8(vcpu.regs.rsi, &vcpu.sregs)?;
        vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (val as u64);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(1);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LODSW/LODSD/LODSQ (0xAD)
pub fn lods(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let delta = op_size as u64;
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.read_mem(vcpu.regs.rsi, op_size)?;
        vcpu.set_reg(0, val, op_size);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(delta);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SCASB (0xAE)
pub fn scasb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.mmu.read_u8(vcpu.regs.rdi, &vcpu.sregs)? as u64;
        let al = vcpu.regs.rax & 0xFF;
        let result = al.wrapping_sub(val);
        flags::update_flags_sub(&mut vcpu.regs.rflags, al, val, result, 1);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
            let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;
            // REPE (0xF3): continue while equal (ZF=1)
            // REPNE (0xF2): continue while not equal (ZF=0)
            if ctx.rep_prefix == Some(0xF3) && !zf {
                break;
            }
            if ctx.rep_prefix == Some(0xF2) && zf {
                break;
            }
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SCASW/SCASD/SCASQ (0xAF)
pub fn scas(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let delta = op_size as u64;
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.read_mem(vcpu.regs.rdi, op_size)?;
        let rax = vcpu.get_reg(0, op_size);
        let result = rax.wrapping_sub(val);
        flags::update_flags_sub(&mut vcpu.regs.rflags, rax, val, result, op_size);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(delta);
        } else {
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
            let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;
            if ctx.rep_prefix == Some(0xF3) && !zf {
                break;
            }
            if ctx.rep_prefix == Some(0xF2) && zf {
                break;
            }
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMPSB (0xA6)
pub fn cmpsb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val1 = vcpu.mmu.read_u8(vcpu.regs.rsi, &vcpu.sregs)? as u64;
        let val2 = vcpu.mmu.read_u8(vcpu.regs.rdi, &vcpu.sregs)? as u64;
        let result = val1.wrapping_sub(val2);
        flags::update_flags_sub(&mut vcpu.regs.rflags, val1, val2, result, 1);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(1);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(1);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(1);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
            let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;
            if ctx.rep_prefix == Some(0xF3) && !zf {
                break;
            }
            if ctx.rep_prefix == Some(0xF2) && zf {
                break;
            }
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMPSW/CMPSD/CMPSQ (0xA7)
pub fn cmps(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let delta = op_size as u64;
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val1 = vcpu.read_mem(vcpu.regs.rsi, op_size)?;
        let val2 = vcpu.read_mem(vcpu.regs.rdi, op_size)?;
        let result = val1.wrapping_sub(val2);
        flags::update_flags_sub(&mut vcpu.regs.rflags, val1, val2, result, op_size);
        if vcpu.regs.rflags & flags::bits::DF == 0 {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_add(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_add(delta);
        } else {
            vcpu.regs.rsi = vcpu.regs.rsi.wrapping_sub(delta);
            vcpu.regs.rdi = vcpu.regs.rdi.wrapping_sub(delta);
        }
        if ctx.rep_prefix.is_some() {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
            let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;
            if ctx.rep_prefix == Some(0xF3) && !zf {
                break;
            }
            if ctx.rep_prefix == Some(0xF2) && zf {
                break;
            }
        }
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
