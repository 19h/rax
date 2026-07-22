//! Shared CRC-32C recurrence for x86 CRC32 instruction forms.

/// Accumulate `width` little-endian source bytes with the reflected Castagnoli
/// polynomial. The architectural CRC32 family consumes 1, 2, 4, or 8 bytes and
/// always produces a 32-bit remainder. Runtime is O(`width`) and space is O(1).
pub(crate) fn crc32c(mut crc: u32, data: u64, width: u8) -> u32 {
    debug_assert!(matches!(width, 1 | 2 | 4 | 8));
    const POLY_REFLECTED: u32 = 0x82F6_3B78;

    for byte in 0..width {
        crc ^= ((data >> (u32::from(byte) * 8)) & 0xFF) as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (POLY_REFLECTED & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}
