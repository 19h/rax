//! Stack-based flag instructions: PUSHF, POPF.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use super::control_regs::current_cpl;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::flags;

const CR0_PE: u64 = 1 << 0;
const CR0_AM: u64 = 1 << 18;
const CR4_VME: u64 = 1 << 0;

const POPF_LOW_MODIFIABLE: u64 = flags::bits::CF
    | flags::bits::PF
    | flags::bits::AF
    | flags::bits::ZF
    | flags::bits::SF
    | flags::bits::TF
    | flags::bits::IF
    | flags::bits::DF
    | flags::bits::OF
    | flags::bits::IOPL_MASK
    | flags::bits::NT;

/// Architectural state consumed by PUSHF/POPF privilege and virtualization
/// decisions. `cpl` is the effective CPL; virtual-8086 mode uses CPL3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86StackFlagsState {
    pub cr0: u64,
    pub cr4: u64,
    pub rflags: u64,
    pub cpl: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86StackFlagsFault {
    GeneralProtection,
    InvalidWidth,
}

#[inline]
fn virtual_8086(state: X86StackFlagsState) -> bool {
    state.cr0 & CR0_PE != 0 && state.rflags & flags::bits::VM != 0
}

#[inline]
fn iopl(state: X86StackFlagsState) -> u8 {
    ((state.rflags & flags::bits::IOPL_MASK) >> 12) as u8
}

/// Validate faults selected before the implicit stack memory access.
///
/// Virtual-8086 execution with IOPL below 3 requires CR4.VME and a 16-bit
/// operand. The VIP/IF and TF checks for POPF occur only after the word has
/// been read and are therefore handled by [`evaluate_x86_pop_flags`].
pub(crate) fn validate_x86_stack_flags_access(
    state: X86StackFlagsState,
    width: u8,
) -> core::result::Result<(), X86StackFlagsFault> {
    if !matches!(width, 2 | 4 | 8) {
        return Err(X86StackFlagsFault::InvalidWidth);
    }
    if virtual_8086(state) && iopl(state) < 3 && (state.cr4 & CR4_VME == 0 || width != 2) {
        return Err(X86StackFlagsFault::GeneralProtection);
    }
    Ok(())
}

/// Construct the exact FLAGS/EFLAGS/RFLAGS stack image without changing state.
pub(crate) fn evaluate_x86_push_flags(
    state: X86StackFlagsState,
    width: u8,
) -> core::result::Result<u64, X86StackFlagsFault> {
    validate_x86_stack_flags_access(state, width)?;

    if virtual_8086(state) && iopl(state) < 3 {
        let mut image = state.rflags & 0xFFFF;
        image = (image & !flags::bits::IF)
            | if state.rflags & flags::bits::VIF != 0 {
                flags::bits::IF
            } else {
                0
            };
        image = (image & !flags::bits::IOPL_MASK) | flags::bits::IOPL_MASK;
        return Ok(image | 0x2);
    }

    Ok(match width {
        2 => (state.rflags & 0xFFFF) | 0x2,
        4 | 8 => (state.rflags & 0x0000_0000_00FC_FFFF) | 0x2,
        _ => return Err(X86StackFlagsFault::InvalidWidth),
    })
}

/// Evaluate POPF after the complete stack value has been read. The returned
/// image preserves every reserved or privilege-protected bit from `state` and
/// clears RF, which cannot be loaded by POPF.
pub(crate) fn evaluate_x86_pop_flags(
    state: X86StackFlagsState,
    width: u8,
    popped: u64,
) -> core::result::Result<u64, X86StackFlagsFault> {
    validate_x86_stack_flags_access(state, width)?;
    let old_iopl = iopl(state);

    if virtual_8086(state) && old_iopl < 3 {
        if (state.rflags & flags::bits::VIP != 0 && popped & flags::bits::IF != 0)
            || popped & flags::bits::TF != 0
        {
            return Err(X86StackFlagsFault::GeneralProtection);
        }
        let mask = POPF_LOW_MODIFIABLE & !(flags::bits::IOPL_MASK | flags::bits::IF);
        let mut result = (state.rflags & !mask) | (popped & mask);
        result = (result & !flags::bits::VIF)
            | if popped & flags::bits::IF != 0 {
                flags::bits::VIF
            } else {
                0
            };
        return Ok((result & !flags::bits::RF) | 0x2);
    }

    let mut mask = POPF_LOW_MODIFIABLE;
    if width != 2 {
        mask |= flags::bits::AC | flags::bits::ID;
    }

    // Real mode and protected CPL0 may load IOPL. Protected CPL1-3 and every
    // virtual-8086 execution preserve IOPL; IF additionally requires CPL<=IOPL.
    if state.cr0 & CR0_PE != 0 && (virtual_8086(state) || state.cpl != 0) {
        mask &= !flags::bits::IOPL_MASK;
        if state.cpl > old_iopl {
            mask &= !flags::bits::IF;
        }
    }

    Ok((((state.rflags & !mask) | (popped & mask)) & !flags::bits::RF) | 0x2)
}

fn pushf_popf_op_size(vcpu: &X86_64Vcpu, ctx: &InsnContext) -> u8 {
    let in_long_mode = (vcpu.sregs.efer & 0x400) != 0;
    let in_64bit_mode = in_long_mode && vcpu.sregs.cs.l;

    if in_64bit_mode {
        // REX.W/REX2.W takes precedence over 66H; otherwise 66H selects W16.
        if ctx.any_rex_w() || !ctx.operand_size_override {
            8
        } else {
            2
        }
    } else {
        let default_16bit = !vcpu.sregs.cs.db;
        let is_16bit = default_16bit ^ ctx.operand_size_override;
        if is_16bit { 2 } else { 4 }
    }
}

fn architectural_state(vcpu: &X86_64Vcpu) -> X86StackFlagsState {
    X86StackFlagsState {
        cr0: vcpu.sregs.cr0,
        cr4: vcpu.sregs.cr4,
        rflags: vcpu.regs.rflags,
        cpl: current_cpl(vcpu),
    }
}

fn raise_gp0(vcpu: &mut X86_64Vcpu) -> Result<Option<VcpuExit>> {
    vcpu.inject_exception(13, Some(0))?;
    Ok(None)
}

fn validate_long_mode_stack_address(vcpu: &mut X86_64Vcpu, width: u8, push: bool) -> Result<bool> {
    if vcpu.sregs.efer & (1 << 10) == 0 || !vcpu.sregs.cs.l {
        return Ok(true);
    }
    let offset = if push {
        vcpu.stack_pointer_wrapping_sub(u64::from(width))
    } else {
        vcpu.stack_pointer_offset()
    };
    // CS.L implies a flat SS base for linear-address generation.
    let address = offset;
    let canonical = address
        .checked_add(u64::from(width) - 1)
        .is_some_and(|last| super::is_canonical_48(address) && super::is_canonical_48(last));
    if !canonical {
        vcpu.inject_exception(12, Some(0))?;
        return Ok(false);
    }
    if address & (u64::from(width) - 1) != 0
        && vcpu.sregs.cr0 & CR0_AM != 0
        && vcpu.regs.rflags & flags::bits::AC != 0
        && current_cpl(vcpu) == 3
    {
        vcpu.inject_exception(17, Some(0))?;
        return Ok(false);
    }
    Ok(true)
}

/// PUSHF/PUSHFD/PUSHFQ (0x9C).
pub fn pushf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    let width = pushf_popf_op_size(vcpu, ctx);
    let image = match evaluate_x86_push_flags(architectural_state(vcpu), width) {
        Ok(image) => image,
        Err(X86StackFlagsFault::GeneralProtection) => return raise_gp0(vcpu),
        Err(X86StackFlagsFault::InvalidWidth) => unreachable!("decoded PUSHF width"),
    };
    if !validate_long_mode_stack_address(vcpu, width, true)? {
        return Ok(None);
    }

    match width {
        2 => vcpu.push16(image as u16)?,
        4 => vcpu.push32(image as u32)?,
        8 => vcpu.push64(image)?,
        _ => unreachable!("decoded PUSHF width"),
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// POPF/POPFD/POPFQ (0x9D).
pub fn popf(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    vcpu.materialize_flags();
    let width = pushf_popf_op_size(vcpu, ctx);
    let state = architectural_state(vcpu);
    match validate_x86_stack_flags_access(state, width) {
        Ok(()) => {}
        Err(X86StackFlagsFault::GeneralProtection) => return raise_gp0(vcpu),
        Err(X86StackFlagsFault::InvalidWidth) => unreachable!("decoded POPF width"),
    }
    if !validate_long_mode_stack_address(vcpu, width, false)? {
        return Ok(None);
    }

    let old_rsp = vcpu.regs.rsp;
    let popped = match width {
        2 => u64::from(vcpu.pop16()?),
        4 => u64::from(vcpu.pop32()?),
        8 => vcpu.pop64()?,
        _ => unreachable!("decoded POPF width"),
    };
    let new_rflags = match evaluate_x86_pop_flags(state, width, popped) {
        Ok(rflags) => rflags,
        Err(X86StackFlagsFault::GeneralProtection) => {
            // VME checks VIP/IF and TF after the stack read. The read remains
            // observable, but neither RSP nor RFLAGS commits on #GP(0).
            vcpu.regs.rsp = old_rsp;
            return raise_gp0(vcpu);
        }
        Err(X86StackFlagsFault::InvalidWidth) => unreachable!("decoded POPF width"),
    };

    vcpu.regs.rflags = new_rflags;
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: u64 = flags::bits::CF
        | flags::bits::PF
        | flags::bits::AF
        | flags::bits::ZF
        | flags::bits::SF
        | flags::bits::OF;

    fn state(cr0: u64, cr4: u64, rflags: u64, cpl: u8) -> X86StackFlagsState {
        X86StackFlagsState {
            cr0,
            cr4,
            rflags,
            cpl,
        }
    }

    #[test]
    fn popf_privilege_table_preserves_protected_fields_and_clears_rf() {
        let old = 0x2
            | flags::bits::RF
            | flags::bits::VM
            | flags::bits::VIF
            | flags::bits::VIP
            | flags::bits::IOPL_MASK;
        let popped = STATUS
            | flags::bits::TF
            | flags::bits::IF
            | flags::bits::DF
            | flags::bits::NT
            | flags::bits::AC
            | flags::bits::ID;

        let cpl0 =
            evaluate_x86_pop_flags(state(CR0_PE, 0, old & !flags::bits::VM, 0), 8, popped).unwrap();
        assert_eq!(
            cpl0 & (STATUS | flags::bits::TF | flags::bits::IF | flags::bits::DF),
            popped & (STATUS | flags::bits::TF | flags::bits::IF | flags::bits::DF)
        );
        assert_eq!(
            cpl0 & (flags::bits::AC | flags::bits::ID),
            flags::bits::AC | flags::bits::ID
        );
        assert_eq!(cpl0 & flags::bits::RF, 0);

        let cpl3_iopl0 = evaluate_x86_pop_flags(
            state(
                CR0_PE,
                0,
                old & !(flags::bits::IOPL_MASK | flags::bits::VM),
                3,
            ),
            8,
            popped,
        )
        .unwrap();
        assert_eq!(cpl3_iopl0 & flags::bits::IF, 0);
        assert_eq!(cpl3_iopl0 & flags::bits::IOPL_MASK, 0);
        assert_eq!(
            cpl3_iopl0 & (flags::bits::AC | flags::bits::ID),
            flags::bits::AC | flags::bits::ID
        );

        let word = evaluate_x86_pop_flags(state(CR0_PE, 0, old, 3), 2, popped).unwrap();
        assert_eq!(word & flags::bits::IF, flags::bits::IF);
        assert_eq!(word & flags::bits::IOPL_MASK, flags::bits::IOPL_MASK);
        assert_eq!(
            word & (flags::bits::AC | flags::bits::ID),
            old & (flags::bits::AC | flags::bits::ID)
        );
    }

    #[test]
    fn vme_push_and_pop_virtualize_if_and_fault_after_read_conditions() {
        let virtual_state = state(
            CR0_PE,
            CR4_VME,
            0x2 | flags::bits::VM | flags::bits::VIF | flags::bits::VIP,
            3,
        );
        let image = evaluate_x86_push_flags(virtual_state, 2).unwrap();
        assert_ne!(image & flags::bits::IF, 0);
        assert_eq!(image & flags::bits::IOPL_MASK, flags::bits::IOPL_MASK);
        assert_eq!(
            image & (flags::bits::VM | flags::bits::VIF | flags::bits::VIP),
            0
        );

        assert_eq!(
            evaluate_x86_pop_flags(virtual_state, 2, flags::bits::IF),
            Err(X86StackFlagsFault::GeneralProtection)
        );
        assert_eq!(
            evaluate_x86_pop_flags(virtual_state, 2, flags::bits::TF),
            Err(X86StackFlagsFault::GeneralProtection)
        );
        let clear_vif = evaluate_x86_pop_flags(virtual_state, 2, STATUS).unwrap();
        assert_eq!(clear_vif & flags::bits::VIF, 0);
        assert_eq!(clear_vif & flags::bits::VIP, flags::bits::VIP);

        for width in [4, 8] {
            assert_eq!(
                validate_x86_stack_flags_access(virtual_state, width),
                Err(X86StackFlagsFault::GeneralProtection)
            );
        }
        let no_vme = X86StackFlagsState {
            cr4: 0,
            ..virtual_state
        };
        assert_eq!(
            validate_x86_stack_flags_access(no_vme, 2),
            Err(X86StackFlagsFault::GeneralProtection)
        );
    }

    #[test]
    fn push_images_clear_vm_rf_and_upper_bits_only_for_dword_or_qword() {
        let all = state(0, 0, u64::MAX, 0);
        assert_eq!(evaluate_x86_push_flags(all, 2).unwrap(), 0xFFFF);
        assert_eq!(evaluate_x86_push_flags(all, 4).unwrap(), 0x00FC_FFFF);
        assert_eq!(evaluate_x86_push_flags(all, 8).unwrap(), 0x00FC_FFFF);

        let malformed_zero = state(0, 0, 0, 0);
        assert_eq!(evaluate_x86_push_flags(malformed_zero, 8).unwrap(), 0x2);
        assert_eq!(evaluate_x86_pop_flags(malformed_zero, 8, 0).unwrap(), 0x2);
    }
}
