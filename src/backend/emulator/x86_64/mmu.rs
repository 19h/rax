//! Memory Management Unit - page table translation.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::cpu::SystemRegisters;
use crate::error::{Error, Result};

/// Page table entry flags.
#[allow(dead_code)]
mod flags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITABLE: u64 = 1 << 1;
    pub const USER: u64 = 1 << 2;
    pub const WRITE_THROUGH: u64 = 1 << 3;
    pub const CACHE_DISABLE: u64 = 1 << 4;
    pub const ACCESSED: u64 = 1 << 5;
    pub const DIRTY: u64 = 1 << 6;
    pub const HUGE_PAGE: u64 = 1 << 7;
    pub const GLOBAL: u64 = 1 << 8;
    pub const NO_EXECUTE: u64 = 1 << 63;
}

/// Control register bits.
mod cr0 {
    pub const PE: u64 = 1 << 0;  // Protected Mode Enable
    pub const PG: u64 = 1 << 31; // Paging Enable
}

#[allow(dead_code)]
mod cr4 {
    pub const PAE: u64 = 1 << 5; // Physical Address Extension
}

mod efer {
    pub const LME: u64 = 1 << 8;  // Long Mode Enable
    pub const LMA: u64 = 1 << 10; // Long Mode Active
}

/// Memory access type.
#[derive(Debug, Clone, Copy)]
pub enum AccessType {
    Read,
    Write,
    Execute,
}

/// Memory Management Unit for address translation.
pub struct Mmu {
    memory: Arc<GuestMemoryMmap>,
}

impl Mmu {
    pub fn new(memory: Arc<GuestMemoryMmap>) -> Self {
        Mmu { memory }
    }

    /// Check if paging is enabled.
    fn paging_enabled(&self, sregs: &SystemRegisters) -> bool {
        (sregs.cr0 & cr0::PG) != 0 && (sregs.cr0 & cr0::PE) != 0
    }

    /// Check if we're in 64-bit long mode.
    fn long_mode(&self, sregs: &SystemRegisters) -> bool {
        (sregs.efer & efer::LMA) != 0 && (sregs.efer & efer::LME) != 0
    }

    /// Translate a virtual address to a physical address.
    pub fn translate(
        &self,
        vaddr: u64,
        _access: AccessType,
        sregs: &SystemRegisters,
    ) -> Result<u64> {
        // If paging is disabled, virtual = physical
        if !self.paging_enabled(sregs) {
            return Ok(vaddr);
        }

        // For now, we only support 64-bit 4-level paging
        if !self.long_mode(sregs) {
            return Err(Error::Emulator(
                "only 64-bit long mode paging is supported".to_string(),
            ));
        }

        // 4-level paging: PML4 -> PDPT -> PD -> PT
        let pml4_base = sregs.cr3 & !0xFFF;
        let pml4_index = (vaddr >> 39) & 0x1FF;
        let pdpt_index = (vaddr >> 30) & 0x1FF;
        let pd_index = (vaddr >> 21) & 0x1FF;
        let pt_index = (vaddr >> 12) & 0x1FF;
        let offset = vaddr & 0xFFF;

        // Read PML4 entry
        let pml4e = self.read_pte(pml4_base + pml4_index * 8)?;
        if pml4e & flags::PRESENT == 0 {
            return Err(Error::Emulator(format!(
                "page fault: PML4E not present at vaddr {:#x}",
                vaddr
            )));
        }

        // Read PDPT entry
        let pdpt_base = pml4e & 0x000F_FFFF_FFFF_F000;
        let pdpte = self.read_pte(pdpt_base + pdpt_index * 8)?;
        if pdpte & flags::PRESENT == 0 {
            return Err(Error::Emulator(format!(
                "page fault: PDPTE not present at vaddr {:#x}",
                vaddr
            )));
        }

        // Check for 1GB huge page
        if pdpte & flags::HUGE_PAGE != 0 {
            let page_base = pdpte & 0x000F_FFFF_C000_0000;
            return Ok(page_base | (vaddr & 0x3FFF_FFFF));
        }

        // Read PD entry
        let pd_base = pdpte & 0x000F_FFFF_FFFF_F000;
        let pde = self.read_pte(pd_base + pd_index * 8)?;
        if pde & flags::PRESENT == 0 {
            return Err(Error::Emulator(format!(
                "page fault: PDE not present at vaddr {:#x}",
                vaddr
            )));
        }

        // Check for 2MB huge page
        if pde & flags::HUGE_PAGE != 0 {
            let page_base = pde & 0x000F_FFFF_FFE0_0000;
            return Ok(page_base | (vaddr & 0x1F_FFFF));
        }

        // Read PT entry
        let pt_base = pde & 0x000F_FFFF_FFFF_F000;
        let pte = self.read_pte(pt_base + pt_index * 8)?;
        if pte & flags::PRESENT == 0 {
            return Err(Error::Emulator(format!(
                "page fault: PTE not present at vaddr {:#x}",
                vaddr
            )));
        }

        let page_base = pte & 0x000F_FFFF_FFFF_F000;
        Ok(page_base | offset)
    }

    /// Read a page table entry from physical memory.
    fn read_pte(&self, paddr: u64) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.memory
            .read_slice(&mut buf, GuestAddress(paddr))
            .map_err(|e| Error::Emulator(format!("failed to read PTE at {:#x}: {}", paddr, e)))?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Read bytes from guest memory (physical address).
    pub fn read_phys(&self, paddr: u64, buf: &mut [u8]) -> Result<()> {
        self.memory
            .read_slice(buf, GuestAddress(paddr))
            .map_err(|e| Error::Emulator(format!("failed to read at {:#x}: {}", paddr, e)))
    }

    /// Write bytes to guest memory (physical address).
    pub fn write_phys(&self, paddr: u64, buf: &[u8]) -> Result<()> {
        self.memory
            .write_slice(buf, GuestAddress(paddr))
            .map_err(|e| Error::Emulator(format!("failed to write at {:#x}: {}", paddr, e)))
    }

    /// Read bytes from guest memory (virtual address).
    pub fn read(&self, vaddr: u64, buf: &mut [u8], sregs: &SystemRegisters) -> Result<()> {
        // Handle page boundary crossing
        let mut offset = 0;
        let mut remaining = buf.len();
        let mut addr = vaddr;

        while remaining > 0 {
            let paddr = self.translate(addr, AccessType::Read, sregs)?;
            let page_offset = (paddr & 0xFFF) as usize;
            let bytes_in_page = std::cmp::min(remaining, 0x1000 - page_offset);

            self.read_phys(paddr, &mut buf[offset..offset + bytes_in_page])?;

            offset += bytes_in_page;
            remaining -= bytes_in_page;
            addr += bytes_in_page as u64;
        }

        Ok(())
    }

    /// Write bytes to guest memory (virtual address).
    pub fn write(&self, vaddr: u64, buf: &[u8], sregs: &SystemRegisters) -> Result<()> {
        // Handle page boundary crossing
        let mut offset = 0;
        let mut remaining = buf.len();
        let mut addr = vaddr;

        while remaining > 0 {
            let paddr = self.translate(addr, AccessType::Write, sregs)?;
            let page_offset = (paddr & 0xFFF) as usize;
            let bytes_in_page = std::cmp::min(remaining, 0x1000 - page_offset);

            self.write_phys(paddr, &buf[offset..offset + bytes_in_page])?;

            offset += bytes_in_page;
            remaining -= bytes_in_page;
            addr += bytes_in_page as u64;
        }

        Ok(())
    }

    /// Read a u8 from virtual address.
    pub fn read_u8(&self, vaddr: u64, sregs: &SystemRegisters) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.read(vaddr, &mut buf, sregs)?;
        Ok(buf[0])
    }

    /// Read a u16 from virtual address.
    pub fn read_u16(&self, vaddr: u64, sregs: &SystemRegisters) -> Result<u16> {
        let mut buf = [0u8; 2];
        self.read(vaddr, &mut buf, sregs)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Read a u32 from virtual address.
    pub fn read_u32(&self, vaddr: u64, sregs: &SystemRegisters) -> Result<u32> {
        let mut buf = [0u8; 4];
        self.read(vaddr, &mut buf, sregs)?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Read a u64 from virtual address.
    pub fn read_u64(&self, vaddr: u64, sregs: &SystemRegisters) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.read(vaddr, &mut buf, sregs)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Write a u8 to virtual address.
    pub fn write_u8(&self, vaddr: u64, value: u8, sregs: &SystemRegisters) -> Result<()> {
        self.write(vaddr, &[value], sregs)
    }

    /// Write a u16 to virtual address.
    pub fn write_u16(&self, vaddr: u64, value: u16, sregs: &SystemRegisters) -> Result<()> {
        self.write(vaddr, &value.to_le_bytes(), sregs)
    }

    /// Write a u32 to virtual address.
    pub fn write_u32(&self, vaddr: u64, value: u32, sregs: &SystemRegisters) -> Result<()> {
        self.write(vaddr, &value.to_le_bytes(), sregs)
    }

    /// Write a u64 to virtual address.
    pub fn write_u64(&self, vaddr: u64, value: u64, sregs: &SystemRegisters) -> Result<()> {
        self.write(vaddr, &value.to_le_bytes(), sregs)
    }

    /// Invalidate TLB entry for a virtual address.
    /// This is a no-op since we don't cache translations.
    pub fn invlpg(&self, _vaddr: u64) {
        // No TLB cache to invalidate
    }
}
