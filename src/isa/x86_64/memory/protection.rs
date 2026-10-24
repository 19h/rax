//! Dynamic x86 data-access state and protection predicates (SDM Vol.3A 5.6/5.7).
use super::{AccessType, Mmu, SystemRegisters, TlbEntry, cr0};

#[derive(Default)]
pub(super) struct ProtectionState {
    alignment_check: bool,
    pkru: u32,
    implicit: bool,
}

impl Mmu {
    /// Supply current RFLAGS.AC and PKRU for subsequent guest linear accesses.
    /// Changing these controls takes effect on TLB hits without invalidation.
    /// CPU adapters refresh this derived context at each execution boundary.
    pub fn set_data_access_state(&mut self, rflags: u64, pkru: u32) {
        self.protection.alignment_check = rflags & (1 << 18) != 0;
        self.protection.pkru = pkru;
    }

    /// Run an implicit system-structure access with supervisor privilege.
    /// Guest access faults return through the closure before context restoration.
    pub(super) fn implicit_system_access<T>(&mut self, access: impl FnOnce(&mut Self) -> T) -> T {
        let original = self.protection.implicit;
        self.protection.implicit = true;
        let result = access(self);
        self.protection.implicit = original;
        result
    }

    pub(super) fn protection_error_flags(
        &self,
        access: AccessType,
        sregs: &SystemRegisters,
        mapping: &TlbEntry,
    ) -> u64 {
        let user = sregs.cs.selector & 3 == 3;
        let execute = matches!(access, AccessType::Execute);
        let smep = execute && !user && mapping.user && sregs.cr4 & (1 << 20) != 0;
        let smap = !execute
            && !user
            && mapping.user
            && sregs.cr4 & (1 << 21) != 0
            && (!self.protection.alignment_check || self.protection.implicit);
        // Protection keys classify the page, not the current CPL. AD always
        // denies data access; supervisor WD is bypassed only when CR0.WP=0.
        let rights = (self.protection.pkru >> (2 * u32::from(mapping.protection_key))) & 3;
        let key = !execute
            && self.long_mode(sregs)
            && mapping.user
            && sregs.cr4 & (1 << 22) != 0
            && (rights & 1 != 0
                || (rights & 2 != 0
                    && matches!(access, AccessType::Write)
                    && (user || sregs.cr0 & cr0::WP != 0)));
        u64::from(smep || smap || key) | (u64::from(key) << 5)
    }
}
