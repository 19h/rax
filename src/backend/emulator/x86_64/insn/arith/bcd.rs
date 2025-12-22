//! BCD Adjustment Instructions.
//!
//! Note: These are technically invalid in 64-bit mode per spec, but we
//! implement them for compatibility with tests and legacy code.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;

/// DAA - Decimal Adjust AL after Addition (0x27)
/// Adjusts AL after BCD addition
pub fn daa(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let old_al = (vcpu.regs.rax & 0xFF) as u8;
    let old_cf = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let af = (vcpu.regs.rflags & flags::bits::AF) != 0;

    let mut al = old_al;
    let mut cf = false;
    let mut af_new = false;

    // If lower nibble > 9 or AF is set, add 6
    if (al & 0x0F) > 9 || af {
        let (new_al, carry) = al.overflowing_add(6);
        al = new_al;
        cf = old_cf || carry;
        af_new = true;
    }

    // If original AL > 0x99 or CF was set, add 0x60
    if old_al > 0x99 || old_cf {
        al = al.wrapping_add(0x60);
        cf = true;
    }

    // Set flags
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (al as u64);

    // Update flags: CF, AF, SF, ZF, PF
    if cf {
        vcpu.regs.rflags |= flags::bits::CF;
    } else {
        vcpu.regs.rflags &= !flags::bits::CF;
    }
    if af_new {
        vcpu.regs.rflags |= flags::bits::AF;
    } else {
        vcpu.regs.rflags &= !flags::bits::AF;
    }

    // SF, ZF, PF based on result
    flags::update_szp(&mut vcpu.regs.rflags, al as u64, 1);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// DAS - Decimal Adjust AL after Subtraction (0x2F)
/// Adjusts AL after BCD subtraction
pub fn das(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let old_al = (vcpu.regs.rax & 0xFF) as u8;
    let old_cf = (vcpu.regs.rflags & flags::bits::CF) != 0;
    let af = (vcpu.regs.rflags & flags::bits::AF) != 0;

    let mut al = old_al;
    let mut cf = false;
    let mut af_new = false;

    // If lower nibble > 9 or AF is set, subtract 6
    if (al & 0x0F) > 9 || af {
        let (new_al, borrow) = al.overflowing_sub(6);
        al = new_al;
        cf = old_cf || borrow;
        af_new = true;
    }

    // If original AL > 0x99 or CF was set, subtract 0x60
    if old_al > 0x99 || old_cf {
        al = al.wrapping_sub(0x60);
        cf = true;
    }

    // Set result
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (al as u64);

    // Update flags: CF, AF, SF, ZF, PF
    if cf {
        vcpu.regs.rflags |= flags::bits::CF;
    } else {
        vcpu.regs.rflags &= !flags::bits::CF;
    }
    if af_new {
        vcpu.regs.rflags |= flags::bits::AF;
    } else {
        vcpu.regs.rflags &= !flags::bits::AF;
    }

    // SF, ZF, PF based on result
    flags::update_szp(&mut vcpu.regs.rflags, al as u64, 1);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AAM - ASCII Adjust AX after Multiply (0xD4 imm8)
/// AH = AL / imm8, AL = AL % imm8
pub fn aam(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm8 = ctx.consume_u8()?;

    // Division by zero causes #DE
    if imm8 == 0 {
        return Err(Error::Emulator("AAM: divide by zero".to_string()));
    }

    let al = (vcpu.regs.rax & 0xFF) as u8;
    let ah = al / imm8;
    let new_al = al % imm8;

    // Set AH and AL
    vcpu.regs.rax = (vcpu.regs.rax & !0xFFFF) | ((ah as u64) << 8) | (new_al as u64);

    // Update SF, ZF, PF based on AL
    flags::update_szp(&mut vcpu.regs.rflags, new_al as u64, 1);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// AAD - ASCII Adjust AX before Division (0xD5 imm8)
/// AL = (AL + (AH * imm8)) & 0xFF, AH = 0
pub fn aad(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let imm8 = ctx.consume_u8()? as u16;

    let al = (vcpu.regs.rax & 0xFF) as u16;
    let ah = ((vcpu.regs.rax >> 8) & 0xFF) as u16;

    let new_al = ((al + (ah * imm8)) & 0xFF) as u8;

    // Set AL, clear AH
    vcpu.regs.rax = (vcpu.regs.rax & !0xFFFF) | (new_al as u64);

    // Update SF, ZF, PF based on AL
    flags::update_szp(&mut vcpu.regs.rflags, new_al as u64, 1);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
