//! Scan string instructions: SCASB, SCASW, SCASD, SCASQ.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

/// SCASB (0xAE)
pub fn scasb(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let count = if ctx.rep_prefix.is_some() {
        vcpu.regs.rcx
    } else {
        1
    };
    
    // Debug: trace REPNE SCASB looking for 0x87 (CTLQUOTEMARK)
    let al = vcpu.regs.rax & 0xFF;
    if ctx.rep_prefix == Some(0xF2) && al == 0x87 && 
       vcpu.regs.rdi >= 0x30000000 && vcpu.regs.rdi < 0x50000000 {
        let rip = vcpu.regs.rip;
        eprintln!("[SCASB] RIP={:#x} RDI={:#x} AL={:#x} RCX={}", rip, vcpu.regs.rdi, al, vcpu.regs.rcx);
        // Dump first 32 bytes at RDI
        let mut dump = String::new();
        for i in 0..32u64 {
            if let Ok(b) = vcpu.mmu.read_u8(vcpu.regs.rdi.wrapping_add(i), &vcpu.sregs) {
                dump.push_str(&format!("{:02x} ", b));
            } else {
                dump.push_str("?? ");
            }
        }
        eprintln!("  RDI dump: {}", dump);
    }
    
    for _ in 0..count {
        if ctx.rep_prefix.is_some() && vcpu.regs.rcx == 0 {
            break;
        }
        let val = vcpu.mmu.read_u8(vcpu.regs.rdi, &vcpu.sregs)? as u64;
        let al = vcpu.regs.rax & 0xFF;
        let result = al.wrapping_sub(val);
        flags::update_flags_sub(&mut vcpu.regs.rflags, al, val, result, 1);
        vcpu.clear_lazy_flags();
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
        vcpu.clear_lazy_flags();
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
