//! Interrupt instructions: INT, INT3, INTO.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

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
