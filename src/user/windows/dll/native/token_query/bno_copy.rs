//! Ordered stores for the measured WoW64 BNO prefix conversion.
//!
//! The installed ARM64 NTDLL copy path uses 16-byte stores, with distinct
//! head/tail ordering up to 128 bytes and an eight-byte source alignment bias
//! above that size. Each store can fault independently. The native boundary
//! corpus binds this order; a whole-range preflight would lose visible progress.
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;

pub(super) fn copy(c: &mut Ctx, output: u64, bytes: &[u8]) -> ApiResult {
    for (offset, count) in stores(bytes.len()) {
        c.mem()
            .wr(output + offset as u64, &bytes[offset..offset + count])?;
    }
    Flow::ret(0)
}

/// O(N/16) time and storage, bounded by the captured 64-KiB token record.
fn stores(n: usize) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut put = |at, count| result.push((at, count));
    if n <= 32 {
        let width = if n >= 16 {
            16
        } else if n >= 8 {
            8
        } else if n >= 4 {
            4
        } else {
            0
        };
        if width != 0 {
            put(0, width);
            put(n - width, width);
        } else if n != 0 {
            put(0, 1);
            put(n / 2, 1);
            put(n - 1, 1);
        }
    } else if n <= 128 {
        if n > 96 {
            put(n - 64, 16);
            put(n - 48, 16);
        }
        put(0, 16);
        put(16, 16);
        if n > 64 {
            put(32, 16);
            put(48, 16);
        }
        put(n - 32, 16);
        put(n - 16, 16);
    } else {
        put(0, 16);
        let mut bias = 8usize;
        let mut left = n as isize + 8 - 144;
        while left > 0 {
            for offset in [0, 16, 32, 48] {
                put(bias + offset, 16);
            }
            bias += 64;
            left -= 64;
        }
        for offset in [0, 16, 32, 48] {
            put(bias + offset, 16);
        }
        for offset in [64, 48, 32, 16] {
            put(n - offset, 16);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bno_prefix_store_plan_covers_each_byte_without_exceeding_capture() {
        for n in 0..=65520 {
            let plan = stores(n);
            let mut covered = vec![false; n];
            for (at, count) in plan {
                assert!(count != 0 && at + count <= n);
                covered[at..at + count].fill(true);
            }
            assert!(covered.into_iter().all(|value| value));
        }
    }
}
