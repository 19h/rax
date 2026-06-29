//! RISC-V (RV64) architecture integration: image loading and boot state.
//!
//! This wires the self-contained [`crate::riscv`] interpreter into the VMM as a
//! bootable architecture. It loads a flat binary or an ELF image into guest
//! memory and produces the initial register file (entry PC, stack pointer), and
//! exposes a 16550 UART at the RISC-V "virt" MMIO address for console output.

use std::fs::File;
use std::io::Read;

use goblin::elf::Elf;
use vm_memory::{Address, Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

use crate::arch::{Arch, BootInfo, RiscVBootInfo};
use crate::config::VmConfig;
use crate::cpu::{CpuState, RiscVRegisters};
use crate::devices::bus::{IoBus, MmioBus};
use crate::error::{Error, Result};

/// 16550 UART MMIO base (matches the RISC-V "virt" machine convention).
const RISCV_UART_BASE: u64 = 0x1000_0000;
/// `EM_RISCV` machine type.
const EM_RISCV: u16 = 243;

/// Cr50/Ti50-family "SignedHeader" magic at file offset 0.
const GSC_HEADER_MAGIC: u32 = 0xFFFF_FFFD;
/// SignedHeader region-descriptor: total image size in the slot.
const HDR_IMAGE_SIZE: usize = 0x328;
/// SignedHeader region-descriptor: read-only/load base address.
const HDR_RO_BASE: usize = 0x32C;
/// SignedHeader vector struct: the `_start` entry point.
const HDR_ENTRY: usize = 0x404;
/// A slot whose image is at least this large is treated as an RW (firmware)
/// slot when selecting the entry of a multi-slot A/B flash.
const GSC_RW_IMAGE_THRESHOLD: u64 = 0x3_0000;

/// Read a little-endian `u32` from `buf` at `off`, if in range.
fn rd_u32(buf: &[u8], off: usize) -> Option<u32> {
    buf.get(off..off + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Parse a hex (or `0x`-prefixed) environment variable.
fn env_hex(name: &str) -> Option<u64> {
    let raw = std::env::var(name).ok()?;
    let s = raw.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Choose the entry PC for a GSC image. A single self-contained slot (header-0
/// `image_size` equals the whole file, e.g. `fw.bin`) uses header-0's `_start`.
/// A multi-slot A/B full-flash (e.g. `ti50.bin.prod`) uses the first
/// (lowest-offset) RW-sized slot's `_start`, found by scanning 4 KiB-aligned
/// SignedHeaders.
fn select_gsc_entry(buf: &[u8], load_base: u64) -> u64 {
    let hdr0_image = rd_u32(buf, HDR_IMAGE_SIZE).map(|v| v as u64).unwrap_or(0);
    let hdr0_entry = rd_u32(buf, HDR_ENTRY)
        .map(|v| v as u64)
        .unwrap_or(load_base);
    if hdr0_image == buf.len() as u64 {
        return hdr0_entry;
    }
    let mut off = 0x1000usize;
    while off + HDR_ENTRY + 4 <= buf.len() {
        if rd_u32(buf, off) == Some(GSC_HEADER_MAGIC) {
            let img = rd_u32(buf, off + HDR_IMAGE_SIZE)
                .map(|v| v as u64)
                .unwrap_or(0);
            if img >= GSC_RW_IMAGE_THRESHOLD {
                if let Some(e) = rd_u32(buf, off + HDR_ENTRY) {
                    if e != 0 {
                        return e as u64;
                    }
                }
            }
        }
        off += 0x1000;
    }
    hdr0_entry
}

pub struct Riscv64Arch;

impl Riscv64Arch {
    pub fn new() -> Self {
        Riscv64Arch
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

    /// Load a Google Security Chip (Ti50/Dauntless) image.
    ///
    /// GSC images begin with a Cr50/Ti50-family "SignedHeader" (magic
    /// [`GSC_HEADER_MAGIC`]). The whole flash is execute-in-place: it is mapped
    /// verbatim at the header's `ro_base`, so `VA = file_offset + ro_base`. The
    /// entry PC is chosen by [`select_gsc_entry`] (a single self-contained slot
    /// like `fw.bin`, or the primary RW slot of a multi-slot A/B flash like
    /// `ti50.bin.prod`); `RAX_GSC_ENTRY=<hex>` overrides it. The RSA signature
    /// and public key in the header are verification-only and ignored here.
    fn load_gsc(mem: &GuestMemoryMmap, buf: &[u8]) -> Result<RiscVBootInfo> {
        if rd_u32(buf, 0) != Some(GSC_HEADER_MAGIC) {
            // Not a SignedHeader image; treat it as a raw blob at 0.
            return Self::load_raw(mem, buf);
        }
        let load_base = rd_u32(buf, HDR_RO_BASE)
            .map(|v| v as u64)
            .filter(|&v| v != 0)
            .unwrap_or(0);
        mem.write_slice(buf, GuestAddress(load_base))?;

        let entry = env_hex("RAX_GSC_ENTRY").unwrap_or_else(|| select_gsc_entry(buf, load_base));

        Ok(RiscVBootInfo {
            entry_point: entry,
            load_addr: load_base,
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

impl Arch for Riscv64Arch {
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
            return Ok(BootInfo::RiscV(Self::load_gsc(mem, &buf)?));
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

#[cfg(test)]
mod gsc_loader_tests {
    use super::*;

    fn put(buf: &mut [u8], off: usize, v: u32) {
        buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn rd_u32_reads_le_and_bounds_checks() {
        let buf = [0x11u8, 0x22, 0x33, 0x44, 0x55];
        assert_eq!(rd_u32(&buf, 0), Some(0x4433_2211));
        assert_eq!(rd_u32(&buf, 1), Some(0x5544_3322));
        assert_eq!(rd_u32(&buf, 2), None); // would read past the end
    }

    #[test]
    fn select_entry_single_slot_uses_header0() {
        // A self-contained slot (like fw.bin): header-0 image_size == file size.
        let mut buf = vec![0u8; 0x4_0000];
        put(&mut buf, 0, GSC_HEADER_MAGIC);
        put(&mut buf, HDR_RO_BASE, 0xa_0000);
        let total = buf.len() as u32;
        put(&mut buf, HDR_IMAGE_SIZE, total);
        put(&mut buf, HDR_ENTRY, 0xa_043c);
        assert_eq!(select_gsc_entry(&buf, 0xa_0000), 0xa_043c);
    }

    #[test]
    fn select_entry_multi_slot_picks_first_rw() {
        // A/B full flash (like ti50.bin.prod): RO_A (small) at 0, RW_A (large)
        // at 0x15000. The primary RW entry must be chosen.
        let mut buf = vec![0u8; 0x10_0000];
        // RO_A header (small image — not an RW slot)
        put(&mut buf, 0, GSC_HEADER_MAGIC);
        put(&mut buf, HDR_RO_BASE, 0x8_0000);
        put(&mut buf, HDR_IMAGE_SIZE, 0x1_4000);
        put(&mut buf, HDR_ENTRY, 0x9_0278);
        // RW_A header at 0x15000 (large image — the firmware slot)
        put(&mut buf, 0x1_5000, GSC_HEADER_MAGIC);
        put(&mut buf, 0x1_5000 + HDR_IMAGE_SIZE, 0x5_a000);
        put(&mut buf, 0x1_5000 + HDR_ENTRY, 0x9_56b2);
        assert_eq!(select_gsc_entry(&buf, 0x8_0000), 0x9_56b2);
    }

    #[test]
    fn load_gsc_maps_at_ro_base_and_sets_entry() {
        let mut buf = vec![0u8; 0x4_0000];
        put(&mut buf, 0, GSC_HEADER_MAGIC);
        put(&mut buf, HDR_RO_BASE, 0xa_0000);
        let total = buf.len() as u32;
        put(&mut buf, HDR_IMAGE_SIZE, total);
        put(&mut buf, HDR_ENTRY, 0xa_043c);
        // a recognizable marker just after the header
        put(&mut buf, 0x400, 0xdead_beef);

        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap();
        let info = Riscv64Arch::load_gsc(&mem, &buf).unwrap();
        assert_eq!(info.load_addr, 0xa_0000);
        assert_eq!(info.entry_point, 0xa_043c);
        assert_eq!(info.image_size, buf.len() as u64);
        // The image is mapped verbatim at ro_base (VA = file_off + ro_base).
        let mut word = [0u8; 4];
        mem.read_slice(&mut word, GuestAddress(0xa_0000 + 0x400))
            .unwrap();
        assert_eq!(u32::from_le_bytes(word), 0xdead_beef);
    }

    #[test]
    fn load_gsc_falls_back_to_raw_without_magic() {
        let buf = vec![0x13u8, 0x05, 0x00, 0x00, 0xaa, 0xbb]; // no SignedHeader
        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x1_0000usize)]).unwrap();
        let info = Riscv64Arch::load_gsc(&mem, &buf).unwrap();
        assert_eq!(info.load_addr, 0);
        assert_eq!(info.entry_point, 0);
    }
}
