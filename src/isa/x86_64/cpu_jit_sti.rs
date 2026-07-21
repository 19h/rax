//! Native helper for fault-precise x86 STI execution.

use crate::isa::x86_64::execute::system::{
    X86StiEffect, X86StiFault, X86StiState, evaluate_x86_sti,
};
use crate::isa::x86_64::flags;
use crate::smir::lower::runtime::GuestRegs;

/// Evaluate and commit one STI against the marshalled guest control state.
///
/// A zero result requests direct replay at the instruction PC. Every failure
/// is non-committing; a nonzero result sets exactly IF or VIF and records the
/// one-instruction interrupt shadow only for an IF transition from zero to one.
pub(super) unsafe extern "C" fn rax_jit_sti(state: *mut GuestRegs, requires_apx: u64) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if requires_apx != 0 && state.apx_enabled == 0 {
        return 0;
    }
    let Ok(cpl) = u8::try_from(state.cpl) else {
        return 0;
    };
    if cpl > 3 {
        return 0;
    }

    match evaluate_x86_sti(X86StiState {
        cr0: state.cr0,
        cr4: state.cr4,
        rflags: state.interrupt_flags,
        cpl,
    }) {
        Ok(X86StiEffect::SetIf { inhibit_interrupts }) => {
            state.interrupt_flags |= flags::bits::IF;
            state.interrupt_inhibit = u64::from(inhibit_interrupts);
        }
        Ok(X86StiEffect::SetVif) => {
            state.interrupt_flags |= flags::bits::VIF;
            state.interrupt_inhibit = 0;
        }
        Err(X86StiFault::GeneralProtection) => return 0,
    }
    1
}
