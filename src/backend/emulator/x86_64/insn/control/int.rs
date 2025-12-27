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
/// This instruction invokes exception vector 3 (breakpoint exception)
pub fn int3(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // INT3 is a 1-byte instruction - RIP should point AFTER the INT3
    // (it's a trap, not a fault, so RIP points to next instruction)
    vcpu.regs.rip += ctx.cursor as u64;
    // Inject #BP exception (vector 3) into the guest via IDT
    vcpu.inject_exception(3, None)?;
    Ok(None)
}

/// INT imm8 (0xCD) - Software interrupt
pub fn int_imm8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let vector = ctx.consume_u8()?;
    vcpu.regs.rip += ctx.cursor as u64;
    // Inject the software interrupt via IDT
    vcpu.inject_exception(vector, None)?;
    Ok(None)
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
        // INTO is invalid in 64-bit mode - inject #UD
        vcpu.inject_exception(6, None)?;
        return Ok(None);
    }

    // Check overflow flag
    if vcpu.regs.rflags & flags::bits::OF != 0 {
        // OF=1: Generate INT 4 (overflow exception)
        vcpu.regs.rip += ctx.cursor as u64;
        vcpu.inject_exception(4, None)?;
        Ok(None)
    } else {
        // OF=0: No interrupt, continue execution
        vcpu.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}

/// IRET/IRETD/IRETQ (0xCF) - Interrupt return
pub fn iret(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static IRET_TO_USER: AtomicUsize = AtomicUsize::new(0);

    let op_size = ctx.op_size;
    let old_cpl = vcpu.sregs.cs.selector & 0x3;

    // Check if we're in 64-bit mode
    let in_long_mode = (vcpu.sregs.efer & 0x400) != 0; // EFER.LMA = bit 10
    let in_64bit_mode = in_long_mode && vcpu.sregs.cs.l;

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

    // Log ALL IRETs to see what's happening
    let count = IRET_TO_USER.fetch_add(1, Ordering::Relaxed);
    // Log more generously, now including IF flag info
    let saved_if = (flags & 0x200) != 0; // Bit 9 is IF
    if count < 20 || (count % 1000 == 0) || new_cpl == 3 {
        eprintln!("[IRET #{}] cpl:{}->{} RIP={:#x} CS={:#x} saved_IF={}",
            count, old_cpl, new_cpl, ret_ip, cs, if saved_if { 1 } else { 0 });
    }

    // In 64-bit mode, IRETQ ALWAYS pops RSP and SS, regardless of privilege level change.
    // In 32-bit mode, RSP/SS are only popped on privilege level change.
    let (new_rsp, new_ss) = if in_64bit_mode || new_cpl > old_cpl {
        let new_rsp = pop_by_size(vcpu, op_size)?;
        let new_ss = pop_by_size(vcpu, op_size)? as u16;

        vcpu.set_sreg(2, new_ss); // SS is segment register 2
        vcpu.regs.rsp = new_rsp;
        (new_rsp, new_ss)
    } else {
        (vcpu.regs.rsp, vcpu.sregs.ss.selector)
    };

    apply_iret_flags(vcpu, op_size, flags)?;
    vcpu.regs.rip = ret_ip;
    vcpu.set_sreg(1, cs); // CS is segment register 1
    Ok(None)
}
