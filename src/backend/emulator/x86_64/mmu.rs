//! Memory Management Unit - page table translation with TLB caching.

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

/// TLB entry - caches virtual to physical page translations.
#[derive(Clone, Copy)]
struct TlbEntry {
    /// Virtual page number (vaddr >> 12), with CR3 mixed in for context tagging
    tag: u64,
    /// Physical page base address (already shifted)
    phys_base: u64,
    /// Page size shift: 12 for 4KB, 21 for 2MB, 30 for 1GB
    page_shift: u8,
    /// Valid bit
    valid: bool,
}

impl Default for TlbEntry {
    #[inline(always)]
    fn default() -> Self {
        TlbEntry {
            tag: 0,
            phys_base: 0,
            page_shift: 12,
            valid: false,
        }
    }
}

/// TLB size - must be power of 2 for fast indexing
const TLB_SIZE: usize = 256;
const TLB_MASK: usize = TLB_SIZE - 1;

/// Code page bitmap size - tracks which pages have been executed from.
/// Each bit represents a 4KB page. 64KB of bitmap = 512MB of tracked memory.
/// For larger address spaces, we use a hash of the virtual address.
const CODE_PAGE_BITMAP_SIZE: usize = 64 * 1024; // 64KB = 512K pages = 2GB coverage
const CODE_PAGE_BITMAP_MASK: usize = CODE_PAGE_BITMAP_SIZE * 8 - 1;

/// Memory Management Unit for address translation with TLB.
pub struct Mmu {
    memory: Arc<GuestMemoryMmap>,
    /// Direct-mapped TLB for 4KB pages
    tlb: [TlbEntry; TLB_SIZE],
    /// Cached CR3 for detecting context switches
    cached_cr3: u64,
    /// Bitmap tracking pages that have been executed from (code pages).
    /// Used for self-modifying code detection: writes to code pages
    /// require decode cache invalidation.
    code_page_bitmap: Box<[u8; CODE_PAGE_BITMAP_SIZE]>,
}

impl Mmu {
    pub fn new(memory: Arc<GuestMemoryMmap>) -> Self {
        Mmu {
            memory,
            tlb: [TlbEntry::default(); TLB_SIZE],
            cached_cr3: 0,
            code_page_bitmap: Box::new([0u8; CODE_PAGE_BITMAP_SIZE]),
        }
    }

    // =========================================================================
    // Self-modifying code detection
    // =========================================================================

    /// Compute the bitmap index for a virtual page.
    #[inline(always)]
    fn code_page_index(vaddr: u64) -> (usize, u8) {
        // Use page number (vaddr >> 12) and hash to fit in bitmap
        let page_num = (vaddr >> 12) as usize;
        let bit_index = page_num & CODE_PAGE_BITMAP_MASK;
        let byte_index = bit_index >> 3;
        let bit_offset = (bit_index & 7) as u8;
        (byte_index, 1u8 << bit_offset)
    }

    /// Mark a page as containing executed code.
    /// Called when fetching instructions from a page.
    #[inline(always)]
    pub fn mark_code_page(&mut self, vaddr: u64) {
        let (byte_idx, bit_mask) = Self::code_page_index(vaddr);
        self.code_page_bitmap[byte_idx] |= bit_mask;
    }

    /// Check if a page has been marked as code.
    /// Called when writing to memory to detect self-modifying code.
    #[inline(always)]
    pub fn is_code_page(&self, vaddr: u64) -> bool {
        let (byte_idx, bit_mask) = Self::code_page_index(vaddr);
        (self.code_page_bitmap[byte_idx] & bit_mask) != 0
    }

    /// Clear the code page bitmap.
    /// Should be called on context switch or when JIT cache is fully cleared.
    #[inline]
    pub fn clear_code_pages(&mut self) {
        self.code_page_bitmap.fill(0);
    }

    /// Check if paging is enabled.
    #[inline(always)]
    fn paging_enabled(&self, sregs: &SystemRegisters) -> bool {
        (sregs.cr0 & cr0::PG) != 0 && (sregs.cr0 & cr0::PE) != 0
    }

    /// Check if we're in 64-bit long mode.
    #[inline(always)]
    fn long_mode(&self, sregs: &SystemRegisters) -> bool {
        (sregs.efer & efer::LMA) != 0 && (sregs.efer & efer::LME) != 0
    }

    /// Compute TLB index from virtual address (uses bits that vary most)
    #[inline(always)]
    fn tlb_index(vaddr: u64) -> usize {
        // Use bits 12-19 (page number bits) for index
        ((vaddr >> 12) as usize) & TLB_MASK
    }

    /// Compute TLB tag from virtual address and CR3
    #[inline(always)]
    fn tlb_tag(vaddr: u64, cr3: u64) -> u64 {
        // Tag includes page number and CR3 to handle context switches
        // For 4KB pages: tag = VPN (bits 12-47) | (CR3 bits 12-35 shifted)
        let vpn = vaddr >> 12;
        let cr3_tag = (cr3 >> 12) & 0xFFFFFF; // 24 bits of CR3
        vpn ^ (cr3_tag << 36) // Mix CR3 into upper bits
    }

    /// Flush entire TLB (called on CR3 change)
    #[inline]
    pub fn flush_tlb(&mut self) {
        for entry in &mut self.tlb {
            entry.valid = false;
        }
    }

    /// Invalidate TLB entry for a virtual address.
    #[inline]
    pub fn invlpg(&mut self, vaddr: u64) {
        let index = Self::tlb_index(vaddr);
        self.tlb[index].valid = false;
    }

    /// Translate a virtual address to a physical address.
    #[inline]
    pub fn translate(
        &mut self,
        vaddr: u64,
        access: AccessType,
        sregs: &SystemRegisters,
    ) -> Result<u64> {
        // If paging is disabled, virtual = physical
        if !self.paging_enabled(sregs) {
            return Ok(vaddr);
        }

        // Check for CR3 change (context switch) - flush TLB
        if sregs.cr3 != self.cached_cr3 {
            self.flush_tlb();
            self.cached_cr3 = sregs.cr3;
        }

        // TLB lookup - only use for reads (writes need permission check)
        // TODO: Could cache write permission in TLB for better performance
        if !matches!(access, AccessType::Write) {
            let index = Self::tlb_index(vaddr);
            let tag = Self::tlb_tag(vaddr, sregs.cr3);

            let entry = &self.tlb[index];
            if entry.valid && entry.tag == tag {
                // TLB hit! Fast path
                let offset_mask = (1u64 << entry.page_shift) - 1;
                let paddr = entry.phys_base | (vaddr & offset_mask);
                return Ok(paddr);
            }
        }

        // TLB miss or write - do full page table walk with permission check
        let index = Self::tlb_index(vaddr);
        let tag = Self::tlb_tag(vaddr, sregs.cr3);
        self.translate_slow(vaddr, access, sregs, index, tag)
    }

    /// Slow path: full page table walk (called on TLB miss)
    #[cold]
    fn translate_slow(
        &mut self,
        vaddr: u64,
        access: AccessType,
        sregs: &SystemRegisters,
        tlb_index: usize,
        tlb_tag: u64,
    ) -> Result<u64> {
        // For now, we only support 64-bit 4-level paging
        if !self.long_mode(sregs) {
            return Err(Error::Emulator(
                "only 64-bit long mode paging is supported".to_string(),
            ));
        }

        let is_write = matches!(access, AccessType::Write);

        // 4-level paging: PML4 -> PDPT -> PD -> PT
        let pml4_base = sregs.cr3 & !0xFFF;
        let pml4_index = (vaddr >> 39) & 0x1FF;
        let pdpt_index = (vaddr >> 30) & 0x1FF;
        let pd_index = (vaddr >> 21) & 0x1FF;
        let pt_index = (vaddr >> 12) & 0x1FF;

        // Read PML4 entry
        let pml4e = self.read_pte(pml4_base + pml4_index * 8)?;
        if pml4e & flags::PRESENT == 0 {
            return Err(Error::PageFault { vaddr, error_code: 0 });
        }
        // Check write permission at PML4 level
        if is_write && pml4e & flags::WRITABLE == 0 {
            // error_code bit 1 = write access, bit 0 = present
            return Err(Error::PageFault { vaddr, error_code: 0x3 });
        }

        // Read PDPT entry
        let pdpt_base = pml4e & 0x000F_FFFF_FFFF_F000;
        let pdpte = self.read_pte(pdpt_base + pdpt_index * 8)?;
        if pdpte & flags::PRESENT == 0 {
            return Err(Error::PageFault { vaddr, error_code: 0 });
        }
        // Check write permission at PDPT level
        if is_write && pdpte & flags::WRITABLE == 0 {
            return Err(Error::PageFault { vaddr, error_code: 0x3 });
        }

        // Check for 1GB huge page
        if pdpte & flags::HUGE_PAGE != 0 {
            let page_base = pdpte & 0x000F_FFFF_C000_0000;
            // Cache in TLB
            self.tlb[tlb_index] = TlbEntry {
                tag: tlb_tag,
                phys_base: page_base,
                page_shift: 30, // 1GB
                valid: true,
            };
            return Ok(page_base | (vaddr & 0x3FFF_FFFF));
        }

        // Read PD entry
        let pd_base = pdpte & 0x000F_FFFF_FFFF_F000;
        let pde = self.read_pte(pd_base + pd_index * 8)?;
        if pde & flags::PRESENT == 0 {
            return Err(Error::PageFault { vaddr, error_code: 0 });
        }
        // Check write permission at PD level
        if is_write && pde & flags::WRITABLE == 0 {
            return Err(Error::PageFault { vaddr, error_code: 0x3 });
        }

        // Check for 2MB huge page
        if pde & flags::HUGE_PAGE != 0 {
            let page_base = pde & 0x000F_FFFF_FFE0_0000;
            // Cache in TLB
            self.tlb[tlb_index] = TlbEntry {
                tag: tlb_tag,
                phys_base: page_base,
                page_shift: 21, // 2MB
                valid: true,
            };
            return Ok(page_base | (vaddr & 0x1F_FFFF));
        }

        // Read PT entry
        let pt_base = pde & 0x000F_FFFF_FFFF_F000;
        let pte = self.read_pte(pt_base + pt_index * 8)?;
        if pte & flags::PRESENT == 0 {
            return Err(Error::PageFault { vaddr, error_code: 0 });
        }
        // Check write permission at PT level
        if is_write && pte & flags::WRITABLE == 0 {
            return Err(Error::PageFault { vaddr, error_code: 0x3 });
        }

        let page_base = pte & 0x000F_FFFF_FFFF_F000;
        let offset = vaddr & 0xFFF;

        // Cache in TLB
        self.tlb[tlb_index] = TlbEntry {
            tag: tlb_tag,
            phys_base: page_base,
            page_shift: 12, // 4KB
            valid: true,
        };

        Ok(page_base | offset)
    }

    /// Read a page table entry from physical memory.
    #[inline(always)]
    fn read_pte(&self, paddr: u64) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.memory
            .read_slice(&mut buf, GuestAddress(paddr))
            .map_err(|e| Error::Emulator(format!("failed to read PTE at {:#x}: {}", paddr, e)))?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Read bytes from guest memory (physical address).
    #[inline(always)]
    pub fn read_phys(&self, paddr: u64, buf: &mut [u8]) -> Result<()> {
        self.memory
            .read_slice(buf, GuestAddress(paddr))
            .map_err(|e| Error::Emulator(format!("failed to read at {:#x}: {}", paddr, e)))
    }

    /// Write bytes to guest memory (physical address).
    #[inline(always)]
    pub fn write_phys(&self, paddr: u64, buf: &[u8]) -> Result<()> {
        // Debug: DISABLED - was watching PML4 entries
        self.memory
            .write_slice(buf, GuestAddress(paddr))
            .map_err(|e| Error::Emulator(format!("failed to write at {:#x}: {}", paddr, e)))
    }

    /// Read bytes from guest memory (virtual address).
    /// Fast path for single-page access, handles page crossing.
    #[inline]
    pub fn read(&mut self, vaddr: u64, buf: &mut [u8], sregs: &SystemRegisters) -> Result<()> {
        let len = buf.len();

        // Fast path: access doesn't cross page boundary
        let page_offset = (vaddr & 0xFFF) as usize;
        if page_offset + len <= 0x1000 {
            let paddr = self.translate(vaddr, AccessType::Read, sregs)?;
            return self.read_phys(paddr, buf);
        }

        // Slow path: handle page boundary crossing
        self.read_crossing(vaddr, buf, sregs)
    }

    /// Slow path for reads that cross page boundaries
    #[cold]
    fn read_crossing(&mut self, vaddr: u64, buf: &mut [u8], sregs: &SystemRegisters) -> Result<()> {
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
    /// Fast path for single-page access, handles page crossing.
    #[inline]
    pub fn write(&mut self, vaddr: u64, buf: &[u8], sregs: &SystemRegisters) -> Result<()> {
        let len = buf.len();

        // Fast path: access doesn't cross page boundary
        let page_offset = (vaddr & 0xFFF) as usize;
        if page_offset + len <= 0x1000 {
            let paddr = self.translate(vaddr, AccessType::Write, sregs)?;
            return self.write_phys(paddr, buf);
        }

        // Slow path: handle page boundary crossing
        self.write_crossing(vaddr, buf, sregs)
    }

    /// Slow path for writes that cross page boundaries
    #[cold]
    fn write_crossing(&mut self, vaddr: u64, buf: &[u8], sregs: &SystemRegisters) -> Result<()> {
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
    #[inline(always)]
    pub fn read_u8(&mut self, vaddr: u64, sregs: &SystemRegisters) -> Result<u8> {
        let paddr = self.translate(vaddr, AccessType::Read, sregs)?;
        let mut buf = [0u8; 1];
        self.read_phys(paddr, &mut buf)?;
        Ok(buf[0])
    }

    /// Read a u16 from virtual address.
    #[inline(always)]
    pub fn read_u16(&mut self, vaddr: u64, sregs: &SystemRegisters) -> Result<u16> {
        // Fast path if not crossing page boundary
        if (vaddr & 0xFFF) <= 0xFFE {
            let paddr = self.translate(vaddr, AccessType::Read, sregs)?;
            let mut buf = [0u8; 2];
            self.read_phys(paddr, &mut buf)?;
            Ok(u16::from_le_bytes(buf))
        } else {
            let mut buf = [0u8; 2];
            self.read(vaddr, &mut buf, sregs)?;
            Ok(u16::from_le_bytes(buf))
        }
    }

    /// Read a u32 from virtual address.
    #[inline(always)]
    pub fn read_u32(&mut self, vaddr: u64, sregs: &SystemRegisters) -> Result<u32> {
        // Fast path if not crossing page boundary
        if (vaddr & 0xFFF) <= 0xFFC {
            let paddr = self.translate(vaddr, AccessType::Read, sregs)?;
            let mut buf = [0u8; 4];
            self.read_phys(paddr, &mut buf)?;
            Ok(u32::from_le_bytes(buf))
        } else {
            let mut buf = [0u8; 4];
            self.read(vaddr, &mut buf, sregs)?;
            Ok(u32::from_le_bytes(buf))
        }
    }

    /// Read a u64 from virtual address.
    #[inline(always)]
    pub fn read_u64(&mut self, vaddr: u64, sregs: &SystemRegisters) -> Result<u64> {
        // Fast path if not crossing page boundary
        if (vaddr & 0xFFF) <= 0xFF8 {
            let paddr = self.translate(vaddr, AccessType::Read, sregs)?;
            let mut buf = [0u8; 8];
            self.read_phys(paddr, &mut buf)?;
            Ok(u64::from_le_bytes(buf))
        } else {
            let mut buf = [0u8; 8];
            self.read(vaddr, &mut buf, sregs)?;
            Ok(u64::from_le_bytes(buf))
        }
    }

    /// Write a u8 to virtual address.
    #[inline(always)]
    pub fn write_u8(&mut self, vaddr: u64, value: u8, sregs: &SystemRegisters) -> Result<()> {
        let paddr = self.translate(vaddr, AccessType::Write, sregs)?;
        self.write_phys(paddr, &[value])
    }

    /// Write a u16 to virtual address.
    #[inline(always)]
    pub fn write_u16(&mut self, vaddr: u64, value: u16, sregs: &SystemRegisters) -> Result<()> {
        if (vaddr & 0xFFF) <= 0xFFE {
            let paddr = self.translate(vaddr, AccessType::Write, sregs)?;
            self.write_phys(paddr, &value.to_le_bytes())
        } else {
            self.write(vaddr, &value.to_le_bytes(), sregs)
        }
    }

    /// Write a u32 to virtual address.
    #[inline(always)]
    pub fn write_u32(&mut self, vaddr: u64, value: u32, sregs: &SystemRegisters) -> Result<()> {
        if (vaddr & 0xFFF) <= 0xFFC {
            let paddr = self.translate(vaddr, AccessType::Write, sregs)?;
            self.write_phys(paddr, &value.to_le_bytes())
        } else {
            self.write(vaddr, &value.to_le_bytes(), sregs)
        }
    }

    /// Write a u64 to virtual address.
    #[inline(always)]
    pub fn write_u64(&mut self, vaddr: u64, value: u64, sregs: &SystemRegisters) -> Result<()> {
        if (vaddr & 0xFFF) <= 0xFF8 {
            let paddr = self.translate(vaddr, AccessType::Write, sregs)?;
            self.write_phys(paddr, &value.to_le_bytes())
        } else {
            self.write(vaddr, &value.to_le_bytes(), sregs)
        }
    }

}
