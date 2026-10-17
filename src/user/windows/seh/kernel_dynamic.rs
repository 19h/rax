//! Process-owned ranges registered by native ProcessDynamicFunctionTableInformation.
//!
//! These registrations describe the kernel's range ledger, not the fixed
//! user-mode arrays admitted by `RtlAddFunctionTable`. In particular, native
//! startup registers a type3 table whose first entry is zero. No host kernel
//! call or host pointer participates in this ledger.

use crate::user::windows::nt::status::{
    STATUS_BAD_FUNCTION_TABLE, STATUS_CONFLICTING_ADDRESSES, STATUS_INSUFFICIENT_RESOURCES,
    STATUS_INTEGER_OVERFLOW, STATUS_NO_MEMORY, STATUS_SUCCESS,
};

/// Emulator admission bound, not a measured Windows limit.
pub(crate) const MAX_TABLES: usize = 65_536;

#[derive(Clone, Copy, Debug)]
struct Range {
    base: u64,
    bytes: u32,
}

/// Registrations retain insertion order, including duplicate empty ranges.
#[derive(Debug, Default)]
pub(crate) struct Tables {
    ranges: Vec<Range>,
}

impl Tables {
    /// Admission checks the nonwrapping extent before overlap and quota.
    /// Linear scans use O(N) time and the ledger occupies O(N) host space.
    pub(crate) fn register(&mut self, base: u64, maximum: u64) -> u32 {
        let Some(bytes) = maximum
            .checked_sub(base)
            .and_then(|extent| u32::try_from(extent).ok())
        else {
            return STATUS_INTEGER_OVERFLOW;
        };
        for range in &self.ranges {
            // Admission guarantees this addition cannot overflow. The native
            // ordering intentionally treats an empty existing range differently
            // from an empty candidate at the start of a nonempty range.
            let end = range.base + u64::from(range.bytes);
            let overlaps = if base < range.base {
                maximum > range.base
            } else {
                base < end
            };
            if overlaps {
                return STATUS_CONFLICTING_ADDRESSES;
            }
        }
        if self.ranges.len() >= MAX_TABLES {
            return STATUS_INSUFFICIENT_RESOURCES;
        }
        if self.ranges.try_reserve(1).is_err() {
            return STATUS_NO_MEMORY;
        }
        self.ranges.push(Range { base, bytes });
        STATUS_SUCCESS
    }

    /// Removal checks a wrapping extent and rejects UINT32_MAX itself, unlike
    /// admission. The first registration at the requested base owns matching:
    /// a later equal-base range cannot bypass an earlier different extent.
    pub(crate) fn remove(&mut self, base: u64, maximum: u64) -> u32 {
        let extent = maximum.wrapping_sub(base);
        let Ok(bytes) = u32::try_from(extent) else {
            return STATUS_INTEGER_OVERFLOW;
        };
        if bytes == u32::MAX {
            return STATUS_INTEGER_OVERFLOW;
        }
        let Some(index) = self.ranges.iter().position(|range| range.base == base) else {
            return STATUS_BAD_FUNCTION_TABLE;
        };
        if self.ranges[index].bytes != bytes {
            return STATUS_BAD_FUNCTION_TABLE;
        }
        self.ranges.remove(index);
        STATUS_SUCCESS
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.ranges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_dynamic_function_table_quota_preserves_state_and_recovers_after_delete() {
        let mut tables = Tables {
            ranges: vec![Range { base: 0, bytes: 0 }; MAX_TABLES],
        };
        assert_eq!(tables.register(1, 1), STATUS_INSUFFICIENT_RESOURCES);
        assert_eq!(tables.len(), MAX_TABLES);
        assert_eq!(tables.remove(0, 0), STATUS_SUCCESS);
        assert_eq!(tables.register(1, 1), STATUS_SUCCESS);
        assert_eq!(tables.len(), MAX_TABLES);
        assert_eq!(tables.remove(1, 1), STATUS_SUCCESS);
        assert_eq!(tables.len(), MAX_TABLES - 1);
    }
}
