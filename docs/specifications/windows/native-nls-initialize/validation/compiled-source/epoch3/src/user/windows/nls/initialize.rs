//! Immutable locale.nls initialization bytes, separate from numbered tables.
use super::{MAX_SECTION_BYTES, MAX_TOTAL_BYTES, Nls, invalid};
use std::io;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct Initialize {
    pub(crate) locale: u32,
    pub(crate) bytes: Arc<[u8]>,
}

impl Nls {
    /// Selection-time immutable data. O(S) byte accounting for S numbered
    /// sections; the new bytes must already include their native page extent.
    pub(crate) fn with_initialize(mut self, locale: u32, bytes: Vec<u8>) -> io::Result<Self> {
        if self.initialize.is_some()
            || bytes.is_empty()
            || bytes.len() > MAX_SECTION_BYTES
            || bytes.len() % 4096 != 0
            || self.sections.values().map(|data| data.len()).sum::<usize>() + bytes.len()
                > MAX_TOTAL_BYTES
        {
            return Err(invalid(
                "invalid or oversized installed NLS initialization snapshot",
            ));
        }
        self.initialize = Some(Initialize {
            locale,
            bytes: bytes.into(),
        });
        Ok(self)
    }

    pub(crate) fn initialize(&self) -> Option<&Initialize> {
        self.initialize.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_nls_initialize_snapshot_bounds_distinct_identity_and_combined_budget() {
        let nls = Nls::new(vec![((14, 0), vec![0x74; 100])])
            .unwrap()
            .with_initialize(0x409, vec![0x53; 8192])
            .unwrap();
        assert_eq!(nls.initialize().unwrap().locale, 0x409);
        assert!(Arc::ptr_eq(
            &nls.initialize().unwrap().bytes,
            &nls.clone().initialize().unwrap().bytes
        ));
        assert!(!Arc::ptr_eq(
            &nls.initialize().unwrap().bytes,
            &nls.section(14, 0).unwrap()
        ));
        assert!(nls.with_initialize(0x409, vec![0; 4096]).is_err());
        for size in [0, 4095, MAX_SECTION_BYTES + 4096] {
            assert!(
                Nls::new(vec![])
                    .unwrap()
                    .with_initialize(0x409, vec![0; size])
                    .is_err()
            );
        }
        let nls = Nls::new(
            (0..8)
                .map(|code| ((11, code), vec![0; MAX_SECTION_BYTES]))
                .collect(),
        )
        .unwrap();
        assert!(nls.with_initialize(0x409, vec![0; 4096]).is_err());
    }
}
