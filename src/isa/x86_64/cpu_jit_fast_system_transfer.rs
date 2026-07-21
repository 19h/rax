//! Native-helper bridge for fault-precise Intel SYSENTER/SYSEXIT.

use super::X86_64Vcpu;
use crate::isa::x86_64::execute::system::{
    X86_INTERRUPT_CONTROL_RFLAGS_MASK, X86FastSystemTransferFault, X86FastSystemTransferState,
    commit_x86_fast_system_transfer_segments, evaluate_x86_sysenter, evaluate_x86_sysexit,
};
use crate::smir::lower::runtime::GuestRegs;

/// Evaluate one terminal SYSENTER (`kind=0`) or SYSEXIT (`kind=1`) from the
/// complete marshalled state. A zero return is noncommitting and requests
/// direct replay at the original PC; one commits fixed CS/SS caches plus the
/// helper-provided RSP/RIP, CPL, CS.L, and interrupt-control shadow.
pub(super) unsafe extern "C" fn rax_jit_fast_system_transfer(
    state: *mut GuestRegs,
    kind: u64,
    operand64: u64,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if operand64 > 1
        || kind > 1
        || state.cs_l != 1
        || state.efer & (1 << 10) == 0
        || !vcpu.sregs.cs.l
        || vcpu.sregs.efer & (1 << 10) == 0
    {
        return 0;
    }
    let Ok(cpl) = u8::try_from(state.cpl) else {
        return 0;
    };
    if cpl > 3 || kind == 0 && operand64 != 0 {
        return 0;
    }

    let inputs = X86FastSystemTransferState {
        cr0: state.cr0,
        efer: state.efer,
        cpl,
        rflags: state.interrupt_flags,
        sysenter_cs: state.sysenter_cs,
        sysenter_esp: state.sysenter_esp,
        sysenter_eip: state.sysenter_eip,
        rcx: state.gpr[1],
        rdx: state.gpr[2],
    };
    let effect = match kind {
        0 => evaluate_x86_sysenter(inputs),
        1 => evaluate_x86_sysexit(inputs, operand64 != 0),
        _ => unreachable!("validated fast-system-transfer helper kind changed"),
    };
    let effect = match effect {
        Ok(effect) => effect,
        Err(X86FastSystemTransferFault::GeneralProtection) => return 0,
    };

    commit_x86_fast_system_transfer_segments(vcpu, effect);
    state.gpr[4] = effect.rsp;
    state.exit_pc = effect.rip;
    state.cpl = u64::from(effect.cpl);
    state.cs_l = u64::from(effect.cs_long);
    state.interrupt_flags = effect.rflags & X86_INTERRUPT_CONTROL_RFLAGS_MASK;
    1
}
