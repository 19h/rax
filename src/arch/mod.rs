//! Architecture abstraction layer.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod x86_64;

pub mod hexagon;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub mod x86_64 {
    use vm_memory::GuestMemoryMmap;

    use super::{Arch, BootInfo};
    use crate::config::VmConfig;
    use crate::cpu::CpuState;
    use crate::devices::bus::{IoBus, MmioBus};
    use crate::error::{Error, Result};

    pub struct X86_64Arch;

    impl X86_64Arch {
        pub fn new() -> Self {
            X86_64Arch
        }

        fn unsupported<T>() -> Result<T> {
            Err(Error::InvalidConfig(
                "x86_64 architecture is not supported on this host".to_string(),
            ))
        }
    }

    impl Arch for X86_64Arch {
        fn name(&self) -> &'static str {
            "x86_64"
        }

        fn setup_devices(&self, _io_bus: &mut IoBus, _mmio_bus: &mut MmioBus) -> Result<()> {
            Self::unsupported()
        }

        fn load_kernel(&self, _mem: &GuestMemoryMmap, _config: &VmConfig) -> Result<BootInfo> {
            Self::unsupported()
        }

        fn initial_cpu_state(
            &self,
            _mem: &GuestMemoryMmap,
            _boot: &BootInfo,
        ) -> Result<CpuState> {
            Self::unsupported()
        }

        #[cfg(all(feature = "kvm", target_os = "linux"))]
        fn init_vm(&self, _vm: &crate::backend::kvm::KvmVm, _boot: &BootInfo) -> Result<()> {
            Self::unsupported()
        }
    }
}

use vm_memory::{GuestAddress, GuestMemoryMmap};

#[cfg(all(feature = "kvm", target_os = "linux"))]
use crate::backend::kvm::KvmVm;
use crate::config::{ArchKind, VmConfig};
use crate::cpu::CpuState;
use crate::devices::bus::{IoBus, MmioBus};
use crate::error::Result;

/// Boot information for x86_64 kernel loading.
pub struct X86_64BootInfo {
    pub entry_point: u64,
    pub boot_params_addr: GuestAddress,
    pub tss_addr: u64,
    pub identity_map_addr: u64,
}

/// Boot information for Hexagon bare-metal loading.
pub struct HexagonBootInfo {
    pub entry_point: u64,
    pub load_addr: u64,
    pub image_size: u64,
}

/// Boot information returned after image loading.
pub enum BootInfo {
    X86_64(X86_64BootInfo),
    Hexagon(HexagonBootInfo),
}

impl BootInfo {
    pub fn entry_point(&self) -> u64 {
        match self {
            BootInfo::X86_64(info) => info.entry_point,
            BootInfo::Hexagon(info) => info.entry_point,
        }
    }

    pub fn as_x86_64(&self) -> Option<&X86_64BootInfo> {
        match self {
            BootInfo::X86_64(info) => Some(info),
            _ => None,
        }
    }

    pub fn as_hexagon(&self) -> Option<&HexagonBootInfo> {
        match self {
            BootInfo::Hexagon(info) => Some(info),
            _ => None,
        }
    }
}

/// Architecture abstraction trait.
pub trait Arch: Send + Sync {
    /// Architecture name.
    fn name(&self) -> &'static str;

    /// Setup architecture-specific I/O devices.
    fn setup_devices(&self, io_bus: &mut IoBus, mmio_bus: &mut MmioBus) -> Result<()>;

    /// Optional MMIO base for the serial device.
    fn serial_mmio_base(&self) -> Option<u64> {
        None
    }

    /// Optional IRQ line for the serial device.
    fn serial_irq(&self) -> Option<u32> {
        None
    }

    /// Load kernel and prepare boot environment.
    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo>;

    /// Initialize VM-level state (IRQ chip, PIT, TSS, identity map).
    /// This is KVM-specific.
    #[cfg(all(feature = "kvm", target_os = "linux"))]
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
        ArchKind::Hexagon => Box::new(hexagon::HexagonArch::new()),
    }
}
