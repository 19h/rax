//! Virtual fault metadata from translation and physical backing failures.

use crate::isa::arm::aarch64::cpu::AArch64Cpu;
use crate::isa::arm::aarch64::mmu::TranslationFault;
use crate::isa::arm::common::cpu::{AccessType, ArmError, MemoryFaultInfo, MemoryFaultType};
use crate::isa::arm::common::memory::MemoryError;

impl AArch64Cpu {
    /// Convert translation fault to ArmError.
    pub(crate) fn translation_fault_to_error(
        &self,
        fault: TranslationFault,
        access: AccessType,
    ) -> ArmError {
        use crate::isa::arm::aarch64::mmu::TranslationFaultType;

        let fault_type = match fault.fault_type {
            TranslationFaultType::Translation => MemoryFaultType::Translation,
            TranslationFaultType::Permission => MemoryFaultType::Permission,
            TranslationFaultType::Alignment => MemoryFaultType::Alignment,
            TranslationFaultType::AccessFlag => MemoryFaultType::AccessFlag,
            TranslationFaultType::AddressSize => MemoryFaultType::AddressSize,
            TranslationFaultType::ExternalAbort => MemoryFaultType::External,
        };

        self.last_fault_level
            .store(fault.level, std::sync::atomic::Ordering::Relaxed);

        ArmError::MemoryError(MemoryFaultInfo {
            address: fault.va,
            access,
            fault_type,
            stage2: fault.stage2,
        })
    }

    /// Preserve virtual address and access intent when physical backing fails.
    pub(super) fn physical_memory_error(
        &self,
        error: MemoryError,
        va: u64,
        pa: u64,
        width: usize,
        access: AccessType,
    ) -> ArmError {
        // Once translation succeeded, a missing physical target is an
        // external abort, rather than another stage-1 translation fault.
        let missing_physical = self.mmu.is_enabled()
            && matches!(
                error,
                MemoryError::OutOfBounds { .. } | MemoryError::Unmapped { .. }
            );
        let mut error = ArmError::from(error);
        if let ArmError::MemoryError(info) = &mut error {
            let offset = info
                .address
                .checked_sub(pa)
                .filter(|offset| *offset < width as u64)
                .unwrap_or(0);
            info.address = va.wrapping_add(offset);
            info.access = access;
            if missing_physical {
                info.fault_type = MemoryFaultType::External;
            }
        }
        error
    }
}
