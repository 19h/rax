//! Deterministic guest-PMC helper for native x86-64 JIT regions.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::execute::system::{X86PmcState, read_x86_pmc};
use crate::smir::lower::runtime::GuestRegs;

/// Validate and read one guest PMC. Failure leaves EDX:EAX untouched so the
/// native block can deoptimize at the faulting instruction and let the direct
/// interpreter deliver #GP(0) precisely.
pub(super) unsafe extern "C" fn rax_jit_pmc(state: *mut GuestRegs) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *const X86_64Vcpu).as_ref() }) else {
        return 0;
    };
    let counter_base = vcpu.tsc().wrapping_sub(vcpu.tsc_adjust);
    let Ok(value) = read_x86_pmc(
        state.gpr[1] as u32,
        X86PmcState {
            cr0: state.cr0,
            cr4: state.cr4,
            cpl: state.cpl as u8,
        },
        counter_base,
    ) else {
        return 0;
    };

    state.gpr[0] = u64::from(value as u32);
    state.gpr[2] = u64::from((value >> 32) as u32);
    1
}
