//! Loop instructions: LOOP, LOOPZ, LOOPNZ, JRCXZ.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

fn loop_counter_width(vcpu: &X86_64Vcpu, ctx: &InsnContext) -> u8 {
    let in_long_mode = (vcpu.sregs.efer & 0x400) != 0;
    if in_long_mode && vcpu.sregs.cs.l {
        if ctx.address_size_override {
            4
        } else {
            8
        }
    } else {
        let default_16bit = !vcpu.sregs.cs.db;
        let is_16bit = default_16bit ^ ctx.address_size_override;
        if is_16bit {
            2
        } else {
            4
        }
    }
}

fn decrement_loop_counter(vcpu: &mut X86_64Vcpu, width: u8) -> u64 {
    match width {
        2 => {
            let cx = (vcpu.regs.rcx as u16).wrapping_sub(1);
            vcpu.regs.rcx = (vcpu.regs.rcx & !0xffff) | u64::from(cx);
            u64::from(cx)
        }
        4 => {
            let ecx = (vcpu.regs.rcx as u32).wrapping_sub(1);
            vcpu.regs.rcx = u64::from(ecx);
            u64::from(ecx)
        }
        _ => {
            vcpu.regs.rcx = vcpu.regs.rcx.wrapping_sub(1);
            vcpu.regs.rcx
        }
    }
}

fn loop_counter_value(vcpu: &X86_64Vcpu, width: u8) -> u64 {
    match width {
        2 => u64::from(vcpu.regs.rcx as u16),
        4 => u64::from(vcpu.regs.rcx as u32),
        _ => vcpu.regs.rcx,
    }
}

/// LOOPNZ/LOOPNE rel8 (0xE0) - Decrement ECX/RCX; jump if not zero and ZF=0
pub fn loopnz(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    let disp = ctx.consume_u8()? as i8 as i64;
    let next_rip = vcpu.regs.rip + ctx.cursor as u64;

    let counter = decrement_loop_counter(vcpu, loop_counter_width(vcpu, ctx));

    let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;

    if counter != 0 && !zf {
        vcpu.regs.rip = (next_rip as i64 + disp) as u64;
    } else {
        vcpu.regs.rip = next_rip;
    }
    Ok(None)
}

/// LOOPZ/LOOPE rel8 (0xE1) - Decrement ECX/RCX; jump if not zero and ZF=1
pub fn loopz(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    let disp = ctx.consume_u8()? as i8 as i64;
    let next_rip = vcpu.regs.rip + ctx.cursor as u64;

    let counter = decrement_loop_counter(vcpu, loop_counter_width(vcpu, ctx));

    let zf = (vcpu.regs.rflags & flags::bits::ZF) != 0;

    if counter != 0 && zf {
        vcpu.regs.rip = (next_rip as i64 + disp) as u64;
    } else {
        vcpu.regs.rip = next_rip;
    }
    Ok(None)
}

/// LOOP rel8 (0xE2) - Decrement ECX/RCX; jump if not zero
pub fn loop_rel8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u8()? as i8 as i64;
    let next_rip = vcpu.regs.rip + ctx.cursor as u64;

    let counter = decrement_loop_counter(vcpu, loop_counter_width(vcpu, ctx));

    if counter != 0 {
        vcpu.regs.rip = (next_rip as i64 + disp) as u64;
    } else {
        vcpu.regs.rip = next_rip;
    }
    Ok(None)
}

/// JRCXZ/JECXZ rel8 (0xE3) - Jump if RCX/ECX is zero
pub fn jrcxz(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let disp = ctx.consume_u8()? as i8 as i64;
    let next_rip = vcpu.regs.rip + ctx.cursor as u64;

    let counter = loop_counter_value(vcpu, loop_counter_width(vcpu, ctx));

    if counter == 0 {
        vcpu.regs.rip = (next_rip as i64 + disp) as u64;
    } else {
        vcpu.regs.rip = next_rip;
    }
    Ok(None)
}
