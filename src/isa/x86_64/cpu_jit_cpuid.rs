//! Deterministic guest-CPUID helper for native x86-64 JIT regions.

use crate::isa::x86_64::execute::system::{X86CpuidState, evaluate_cpuid};
use crate::smir::lower::runtime::GuestRegs;

/// Inputs and mutable guest-profile state are read from the marshalled register
/// file; no host CPUID result is observable by the guest.
pub(super) unsafe extern "C" fn rax_jit_cpuid(state: *mut GuestRegs) {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return;
    };
    let leaf = state.gpr[0] as u32;
    let subleaf = state.gpr[1] as u32;
    let (eax, ebx, ecx, edx) = evaluate_cpuid(
        leaf,
        subleaf,
        X86CpuidState {
            cr4: state.cr4,
            xcr0: state.xcr0,
            xeon_phi_avx512: state.cpuid_xeon_phi_avx512 != 0,
            vp2intersect: state.cpuid_vp2intersect != 0,
            sse4a: state.cpuid_sse4a != 0,
            tbm: state.cpuid_tbm != 0,
            apx: state.apx_enabled != 0,
        },
    );
    state.gpr[0] = u64::from(eax);
    state.gpr[3] = u64::from(ebx);
    state.gpr[1] = u64::from(ecx);
    state.gpr[2] = u64::from(edx);
}
