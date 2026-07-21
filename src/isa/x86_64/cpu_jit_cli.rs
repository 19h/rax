//! Native helper for fault-precise x86 CLI execution.

use crate::isa::x86_64::execute::system::{
    X86CliEffect, X86CliFault, X86CliState, evaluate_x86_cli,
};
use crate::isa::x86_64::flags;
use crate::smir::lower::runtime::GuestRegs;

/// Evaluate and commit one CLI against the marshalled guest control state.
///
/// A zero result requests direct replay at the instruction PC. Every failure
/// is non-committing; a nonzero result clears exactly IF or VIF in the guest
/// interrupt-control shadow.
pub(super) unsafe extern "C" fn rax_jit_cli(state: *mut GuestRegs, requires_apx: u64) -> u64 {
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

    match evaluate_x86_cli(X86CliState {
        cr0: state.cr0,
        cr4: state.cr4,
        rflags: state.interrupt_flags,
        cpl,
    }) {
        Ok(X86CliEffect::ClearIf) => state.interrupt_flags &= !flags::bits::IF,
        Ok(X86CliEffect::ClearVif) => state.interrupt_flags &= !flags::bits::VIF,
        Err(X86CliFault::GeneralProtection) => return 0,
    }
    1
}
