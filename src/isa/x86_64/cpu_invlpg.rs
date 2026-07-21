//! Direct and helper-backed INVLPG translation-cache invalidation.

use super::X86_64Vcpu;

impl X86_64Vcpu {
    /// Apply one architectural INVLPG invalidation to every translation-
    /// dependent software cache. The MMU may conservatively flush more than the
    /// named page, so decode/JIT caches follow the same full-flush policy; this
    /// also covers large-page aliases whose extent cannot be recovered after a
    /// page-table entry changes. Callers must already have handled the 64-bit
    /// non-canonical-address no-op rule.
    pub(in crate::isa::x86_64) fn invalidate_linear_translation(&mut self, addr: u64) {
        self.mmu.invlpg(addr);
        self.decode_cache.iter_mut().for_each(|entry| {
            entry.rip = 0;
            entry.bytes_len = 0;
        });
        #[cfg(all(
            feature = "smir-jit",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            self.jit_cache.clear();
            self.jit_hot.clear();
            self.jit_ineligible.clear();
            self.jit_ineligible_dirty.clear();
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
use crate::isa::x86_64::execute::system::is_canonical_48;
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
use crate::smir::lower::runtime::GuestRegs;

/// Validate and apply one native INVLPG operation.
///
/// The helper returns zero without changing any cache when APX is unavailable,
/// the runtime state is not 64-bit mode, or effective CPL is nonzero. A
/// non-canonical 64-bit address succeeds without invalidating anything, as
/// required by the architectural NOP rule.
///
/// # Safety
///
/// `state` must reference the live [`GuestRegs`] image for the owning
/// [`X86_64Vcpu`], and `state.ctx` must contain that vCPU's valid address.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_invlpg(
    state: *mut GuestRegs,
    addr: u64,
    requires_apx: u64,
) -> u64 {
    if requires_apx > 1 {
        return 0;
    }
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if (requires_apx != 0 && state.apx_enabled == 0) || state.cs_l == 0 || state.cpl != 0 {
        return 0;
    }
    if !is_canonical_48(addr) {
        return 1;
    }
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    vcpu.invalidate_linear_translation(addr);
    1
}
