//! Interrupt instructions: INT, INT3, INTO.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;
use super::call::validate_far_selector;

fn pop_by_size(vcpu: &mut X86_64Vcpu, size: u8) -> Result<u64> {
    match size {
        2 => Ok(vcpu.pop16()? as u64),
        4 => Ok(vcpu.pop32()? as u64),
        8 => vcpu.pop64(),
        _ => Err(Error::Emulator(format!(
            "invalid IRET stack pop size: {}",
            size
        ))),
    }
}

fn apply_iret_flags(vcpu: &mut X86_64Vcpu, size: u8, value: u64) -> Result<()> {
    match size {
        2 => {
            let mask = 0xFFFFu64;
            vcpu.regs.rflags = (vcpu.regs.rflags & !mask) | (value & mask) | 0x2;
            Ok(())
        }
        4 | 8 => {
            let mask = 0x00000000_00257FD5u64;
            vcpu.regs.rflags = (value & mask) | 0x2;
            Ok(())
        }
        _ => Err(Error::Emulator(format!(
            "invalid IRET flags size: {}",
            size
        ))),
    }
}

/// INT3 (0xCC) - Debug breakpoint interrupt
pub fn int3(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rip += ctx.cursor as u64;
    // Return breakpoint exit to caller
    Ok(Some(VcpuExit::Debug))
}

/// INT imm8 (0xCD) - Software interrupt
pub fn int_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let vector = ctx.consume_u8()?;
    vcpu.regs.rip += ctx.cursor as u64;
    // For now, just return a special exit for INT instructions
    // Real implementation would need IDT lookup and privilege checks
    Ok(Some(VcpuExit::Exception(vector)))
}

/// INTO (0xCE) - Interrupt on Overflow
/// If OF=1, generates INT 4 (overflow exception)
/// If OF=0, does nothing and continues
/// Invalid in 64-bit mode (generates #UD)
pub fn into(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // Check if we're in 64-bit mode
    let in_long_mode = (vcpu.sregs.efer & 0x400) != 0; // EFER.LMA = bit 10
    let in_64bit_mode = in_long_mode && vcpu.sregs.cs.l;

    if in_64bit_mode {
        // INTO is invalid in 64-bit mode
        return Err(Error::Emulator(format!(
            "INTO (0xCE) is invalid in 64-bit mode at RIP={:#x}",
            vcpu.regs.rip
        )));
    }

    // Check overflow flag
    if vcpu.regs.rflags & flags::bits::OF != 0 {
        // OF=1: Generate INT 4 (overflow exception)
        vcpu.regs.rip += ctx.cursor as u64;
        Ok(Some(VcpuExit::Exception(4)))
    } else {
        // OF=0: No interrupt, continue execution
        vcpu.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}

/// IRET/IRETD/IRETQ (0xCF) - Interrupt return
pub fn iret(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let old_cpl = vcpu.sregs.cs.selector & 0x3;

    let ret_ip = pop_by_size(vcpu, op_size)?;
    let cs = pop_by_size(vcpu, op_size)? as u16;
    validate_far_selector(vcpu, cs)?;
    let flags = pop_by_size(vcpu, op_size)?;

    let new_cpl = cs & 0x3;
    if new_cpl < old_cpl {
        return Err(Error::Emulator(
            "IRET privilege increase not supported".to_string(),
        ));
    }

    if new_cpl > old_cpl {
        let new_rsp = pop_by_size(vcpu, op_size)?;
        let new_ss = pop_by_size(vcpu, op_size)? as u16;
        vcpu.set_sreg(2, new_ss); // SS is segment register 2
        vcpu.regs.rsp = new_rsp;
    }

    apply_iret_flags(vcpu, op_size, flags)?;
    vcpu.regs.rip = ret_ip;
    vcpu.set_sreg(1, cs); // CS is segment register 1
    Ok(None)
}
