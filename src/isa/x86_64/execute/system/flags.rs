//! Flag manipulation instructions: CLI, STI, CLC, STC, CLD, STD, CMC, LAHF, SAHF.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use super::control_regs::{current_cpl, raise_gp0};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::flags;

const CR0_PE: u64 = 1 << 0;
const CR4_VME: u64 = 1 << 0;
const CR4_PVI: u64 = 1 << 1;

/// Guest RFLAGS fields needed to evaluate virtualized interrupt-flag
/// instructions. Native execution carries these fields outside host RFLAGS:
/// user-mode PUSHFQ cannot round-trip IF/IOPL, and host VM/VIF/VIP are not
/// guest architectural state.
pub(crate) const X86_INTERRUPT_CONTROL_RFLAGS_MASK: u64 = flags::bits::IF
    | flags::bits::IOPL_MASK
    | flags::bits::VM
    | flags::bits::VIF
    | flags::bits::VIP;

/// Architectural inputs used by one CLI decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86CliState {
    pub cr0: u64,
    pub cr4: u64,
    pub rflags: u64,
    /// Effective CPL. Virtual-8086 mode is represented as CPL3.
    pub cpl: u8,
}

/// The single architectural flag bit cleared by a successful CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86CliEffect {
    ClearIf,
    ClearVif,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86CliFault {
    GeneralProtection,
}

/// Architectural inputs used by one STI decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86StiState {
    pub cr0: u64,
    pub cr4: u64,
    pub rflags: u64,
    /// Effective CPL. Virtual-8086 mode is represented as CPL3.
    pub cpl: u8,
}

/// Architectural state change produced by a successful STI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86StiEffect {
    /// Set IF. `inhibit_interrupts` is true exactly when IF was initially zero,
    /// requiring maskable external interrupts to remain blocked through the
    /// following instruction boundary.
    SetIf { inhibit_interrupts: bool },
    /// Set VIF through protected-mode or virtual-8086 interrupt virtualization.
    SetVif,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86StiFault {
    GeneralProtection,
}

/// Evaluate CLI without committing architectural state.
///
/// Real mode always clears IF. Protected mode clears IF when CPL is permitted
/// by IOPL. Otherwise virtual-8086 mode can redirect the write to VIF through
/// CR4.VME, and protected CPL3 can do so through CR4.PVI; every other case
/// faults with #GP(0).
pub(crate) fn evaluate_x86_cli(
    state: X86CliState,
) -> core::result::Result<X86CliEffect, X86CliFault> {
    if state.cr0 & CR0_PE == 0 {
        return Ok(X86CliEffect::ClearIf);
    }

    let iopl = ((state.rflags & flags::bits::IOPL_MASK) >> 12) as u8;
    if state.cpl <= iopl {
        return Ok(X86CliEffect::ClearIf);
    }

    if state.rflags & flags::bits::VM != 0 {
        return if state.cr4 & CR4_VME != 0 {
            Ok(X86CliEffect::ClearVif)
        } else {
            Err(X86CliFault::GeneralProtection)
        };
    }

    if state.cpl == 3 && state.cr4 & CR4_PVI != 0 {
        Ok(X86CliEffect::ClearVif)
    } else {
        Err(X86CliFault::GeneralProtection)
    }
}

/// Evaluate STI without committing architectural state.
///
/// Real mode sets IF. Protected and virtual-8086 modes set IF when CPL is
/// permitted by IOPL. Otherwise CR4.PVI or CR4.VME can redirect the update to
/// VIF, but a pending virtual interrupt (VIP=1) faults with #GP(0). The
/// one-instruction maskable-interrupt inhibition is produced only when STI
/// changes IF from zero to one; setting VIF never creates that physical
/// interrupt shadow.
pub(crate) fn evaluate_x86_sti(
    state: X86StiState,
) -> core::result::Result<X86StiEffect, X86StiFault> {
    let set_if = || X86StiEffect::SetIf {
        inhibit_interrupts: state.rflags & flags::bits::IF == 0,
    };

    if state.cr0 & CR0_PE == 0 {
        return Ok(set_if());
    }

    let iopl = ((state.rflags & flags::bits::IOPL_MASK) >> 12) as u8;
    if state.cpl <= iopl {
        return Ok(set_if());
    }

    let virtual_interrupts = if state.rflags & flags::bits::VM != 0 {
        state.cr4 & CR4_VME != 0
    } else {
        state.cpl == 3 && state.cr4 & CR4_PVI != 0
    };
    if !virtual_interrupts || state.rflags & flags::bits::VIP != 0 {
        return Err(X86StiFault::GeneralProtection);
    }

    Ok(X86StiEffect::SetVif)
}

/// CLI - Clear Interrupt Flag (0xFA)
pub fn cli(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    use crate::isa::x86_64::cpu::log_if_transition;
    match evaluate_x86_cli(X86CliState {
        cr0: vcpu.sregs.cr0,
        cr4: vcpu.sregs.cr4,
        rflags: vcpu.regs.rflags,
        cpl: current_cpl(vcpu),
    }) {
        Ok(X86CliEffect::ClearIf) => {
            let old_if = (vcpu.regs.rflags & flags::bits::IF) != 0;
            vcpu.regs.rflags &= !flags::bits::IF;
            log_if_transition(vcpu.regs.rip, old_if, false, "CLI");
        }
        Ok(X86CliEffect::ClearVif) => vcpu.regs.rflags &= !flags::bits::VIF,
        Err(X86CliFault::GeneralProtection) => return raise_gp0(vcpu),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STI - Set Interrupt Flag (0xFB)
pub fn sti(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    use crate::isa::x86_64::cpu::log_if_transition;
    match evaluate_x86_sti(X86StiState {
        cr0: vcpu.sregs.cr0,
        cr4: vcpu.sregs.cr4,
        rflags: vcpu.regs.rflags,
        cpl: current_cpl(vcpu),
    }) {
        Ok(X86StiEffect::SetIf { inhibit_interrupts }) => {
            let old_if = (vcpu.regs.rflags & flags::bits::IF) != 0;
            vcpu.regs.rflags |= flags::bits::IF;
            vcpu.interrupt_inhibit = inhibit_interrupts;
            log_if_transition(vcpu.regs.rip, old_if, true, "STI");
        }
        Ok(X86StiEffect::SetVif) => vcpu.regs.rflags |= flags::bits::VIF,
        Err(X86StiFault::GeneralProtection) => return raise_gp0(vcpu),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CLC - Clear Carry Flag (0xF8)
pub fn clc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    vcpu.regs.rflags &= !flags::bits::CF;
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STC - Set Carry Flag (0xF9)
pub fn stc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    vcpu.regs.rflags |= flags::bits::CF;
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CLD - Clear Direction Flag (0xFC)
pub fn cld(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags &= !flags::bits::DF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// STD - Set Direction Flag (0xFD)
pub fn std(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.regs.rflags |= flags::bits::DF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMC - Complement Carry Flag (0xF5)
pub fn cmc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    vcpu.regs.rflags ^= flags::bits::CF;
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// LAHF - Load AH from Flags (0x9F)
/// Loads SF, ZF, AF, PF, CF from RFLAGS into AH
pub fn lahf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    // AH = SF:ZF:0:AF:0:PF:1:CF (bits 7:6:5:4:3:2:1:0)
    let mut flags_byte = (vcpu.regs.rflags & 0xD5) as u8;
    flags_byte |= 0x02;
    // Set AH (bits 8-15 of RAX)
    vcpu.regs.rax = (vcpu.regs.rax & !0xFF00) | ((flags_byte as u64) << 8);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// SAHF - Store AH into Flags (0x9E)
/// Stores AH into SF, ZF, AF, PF, CF of RFLAGS
pub fn sahf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // SAHF preserves OF (and the non-0xD5 bits). After a lazy Jcc/SETcc/CMOVcc
    // the authoritative flags live in lazy_flags, not regs.rflags; commit them
    // before this partial write so the preserved OF is not lost.
    vcpu.materialize_flags();
    // AH contains SF:ZF:0:AF:0:PF:1:CF
    let ah = ((vcpu.regs.rax >> 8) & 0xFF) as u64;
    // Mask for SF, ZF, AF, PF, CF (bits 7, 6, 4, 2, 0)
    let mask = 0xD5u64; // 1101_0101
    vcpu.regs.rflags = (vcpu.regs.rflags & !mask) | (ah & mask);
    // Bit 1 is always set
    vcpu.regs.rflags |= 0x2;
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
