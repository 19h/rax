//! Translation-fault conversion preserving architectural access intent.

use crate::isa::arm::aarch64::cpu::AArch64Cpu;
use crate::isa::arm::aarch64::mmu::TranslationFault;
use crate::isa::arm::common::cpu::{AccessType, ArmError, MemoryFaultInfo, MemoryFaultType};

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
}
