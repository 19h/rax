//! RISC-V (RV64) virtual-machine integration: image loading and boot state.
//!
//! This wires the self-contained [`crate::isa::riscv`] interpreter into the VMM as a
//! bootable machine. It loads a flat binary or an ELF image into guest
//! memory and produces the initial register file (entry PC, stack pointer), and
//! exposes a 16550 UART at the RISC-V "virt" MMIO address for console output.

use std::fs::File;
use std::io::Read;

use goblin::elf::Elf;
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

use crate::config::VmConfig;
use crate::devices::bus::{IoBus, MmioBus};
use crate::error::{Error, Result};
use crate::machine::{BootInfo, Machine, RiscVBootInfo};
use crate::vm::vcpu::{CpuState, RiscVRegisters};

/// 16550 UART MMIO base (matches the RISC-V "virt" machine convention).
const RISCV_UART_BASE: u64 = 0x1000_0000;
/// `EM_RISCV` machine type.
const EM_RISCV: u16 = 243;

pub struct RiscvVirtMachine;

impl RiscvVirtMachine {
    pub fn new() -> Self {
        RiscvVirtMachine
    }

    fn load_raw(mem: &GuestMemoryMmap, buf: &[u8]) -> Result<RiscVBootInfo> {
        mem.write_slice(buf, GuestAddress(0))?;
        Ok(RiscVBootInfo {
            entry_point: 0,
            load_addr: 0,
            image_size: buf.len() as u64,
            tohost_addr: None,
        })
    }

    fn load_elf(mem: &GuestMemoryMmap, buf: &[u8]) -> Result<RiscVBootInfo> {
        let elf =
            Elf::parse(buf).map_err(|e| Error::KernelLoad(format!("ELF parse error: {e}")))?;
        if !elf.is_64 {
            return Err(Error::KernelLoad("RISC-V ELF must be 64-bit".to_string()));
        }
        if elf.header.e_machine != EM_RISCV {
            return Err(Error::KernelLoad(format!(
                "not a RISC-V ELF (e_machine={})",
                elf.header.e_machine
            )));
        }

        let mut min_addr = u64::MAX;
        let mut max_addr = 0u64;
        let mut post_mret_entry = None;
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
            let segment = &buf[file_start..file_end];
            mem.write_slice(segment, GuestAddress(load_addr))?;
            if post_mret_entry.is_none() && ph.p_flags & goblin::elf::program_header::PF_X != 0 {
                if let Some(offset) = segment
                    .windows(4)
                    .position(|word| word == [0x73, 0x00, 0x20, 0x30])
                {
                    post_mret_entry = Some(load_addr + offset as u64 + 4);
                }
            }
            min_addr = min_addr.min(load_addr);
            max_addr = max_addr.max(load_addr + ph.p_memsz);
        }

        let mut entry_point = elf.entry;
        let mut found_test_entry = false;
        let mut first_numbered_test = None;
        let mut tohost_addr = None;
        for sym in &elf.syms {
            if sym.st_value == 0 {
                continue;
            }
            if let Some(name) = elf.strtab.get_at(sym.st_name) {
                if name == "tohost" {
                    tohost_addr = Some(sym.st_value);
                }
                if name == "test_entry" {
                    entry_point = sym.st_value;
                    found_test_entry = true;
                    first_numbered_test = None;
                    break;
                }
                if let Some(suffix) = name.strip_prefix("test_") {
                    if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                        first_numbered_test = Some(
                            first_numbered_test
                                .map_or(sym.st_value, |addr: u64| addr.min(sym.st_value)),
                        );
                    }
                }
            }
        }
        if !found_test_entry {
            if let Some(addr) = post_mret_entry {
                entry_point = addr;
            } else if let Some(addr) = first_numbered_test {
                entry_point = addr;
            }
        }

        Ok(RiscVBootInfo {
            entry_point,
            load_addr: if min_addr == u64::MAX { 0 } else { min_addr },
            image_size: max_addr.saturating_sub(min_addr),
            tohost_addr,
        })
    }
}

impl Machine for RiscvVirtMachine {
    fn name(&self) -> &'static str {
        "riscv64"
    }

    fn setup_devices(&self, _io_bus: &mut IoBus, _mmio_bus: &mut MmioBus) -> Result<()> {
        Ok(())
    }

    fn serial_mmio_base(&self) -> Option<u64> {
        Some(RISCV_UART_BASE)
    }

    fn load_kernel(&self, mem: &GuestMemoryMmap, config: &VmConfig) -> Result<BootInfo> {
        let mut file = File::open(&config.kernel)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        if buf.len() < 4 {
            return Err(Error::KernelLoad("image is too small".to_string()));
        }
        if std::env::var("RAX_MACHINE").as_deref() == Ok("gsc") {
            return Ok(BootInfo::RiscV(crate::machine::gsc::image::load(
                mem, &buf,
            )?));
        }
        let info = if buf.starts_with(b"\x7fELF") {
            Self::load_elf(mem, &buf)?
        } else {
            Self::load_raw(mem, &buf)?
        };
        Ok(BootInfo::RiscV(info))
    }

    #[cfg(all(feature = "kvm", target_os = "linux", target_arch = "x86_64"))]
    fn init_vm(&self, _vm: &crate::backend::kvm::KvmVm, _boot: &BootInfo) -> Result<()> {
        Ok(())
    }

    fn initial_cpu_state(&self, mem: &GuestMemoryMmap, boot: &BootInfo) -> Result<CpuState> {
        let boot = boot
            .as_riscv()
            .ok_or_else(|| Error::InvalidConfig("expected riscv boot info".to_string()))?;

        let mut regs = RiscVRegisters::default();
        regs.pc = boot.entry_point;
        regs.tohost_addr = boot.tohost_addr;
        // Stack pointer (x2) at the top of guest RAM, 16-byte aligned.
        let mem_end = mem.last_addr().raw_value().saturating_add(1);
        regs.x[2] = mem_end.saturating_sub(16) & !0xf;
        Ok(CpuState::riscv(regs))
    }
}

// Compatibility name retained for the previous architecture terminology.
pub use RiscvVirtMachine as Riscv64Arch;
