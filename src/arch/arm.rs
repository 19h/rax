//! ARM architecture family support.
//!
//! This module provides architecture definitions for the ARM family:
//! - AArch64 (64-bit ARMv8-A)
//! - ARMv7-A (32-bit Cortex-A series)
//! - ARMv8-A AArch32 (32-bit mode on ARMv8)
//! - Cortex-M (Thumb-2 based microcontrollers)
//! - Cortex-R (real-time processors)

use std::fs::File;
use std::io::Read;

use goblin::elf::Elf;
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

use crate::arch::{Arch, BootInfo};
use crate::config::VmConfig;
use crate::cpu::CpuState;
use crate::devices::bus::{IoBus, MmioBus};
use crate::error::{Error, Result};

// =============================================================================
// ARM Boot Info
// =============================================================================

/// Boot information for ARM platforms.
#[derive(Clone, Debug)]
pub struct ArmBootInfo {
    /// Entry point address
    pub entry_point: u64,
    /// Load address of the image
    pub load_addr: u64,
    /// Size of the loaded image
    pub image_size: u64,
    /// Device tree blob address (if applicable)
    pub dtb_addr: Option<u64>,
    /// Initial stack pointer
    pub initial_sp: Option<u64>,
}

// =============================================================================
// AArch64 Architecture
// =============================================================================

/// Default UART base address for PL011 (ARM PrimeCell UART)
const AARCH64_UART_BASE: u64 = 0x0900_0000;
/// Default GIC (Generic Interrupt Controller) distributor base
const AARCH64_GICD_BASE: u64 = 0x0800_0000;
/// Default GIC CPU interface base
const AARCH64_GICC_BASE: u64 = 0x0801_0000;
/// Default RAM base address
const AARCH64_RAM_BASE: u64 = 0x4000_0000;

pub struct Aarch64Arch;

impl Aarch64Arch {
    pub fn new() -> Self {
        Aarch64Arch
    }

    fn load_elf(mem: &GuestMemoryMmap, buf: &[u8]) -> Result<ArmBootInfo> {
        let elf =
            Elf::parse(buf).map_err(|e| Error::KernelLoad(format!("ELF parse error: {e}")))?;

        if !elf.is_64 {
            return Err(Error::KernelLoad("AArch64 ELF must be 64-bit".to_string()));
        }

        // Check for ARM64 machine type
        if elf.header.e_machine != goblin::elf::header::EM_AARCH64 {
            return Err(Error::KernelLoad(format!(
                "Expected AArch64 ELF (e_machine=183), got {}",
                elf.header.e_machine
            )));
        }

        let mut min_addr = u64::MAX;
        let mut max_addr = 0u64;

        for ph in &elf.program_headers {
            if ph.p_type != goblin::elf::program_header::PT_LOAD {
                continue;
            }
            let file_start = ph.p_offset as usize;
            let file_end = file_start
                .checked_add(ph.p_filesz as usize)
                .ok_or_else(|| Error::KernelLoad("ELF segment overflow".to_string()))?;
            if file_end > buf.len() {
                return Err(Error::KernelLoad("ELF segment out of range".to_string()));
            }
            let load_addr = if ph.p_paddr != 0 {
                ph.p_paddr
            } else {
                ph.p_vaddr
            };

            mem.write_slice(&buf[file_start..file_end], GuestAddress(load_addr))?;

            min_addr = min_addr.min(load_addr);
            max_addr = max_addr.max(load_addr + ph.p_memsz);
        }

        Ok(ArmBootInfo {
            entry_point: elf.entry,
            load_addr: min_addr,
            image_size: max_addr.saturating_sub(min_addr),
            dtb_addr: None,
            initial_sp: None,
        })
    }

    fn load_raw(mem: &GuestMemoryMmap, buf: &[u8]) -> Result<ArmBootInfo> {
        let load_addr = AARCH64_RAM_BASE;
        mem.write_slice(buf, GuestAddress(load_addr))?;

        Ok(ArmBootInfo {
            entry_point: load_addr,
            load_addr,
            image_size: buf.len() as u64,
            dtb_addr: None,
            initial_sp: None,
        })
    }
}

impl Arch for Aarch64Arch {
    fn name(&self) -> &'static str {
        "aarch64"
    }

    fn setup_devices(&self, _io_bus: &mut IoBus, _mmio_bus: &mut MmioBus) -> Result<()> {
        // ARM uses MMIO for everything, no port I/O
        // TODO: Register PL011 UART, GIC, etc.
        Ok(())
    }

    fn serial_mmio_base(&self) -> Option<u64> {
        Some(AARCH64_UART_BASE)
    }

    fn serial_irq(&self) -> Option<u32> {
        Some(33) // SPI 1 (first SPI is 32)
    }

    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
        let mut file = File::open(&config.kernel)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        if buf.len() < 4 {
            return Err(Error::KernelLoad("image is too small".to_string()));
        }

        let info = if buf.starts_with(b"\x7fELF") {
            Self::load_elf(mem, &buf)?
        } else {
            Self::load_raw(mem, &buf)?
        };

        Ok(BootInfo::Arm(info))
    }

    #[cfg(all(feature = "kvm", target_os = "linux"))]
    fn init_vm(&self, _vm: &crate::backend::kvm::KvmVm, _boot: &BootInfo) -> Result<()> {
        // TODO: Initialize KVM for ARM
        Err(Error::InvalidConfig(
            "KVM for AArch64 not yet implemented".to_string(),
        ))
    }

    fn initial_cpu_state(&self, mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState> {
        use crate::cpu::{Aarch64Registers, Aarch64SystemRegisters};

        let boot = match boot {
            BootInfo::Arm(info) => info,
            _ => return Err(Error::InvalidConfig("expected ARM boot info".to_string())),
        };

        let mut regs = Aarch64Registers::default();
        regs.pc = boot.entry_point;

        // Set up initial stack at end of memory
        let mem_end = mem.last_addr().raw_value().saturating_add(1);
        let sp = (mem_end - 16) & !0xF; // 16-byte aligned
        regs.sp = sp;

        // X0 = DTB address (if present), otherwise 0
        regs.x[0] = boot.dtb_addr.unwrap_or(0);

        let mut sregs = Aarch64SystemRegisters::default();
        // Set up minimal system state for EL1
        sregs.sctlr_el1 = 0; // MMU off, caches off initially

        Ok(CpuState::aarch64(regs, sregs))
    }
}

// =============================================================================
// ARMv7-A Architecture
// =============================================================================

/// Default UART base address for ARMv7-A platforms
const ARMV7A_UART_BASE: u64 = 0x1010_0000; // Common for Versatile/RealView

pub struct Armv7aArch;

impl Armv7aArch {
    pub fn new() -> Self {
        Armv7aArch
    }
}

impl Arch for Armv7aArch {
    fn name(&self) -> &'static str {
        "armv7a"
    }

    fn setup_devices(&self, _io_bus: &mut IoBus, _mmio_bus: &mut MmioBus) -> Result<()> {
        Ok(())
    }

    fn serial_mmio_base(&self) -> Option<u64> {
        Some(ARMV7A_UART_BASE)
    }

    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
        let mut file = File::open(&config.kernel)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        if buf.len() < 4 {
            return Err(Error::KernelLoad("image is too small".to_string()));
        }

        // Load at 0x10000 (common for ARM Linux zImage)
        let load_addr = 0x0001_0000u64;
        mem.write_slice(&buf, GuestAddress(load_addr))?;

        let info = ArmBootInfo {
            entry_point: load_addr,
            load_addr,
            image_size: buf.len() as u64,
            dtb_addr: None,
            initial_sp: None,
        };

        Ok(BootInfo::Arm(info))
    }

    #[cfg(all(feature = "kvm", target_os = "linux"))]
    fn init_vm(&self, _vm: &crate::backend::kvm::KvmVm, _boot: &BootInfo) -> Result<()> {
        Err(Error::InvalidConfig(
            "KVM for ARMv7-A not supported".to_string(),
        ))
    }

    fn initial_cpu_state(&self, mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState> {
        use crate::cpu::{Aarch32Registers, Aarch32SystemRegisters};

        let boot = match boot {
            BootInfo::Arm(info) => info,
            _ => return Err(Error::InvalidConfig("expected ARM boot info".to_string())),
        };

        let mut regs = Aarch32Registers::default();
        regs.pc = boot.entry_point as u32;

        // Set up initial stack at end of memory
        let mem_end = mem.last_addr().raw_value().saturating_add(1);
        let sp = ((mem_end - 16) & !0x7) as u32; // 8-byte aligned
        regs.sp = sp;

        // R0 = 0 (unused), R1 = machine type, R2 = DTB address
        regs.r[0] = 0;
        regs.r[1] = 0xFFFF_FFFF; // Machine type (0xFFFFFFFF = use DTB)
        regs.r[2] = boot.dtb_addr.unwrap_or(0) as u32;

        let sregs = Aarch32SystemRegisters::default();

        Ok(CpuState::aarch32(regs, sregs))
    }
}

// =============================================================================
// ARMv8-A AArch32 Mode
// =============================================================================

pub struct Armv8a32Arch;

impl Armv8a32Arch {
    pub fn new() -> Self {
        Armv8a32Arch
    }
}

impl Arch for Armv8a32Arch {
    fn name(&self) -> &'static str {
        "armv8a32"
    }

    fn setup_devices(&self, _io_bus: &mut IoBus, _mmio_bus: &mut MmioBus) -> Result<()> {
        Ok(())
    }

    fn serial_mmio_base(&self) -> Option<u64> {
        Some(ARMV7A_UART_BASE)
    }

    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
        // Same as ARMv7-A for now
        Armv7aArch::new().load_kernel(mem, config)
    }

    #[cfg(all(feature = "kvm", target_os = "linux"))]
    fn init_vm(&self, _vm: &crate::backend::kvm::KvmVm, _boot: &BootInfo) -> Result<()> {
        Err(Error::InvalidConfig(
            "KVM for ARMv8-A AArch32 not supported".to_string(),
        ))
    }

    fn initial_cpu_state(&self, mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState> {
        // Same as ARMv7-A for now
        Armv7aArch::new().initial_cpu_state(mem, boot)
    }
}

// =============================================================================
// Cortex-M Architecture
// =============================================================================

/// Default UART base for Cortex-M (varies by vendor, using ARM MPS2 as example)
const CORTEXM_UART_BASE: u64 = 0x4000_4000;
/// Default vector table address
const CORTEXM_VTOR_DEFAULT: u32 = 0x0000_0000;

pub struct CortexMArch;

impl CortexMArch {
    pub fn new() -> Self {
        CortexMArch
    }
}

impl Arch for CortexMArch {
    fn name(&self) -> &'static str {
        "cortex-m"
    }

    fn setup_devices(&self, _io_bus: &mut IoBus, _mmio_bus: &mut MmioBus) -> Result<()> {
        Ok(())
    }

    fn serial_mmio_base(&self) -> Option<u64> {
        Some(CORTEXM_UART_BASE)
    }

    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
        let mut file = File::open(&config.kernel)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        if buf.len() < 8 {
            return Err(Error::KernelLoad("image is too small".to_string()));
        }

        // Cortex-M vector table starts at load address
        // First word is initial SP, second word is reset handler
        let load_addr = 0u64;
        mem.write_slice(&buf, GuestAddress(load_addr))?;

        // Read initial SP and reset handler from vector table
        let initial_sp = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let reset_handler = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);

        let info = ArmBootInfo {
            entry_point: reset_handler as u64,
            load_addr,
            image_size: buf.len() as u64,
            dtb_addr: None,
            initial_sp: Some(initial_sp as u64),
        };

        Ok(BootInfo::Arm(info))
    }

    #[cfg(all(feature = "kvm", target_os = "linux"))]
    fn init_vm(&self, _vm: &crate::backend::kvm::KvmVm, _boot: &BootInfo) -> Result<()> {
        Err(Error::InvalidConfig(
            "KVM for Cortex-M not supported".to_string(),
        ))
    }

    fn initial_cpu_state(&self, _mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState> {
        use crate::cpu::{CortexMRegisters, CortexMSystemRegisters};

        let boot = match boot {
            BootInfo::Arm(info) => info,
            _ => return Err(Error::InvalidConfig("expected ARM boot info".to_string())),
        };

        let mut regs = CortexMRegisters::default();
        // PC must have LSB set for Thumb mode (Cortex-M is always Thumb)
        regs.pc = (boot.entry_point as u32) & !1;
        regs.msp = boot.initial_sp.unwrap_or(0x2000_0000) as u32;
        regs.xpsr = 0x0100_0000; // Thumb bit set

        let mut sregs = CortexMSystemRegisters::default();
        sregs.vtor = boot.load_addr as u32;

        Ok(CpuState::cortex_m(regs, sregs))
    }
}

// =============================================================================
// Cortex-R Architecture
// =============================================================================

pub struct CortexRArch;

impl CortexRArch {
    pub fn new() -> Self {
        CortexRArch
    }
}

impl Arch for CortexRArch {
    fn name(&self) -> &'static str {
        "cortex-r"
    }

    fn setup_devices(&self, _io_bus: &mut IoBus, _mmio_bus: &mut MmioBus) -> Result<()> {
        Ok(())
    }

    fn serial_mmio_base(&self) -> Option<u64> {
        Some(0x1010_0000) // Typical for Cortex-R platforms
    }

    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
        // Similar to ARMv7-A
        Armv7aArch::new().load_kernel(mem, config)
    }

    #[cfg(all(feature = "kvm", target_os = "linux"))]
    fn init_vm(&self, _vm: &crate::backend::kvm::KvmVm, _boot: &BootInfo) -> Result<()> {
        Err(Error::InvalidConfig(
            "KVM for Cortex-R not supported".to_string(),
        ))
    }

    fn initial_cpu_state(&self, mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState> {
        // Cortex-R uses AArch32 state
        Armv7aArch::new().initial_cpu_state(mem, boot)
    }
}
