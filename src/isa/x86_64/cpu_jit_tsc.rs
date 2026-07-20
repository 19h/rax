//! Emulated guest timestamp-counter helper for native x86-64 JIT regions.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::smir::lower::runtime::GuestRegs;

/// Read the same emulator clock used by the direct RDTSC/RDTSCP path and
/// commit only the two architectural timestamp destinations. Privilege checks
/// execute in native code before this helper is called.
pub(super) unsafe extern "C" fn rax_jit_tsc(state: *mut GuestRegs) {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *const X86_64Vcpu).as_ref() }) else {
        return;
    };
    let tsc = vcpu.tsc();
    state.gpr[0] = u64::from(tsc as u32);
    state.gpr[2] = u64::from((tsc >> 32) as u32);
}
