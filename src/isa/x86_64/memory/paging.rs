//! Four-level translation and access checks (Intel SDM Vol. 3A, chapter 5).

use super::{AccessType, Mmu, PTE_RSVD_MASK, SystemRegisters, TlbEntry, cr0, cr4, efer, flags};
use crate::error::{Error, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use vm_memory::bitmap::Bitmap;
use vm_memory::{GuestAddress, GuestMemory, VolatileMemory};

const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

impl Mmu {
    fn paging_fault_bits(access: AccessType, sregs: &SystemRegisters) -> u64 {
        let user = sregs.cs.selector & 3 == 3;
        let instruction = matches!(access, AccessType::Execute)
            && (sregs.cr4 & cr4::SMEP != 0
                || (sregs.cr4 & cr4::PAE != 0 && sregs.efer & efer::NXE != 0));
        ((matches!(access, AccessType::Write) as u64) << 1)
            | ((user as u64) << 2)
            | ((instruction as u64) << 4)
    }

    pub(super) fn check_paging_access(
        &self,
        vaddr: u64,
        access: AccessType,
        sregs: &SystemRegisters,
        mapping: &TlbEntry,
    ) -> Result<()> {
        let bits = Self::paging_fault_bits(access, sregs);
        if mapping.no_execute && sregs.efer & efer::NXE == 0 {
            return Err(Error::PageFault {
                vaddr,
                error_code: bits | 9,
            });
        }
        let user = sregs.cs.selector & 3 == 3;
        let protection = self.protection_error_flags(access, sregs, mapping);
        let denied = protection != 0
            || (user && !mapping.user)
            || (matches!(access, AccessType::Write)
                && !mapping.writable
                && (user || sregs.cr0 & cr0::WP != 0))
            || (matches!(access, AccessType::Execute) && mapping.no_execute);
        if denied {
            return Err(Error::PageFault {
                vaddr,
                error_code: bits | 1 | protection,
            });
        }
        Ok(())
    }

    /// Set paging status bits atomically without losing concurrent guest bits.
    fn set_paging_status(&self, address: u64, flags: u64) -> Result<()> {
        let slice = self
            .memory
            .get_slice(GuestAddress(address), 8)
            .map_err(|error| {
                Error::Emulator(format!("page-table status at {address:#x}: {error}"))
            })?;
        let entry = slice.get_atomic_ref::<AtomicU64>(0).map_err(|error| {
            Error::Emulator(format!("page-table status at {address:#x}: {error}"))
        })?;
        entry.fetch_or(flags.to_le(), Ordering::AcqRel);
        slice.bitmap().mark_dirty(0, 8);
        Ok(())
    }

    #[cold]
    pub(super) fn translate_slow(
        &mut self,
        vaddr: u64,
        access: AccessType,
        sregs: &SystemRegisters,
        tlb_index: usize,
        tlb_tag: u64,
    ) -> Result<u64> {
        if !self.long_mode(sregs) {
            return Err(Error::Emulator(
                "only 64-bit long mode paging is supported".to_string(),
            ));
        }
        let fault_bits = Self::paging_fault_bits(access, sregs);
        let mut table = sregs.cr3 & ADDRESS_MASK;
        let mut writable = true;
        let mut user = true;
        let mut no_execute = false;
        for (level, shift) in [39, 30, 21, 12].into_iter().enumerate() {
            let address = table + ((vaddr >> shift) & 511) * 8;
            let entry = self.read_pte(address)?;
            if entry & flags::PRESENT == 0 {
                return Err(Error::PageFault {
                    vaddr,
                    error_code: fault_bits,
                });
            }
            let leaf = level == 3 || (level != 0 && entry & flags::HUGE_PAGE != 0);
            let mut reserved = PTE_RSVD_MASK;
            if sregs.efer & efer::NXE == 0 {
                reserved |= flags::NO_EXECUTE;
            }
            if level == 0 {
                reserved |= flags::HUGE_PAGE;
            }
            if leaf && shift > 12 {
                // A large-page PAT bit is bit 12; lower address bits above it
                // must be zero. At level 3 bit 7 is PAT, not a PS selector.
                reserved |= ((1u64 << shift) - 1) & ADDRESS_MASK & !(1 << 12);
            }
            if entry & reserved != 0 {
                return Err(Error::PageFault {
                    vaddr,
                    error_code: fault_bits | 9,
                });
            }
            writable &= entry & flags::WRITABLE != 0;
            user &= entry & flags::USER != 0;
            no_execute |= entry & flags::NO_EXECUTE != 0;
            if entry & flags::ACCESSED == 0 {
                self.set_paging_status(address, flags::ACCESSED)?;
            }
            if leaf {
                let offset_mask = (1u64 << shift) - 1;
                let mapping = TlbEntry {
                    tag: tlb_tag,
                    phys_base: entry & ADDRESS_MASK & !offset_mask,
                    page_shift: shift as u8,
                    valid: true,
                    writable,
                    user,
                    no_execute,
                    protection_key: ((entry >> 59) & 15) as u8,
                };
                self.check_paging_access(vaddr, access, sregs, &mapping)?;
                if matches!(access, AccessType::Write) && entry & flags::DIRTY == 0 {
                    self.set_paging_status(address, flags::DIRTY)?;
                }
                self.tlb[tlb_index] = mapping;
                return Ok(mapping.phys_base | (vaddr & offset_mask));
            }
            table = entry & ADDRESS_MASK;
        }
        Err(Error::Emulator(
            "four-level walk ended without a leaf".to_string(),
        ))
    }

    fn read_pte(&self, paddr: u64) -> Result<u64> {
        let slice = self
            .memory
            .get_slice(GuestAddress(paddr), 8)
            .map_err(|error| {
                Error::Emulator(format!("failed to read PTE at {paddr:#x}: {error}"))
            })?;
        let entry = slice.get_atomic_ref::<AtomicU64>(0).map_err(|error| {
            Error::Emulator(format!("failed to read PTE at {paddr:#x}: {error}"))
        })?;
        Ok(u64::from_le(entry.load(Ordering::Acquire)))
    }
}
