//! Non-PAE 32-bit paging, Intel SDM086 Vol.3A5.3, tables5-3 through5-6.
//! The deterministic CPUID profile advertises neither PAT nor PSE-36.
use super::{AccessType, Mmu, SystemRegisters, TlbEntry, flags};
use crate::error::{Error, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use vm_memory::bitmap::Bitmap;
use vm_memory::{GuestAddress, GuestMemory, VolatileMemory};
impl Mmu {
    fn read_legacy_entry(&self, address: u64) -> Result<u32> {
        let slice = self
            .memory
            .get_slice(GuestAddress(address), 4)
            .map_err(|e| {
                Error::Emulator(format!("32-bit page-table entry at {address:#x}: {e}"))
            })?;
        let entry = slice.get_atomic_ref::<AtomicU32>(0).map_err(|e| {
            Error::Emulator(format!("32-bit page-table entry at {address:#x}: {e}"))
        })?;
        Ok(u32::from_le(entry.load(Ordering::Acquire)))
    }
    fn set_legacy_status(&self, address: u64, status: u32) -> Result<()> {
        let slice = self
            .memory
            .get_slice(GuestAddress(address), 4)
            .map_err(|e| {
                Error::Emulator(format!("32-bit page-table status at {address:#x}: {e}"))
            })?;
        let entry = slice.get_atomic_ref::<AtomicU32>(0).map_err(|e| {
            Error::Emulator(format!("32-bit page-table status at {address:#x}: {e}"))
        })?;
        entry.fetch_or(status.to_le(), Ordering::AcqRel);
        slice.bitmap().mark_dirty(0, 4);
        Ok(())
    }
    pub(super) fn translate_legacy32(
        &mut self,
        vaddr: u64,
        access: AccessType,
        sregs: &SystemRegisters,
        tlb_index: usize,
        tlb_tag: u64,
    ) -> Result<u64> {
        let fault_bits = Self::paging_fault_bits(access, sregs);
        let mut table = sregs.cr3 & 0xfffff000;
        let mut writable = true;
        let mut user = true;
        let pse = sregs.cr4 & (1 << 4) != 0;
        for (directory, shift) in [(true, 22), (false, 12)] {
            let address = table + ((vaddr >> shift) & 1023) * 4;
            let entry = u64::from(self.read_legacy_entry(address)?);
            if entry & flags::PRESENT == 0 {
                return Err(Error::PageFault {
                    vaddr,
                    error_code: fault_bits,
                });
            }
            let large = directory && pse && entry & flags::HUGE_PAGE != 0;
            // Without PSE-36, bits21:13 of a large PDE are reserved.
            // Without PAT, large-PDE bit12 and PTE bit7 are reserved only
            // when CR4.PSE=1. With PSE=0 the PDE's PS bit is ignored.
            if (large && entry & 0x003ff000 != 0) || (!directory && pse && entry & (1 << 7) != 0) {
                return Err(Error::PageFault {
                    vaddr,
                    error_code: fault_bits | 9,
                });
            }
            writable &= entry & flags::WRITABLE != 0;
            user &= entry & flags::USER != 0;
            if entry & flags::ACCESSED == 0 {
                self.set_legacy_status(address, flags::ACCESSED as u32)?;
            }
            if large || !directory {
                let page_shift = if large { 22 } else { 12 };
                let offset = (1u64 << page_shift) - 1;
                let mapping = TlbEntry {
                    tag: tlb_tag,
                    phys_base: entry & 0xfffff000 & !offset,
                    page_shift,
                    valid: true,
                    writable,
                    user,
                    no_execute: false,
                    protection_key: 0,
                };
                self.check_paging_access(vaddr, access, sregs, &mapping)?;
                if matches!(access, AccessType::Write) && entry & flags::DIRTY == 0 {
                    self.set_legacy_status(address, flags::DIRTY as u32)?;
                }
                self.tlb[tlb_index] = mapping;
                return Ok(mapping.phys_base | (vaddr & offset));
            }
            table = entry & 0xfffff000;
        }
        unreachable!("32-bit walk always terminates at its PTE")
    }
}
