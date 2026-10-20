//! Private-copy commitment records write access, including a successful probe
//! that precedes a later syscall error. It is independent of the stored bytes.
use std::sync::atomic::Ordering;

use super::pagetable::{PTE_PRIVATE, PTE_TRACK_PRIVATE, PTE_VALID};
use super::{AddressSpace, MmError, PAGE_SIZE, check_range, first_hole};

impl AddressSpace {
    /// Enable logical copy charging after initial backing bytes are installed.
    /// Mapping changes require stopped guest CPUs, as for map/protect/unmap.
    pub(crate) fn track_private_copies(&self, start: u64, len: u64) -> Result<(), MmError> {
        let end = check_range(start, len, self.inner.va_limit)?;
        let mut vmas = self.vmas();
        if let Some(hole) = first_hole(&vmas, start, end) {
            return Err(MmError::NotMapped { addr: hole });
        }
        vmas.track_private_copy(start, end);
        self.inner
            .table
            .for_each_populated(start >> 12, end >> 12, |_, slot| {
                let current = slot.load(Ordering::Acquire);
                if current & PTE_PRIVATE == 0 {
                    slot.fetch_or(PTE_TRACK_PRIVATE, Ordering::AcqRel);
                }
            });
        Ok(())
    }

    /// Copy-capable bytes in a mapped half-open page range, without visiting holes.
    pub(crate) fn private_copy_capacity(&self, start: u64, len: u64) -> u64 {
        let end = start.checked_add(len).expect("validated copy query range");
        self.vmas().private_copy_bytes(start, end)
    }

    /// Acquired page charge, including writes that stored an identical value.
    /// Absent page-table subtrees are skipped; no guest frame is populated.
    pub(crate) fn acquired_private_bytes(&self, start: u64, len: u64) -> u64 {
        let end = start.checked_add(len).expect("validated copy query range");
        let mut bytes = 0;
        self.inner
            .table
            .for_each_populated(start >> 12, end >> 12, |_, slot| {
                if slot.load(Ordering::Acquire) & PTE_PRIVATE != 0 {
                    bytes += PAGE_SIZE;
                }
            });
        bytes
    }

    #[inline]
    pub(super) fn acquire_private_copy(&self, address: u64) {
        let table = &self.inner.table;
        if table.get(address >> 12) & PTE_TRACK_PRIVATE == 0 {
            return;
        }
        // Frame/permissions cannot change during execution. Concurrent writes
        // may acquire the same charge once; preserve all unrelated entry bits.
        let _ = table.slot(address >> 12).fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |current| {
                (current & (PTE_VALID | PTE_TRACK_PRIVATE) == (PTE_VALID | PTE_TRACK_PRIVATE))
                    .then_some((current | PTE_PRIVATE) & !PTE_TRACK_PRIVATE)
            },
        );
    }
}

#[cfg(test)]
mod tests;
