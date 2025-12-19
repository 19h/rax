//! Architecture abstraction layer.

pub mod x86_64;

use vm_memory::{GuestAddress, GuestMemoryMmap};

#[cfg(feature = "kvm")]
use crate::backend::kvm::KvmVm;
use crate::config::{ArchKind, VmConfig};
use crate::cpu::CpuState;
use crate::devices::bus::IoBus;
use crate::error::Result;

/// Boot information returned after kernel loading.
pub struct BootInfo {
    /// Kernel entry point address.
    pub entry_point: u64,
    /// Address of boot_params structure.
    pub boot_params_addr: GuestAddress,
    /// TSS address for KVM.
    pub tss_addr: u64,
    /// Identity map address for KVM.
    pub identity_map_addr: u64,
}

/// Architecture abstraction trait.
pub trait Arch: Send + Sync {
    /// Architecture name.
    fn name(&self) -> &'static str;

    /// Setup architecture-specific I/O devices.
    fn setup_devices(&self, io_bus: &mut IoBus) -> Result<()>;

    /// Load kernel and prepare boot environment.
    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo>;

    /// Initialize VM-level state (IRQ chip, PIT, TSS, identity map).
    /// This is KVM-specific.
    #[cfg(feature = "kvm")]
    fn init_vm(&self, vm: &KvmVm, boot: &BootInfo) -> Result<()>;

    /// Get initial CPU state for booting.
    /// Writes necessary structures (GDT, page tables) to guest memory
    /// and returns the initial CPU state.
    fn initial_cpu_state(&self, mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState>;
}

/// Create an architecture implementation from kind.
pub fn from_kind(kind: ArchKind) -> Box<dyn Arch> {
    match kind {
        ArchKind::X86_64 => Box::new(x86_64::X86_64Arch::new()),
    }
}
