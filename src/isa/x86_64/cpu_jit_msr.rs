//! Helper-backed RDMSR/WRMSR execution for native x86-64 JIT regions.

use super::X86_64Vcpu;
use crate::isa::x86_64::execute::system::{
    X86MsrState, read_x86_msr, sync_gs_cr0_shadow, validate_x86_msr_write,
};
use crate::smir::lower::runtime::GuestRegs;

fn msr_state(state: &GuestRegs) -> X86MsrState {
    X86MsrState {
        cr0: state.cr0,
        tsc_adjust: state.tsc_adjust,
        tsc_aux: state.tsc_aux,
        efer: state.efer,
        star: state.star,
        lstar: state.lstar,
        cstar: state.cstar,
        fmask: state.fmask,
        sysenter_cs: state.sysenter_cs,
        sysenter_esp: state.sysenter_esp,
        sysenter_eip: state.sysenter_eip,
        fs_base: state.fs_base,
        gs_base: state.gs_base,
        kernel_gs_base: state.kernel_gs_base,
    }
}

fn commit_msr_state(state: &mut GuestRegs, msr: X86MsrState) {
    state.tsc_adjust = msr.tsc_adjust;
    state.tsc_aux = msr.tsc_aux;
    state.efer = msr.efer;
    state.star = msr.star;
    state.lstar = msr.lstar;
    state.cstar = msr.cstar;
    state.fmask = msr.fmask;
    state.sysenter_cs = msr.sysenter_cs;
    state.sysenter_esp = msr.sysenter_esp;
    state.sysenter_eip = msr.sysenter_eip;
    state.fs_base = msr.fs_base;
    state.gs_base = msr.gs_base;
    state.kernel_gs_base = msr.kernel_gs_base;
}

/// Execute one native MSR access against marshalled guest state. A zero return
/// requests precise direct replay at the faulting guest PC; nonzero means the
/// access completed and every output/state transition has committed.
///
/// # Safety
///
/// `state` must reference the live [`GuestRegs`] image for the owning
/// [`X86_64Vcpu`], and `state.ctx` must contain that vCPU's valid address.
pub(super) unsafe extern "C" fn rax_jit_msr(state: *mut GuestRegs, write: u64) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    let index = state.gpr[1] as u32;
    let current = msr_state(state);
    let tsc = vcpu.tsc();

    if write != 0 {
        let value =
            ((state.gpr[2] & u64::from(u32::MAX)) << 32) | (state.gpr[0] & u64::from(u32::MAX));
        let Ok(effect) = validate_x86_msr_write(index, value, current, tsc) else {
            return 0;
        };
        commit_msr_state(state, effect.state);
        if effect.flush_tlb {
            vcpu.mmu.flush_tlb();
        }
        if effect.sync_gs_cr0_shadow {
            sync_gs_cr0_shadow(vcpu, effect.state.gs_base);
        }
    } else {
        let Ok(value) = read_x86_msr(index, current, tsc) else {
            return 0;
        };
        state.gpr[0] = u64::from(value as u32);
        state.gpr[2] = u64::from((value >> 32) as u32);
    }
    1
}
