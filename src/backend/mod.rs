//! Backend abstraction layer.
//!
//! This module provides traits and factory functions for VM backends.

pub mod emulator;

#[cfg(feature = "kvm")]
pub mod kvm;

use std::any::Any;
use std::sync::Arc;

use vm_memory::GuestMemoryMmap;

use crate::config::BackendKind;
use crate::cpu::VCpu;
#[cfg(not(feature = "kvm"))]
use crate::error::Error;
use crate::error::Result;

/// Abstract VM interface.
///
/// Represents a virtual machine that can create vCPUs and manage interrupts.
pub trait Vm: Send + Sync {
    /// Create a new vCPU.
    fn create_vcpu(&self, id: u32, mem: Arc<GuestMemoryMmap>) -> Result<Box<dyn VCpu>>;

    /// Set IRQ line level.
    fn set_irq_line(&self, irq: u32, level: bool) -> Result<()>;

    /// Get reference as Any for downcasting to concrete types.
    fn as_any(&self) -> &dyn Any;
}

/// Abstract backend interface.
///
/// Represents a virtualization backend (KVM, emulator, etc.).
pub trait Backend: Send + Sync {
    /// Backend name for logging.
    fn name(&self) -> &'static str;

    /// Create a new VM.
    fn create_vm(&self) -> Result<Box<dyn Vm>>;
}

/// Create a backend based on configuration.
pub fn create(kind: BackendKind) -> Result<Box<dyn Backend>> {
    match kind {
        #[cfg(feature = "kvm")]
        BackendKind::Kvm => Ok(Box::new(kvm::KvmBackend::new()?)),
        #[cfg(not(feature = "kvm"))]
        BackendKind::Kvm => Err(Error::InvalidConfig(
            "KVM backend not available (compile with --features kvm)".to_string(),
        )),
        BackendKind::Emulator => Ok(Box::new(emulator::EmulatorBackend::new())),
    }
}
