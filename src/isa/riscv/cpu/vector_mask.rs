//! Snapshot helpers for mask-producing RVV operations.

use super::{RiscVCpu, VLENB};

impl RiscVCpu {
    pub(super) fn vector_snapshot(&self) -> [u8; 32 * VLENB as usize] {
        self.v
    }

    pub(super) fn snapshot_velem(
        snapshot: &[u8; 32 * VLENB as usize],
        vreg: u8,
        element: usize,
        element_bytes: usize,
    ) -> u64 {
        let offset = vreg as usize * VLENB as usize + element * element_bytes;
        let mut bytes = [0u8; 8];
        if offset + element_bytes <= snapshot.len() {
            bytes[..element_bytes].copy_from_slice(&snapshot[offset..offset + element_bytes]);
        }
        u64::from_le_bytes(bytes)
    }

    pub(super) fn snapshot_mask_bit(snapshot: &[u8; 32 * VLENB as usize], element: usize) -> bool {
        snapshot[element / 8] >> (element % 8) & 1 != 0
    }
}
