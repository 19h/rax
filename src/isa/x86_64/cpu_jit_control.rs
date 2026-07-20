//! Helper-backed native MOV-to-control-register state transitions.

use super::X86_64Vcpu;
use crate::isa::x86_64::execute::system::{X86ControlWriteState, validate_x86_control_write};
use crate::smir::lower::runtime::GuestRegs;

/// Validate and commit one native MOV-to-control-register operation.
///
/// The helper updates only the marshalled architectural state. CR0/CR3/CR4
/// writes additionally invalidate the owning vCPU's software TLB before native
/// execution returns to the exact next-instruction frontier.
///
/// # Safety
///
/// `state` must reference the live [`GuestRegs`] image for the owning
/// [`X86_64Vcpu`], and `state.ctx` must contain that vCPU's valid address.
pub(super) unsafe extern "C" fn rax_jit_write_control(
    state: *mut GuestRegs,
    control: u64,
    value: u64,
) -> u64 {
    if !matches!(control, 0 | 2 | 3 | 4 | 8) {
        return 0;
    }
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    // Real-address mode permits MOV-to-CR. Protected, compatibility, and
    // 64-bit execution require effective CPL0; GuestRegs.cpl maps VM86 to 3.
    if state.cr0 & 1 != 0 && state.cpl != 0 {
        return 0;
    }
    let Ok(effect) = validate_x86_control_write(
        control as u8,
        value,
        X86ControlWriteState {
            cr0: state.cr0,
            cr3: state.cr3,
            cr4: state.cr4,
            efer: state.efer,
            cs_l: state.cs_l != 0,
            tr_type: state.tr_type as u8,
        },
    ) else {
        return 0;
    };

    // Resolve every fallible runtime dependency before committing state.
    let vcpu = if effect.flush_tlb {
        let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
            return 0;
        };
        Some(vcpu)
    } else {
        None
    };

    match control {
        0 => state.cr0 = effect.value,
        2 => state.cr2 = effect.value,
        3 => state.cr3 = effect.value,
        4 => state.cr4 = effect.value,
        8 => state.cr8 = effect.value,
        _ => unreachable!("validated control selector changed"),
    }
    state.efer = effect.efer;
    if let Some(vcpu) = vcpu {
        vcpu.mmu.flush_tlb();
    }
    1
}
