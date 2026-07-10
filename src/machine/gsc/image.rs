//! Ti50/Dauntless signed-image mapping and entry-point selection.

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::error::Result;
use crate::machine::RiscVBootInfo;

/// Cr50/Ti50-family `SignedHeader` magic at file offset 0.
const GSC_HEADER_MAGIC: u32 = 0xFFFF_FFFD;
/// SignedHeader region-descriptor: total image size in the slot.
const HDR_IMAGE_SIZE: usize = 0x328;
/// SignedHeader region-descriptor: read-only/load base address.
const HDR_RO_BASE: usize = 0x32C;
/// SignedHeader vector struct: the `_start` entry point.
const HDR_ENTRY: usize = 0x404;
/// Minimum size used to distinguish an RW firmware slot in a full-flash image.
const GSC_RW_IMAGE_THRESHOLD: u64 = 0x3_0000;

fn rd_u32(buf: &[u8], off: usize) -> Option<u32> {
    buf.get(off..off + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn env_hex(name: &str) -> Option<u64> {
    let raw = std::env::var(name).ok()?;
    let value = raw.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u64::from_str_radix(value, 16).ok()
}

/// Choose the entry point for either a self-contained slot or an A/B flash.
fn select_entry(buf: &[u8], load_base: u64) -> u64 {
    let header_image_size = rd_u32(buf, HDR_IMAGE_SIZE).map(u64::from).unwrap_or(0);
    let header_entry = rd_u32(buf, HDR_ENTRY).map(u64::from).unwrap_or(load_base);
    if header_image_size == buf.len() as u64 {
        return header_entry;
    }

    let mut offset = 0x1000usize;
    while offset + HDR_ENTRY + 4 <= buf.len() {
        if rd_u32(buf, offset) == Some(GSC_HEADER_MAGIC) {
            let image_size = rd_u32(buf, offset + HDR_IMAGE_SIZE)
                .map(u64::from)
                .unwrap_or(0);
            if image_size >= GSC_RW_IMAGE_THRESHOLD {
                if let Some(entry) = rd_u32(buf, offset + HDR_ENTRY).filter(|&entry| entry != 0) {
                    return u64::from(entry);
                }
            }
        }
        offset += 0x1000;
    }
    header_entry
}

/// Map a GSC flash image and return its boot metadata.
///
/// Signed images are mapped verbatim at their `ro_base`, preserving
/// `virtual_address = file_offset + ro_base`. A blob without a SignedHeader is
/// treated as a raw image at address zero. `RAX_GSC_ENTRY` overrides automatic
/// entry selection.
pub(crate) fn load(mem: &GuestMemoryMmap, buf: &[u8]) -> Result<RiscVBootInfo> {
    if rd_u32(buf, 0) != Some(GSC_HEADER_MAGIC) {
        mem.write_slice(buf, GuestAddress(0))?;
        return Ok(RiscVBootInfo {
            entry_point: 0,
            load_addr: 0,
            image_size: buf.len() as u64,
            tohost_addr: None,
        });
    }

    let load_base = rd_u32(buf, HDR_RO_BASE)
        .map(u64::from)
        .filter(|&value| value != 0)
        .unwrap_or(0);
    mem.write_slice(buf, GuestAddress(load_base))?;
    let entry_point = env_hex("RAX_GSC_ENTRY").unwrap_or_else(|| select_entry(buf, load_base));

    Ok(RiscVBootInfo {
        entry_point,
        load_addr: load_base,
        image_size: buf.len() as u64,
        tohost_addr: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(buf: &mut [u8], off: usize, value: u32) {
        buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rd_u32_reads_le_and_bounds_checks() {
        let buf = [0x11u8, 0x22, 0x33, 0x44, 0x55];
        assert_eq!(rd_u32(&buf, 0), Some(0x4433_2211));
        assert_eq!(rd_u32(&buf, 1), Some(0x5544_3322));
        assert_eq!(rd_u32(&buf, 2), None);
    }

    #[test]
    fn single_slot_uses_header_entry() {
        let mut buf = vec![0u8; 0x4_0000];
        put(&mut buf, 0, GSC_HEADER_MAGIC);
        put(&mut buf, HDR_RO_BASE, 0xa_0000);
        let total = buf.len() as u32;
        put(&mut buf, HDR_IMAGE_SIZE, total);
        put(&mut buf, HDR_ENTRY, 0xa_043c);
        assert_eq!(select_entry(&buf, 0xa_0000), 0xa_043c);
    }

    #[test]
    fn multi_slot_picks_first_rw_entry() {
        let mut buf = vec![0u8; 0x10_0000];
        put(&mut buf, 0, GSC_HEADER_MAGIC);
        put(&mut buf, HDR_RO_BASE, 0x8_0000);
        put(&mut buf, HDR_IMAGE_SIZE, 0x1_4000);
        put(&mut buf, HDR_ENTRY, 0x9_0278);
        put(&mut buf, 0x1_5000, GSC_HEADER_MAGIC);
        put(&mut buf, 0x1_5000 + HDR_IMAGE_SIZE, 0x5_a000);
        put(&mut buf, 0x1_5000 + HDR_ENTRY, 0x9_56b2);
        assert_eq!(select_entry(&buf, 0x8_0000), 0x9_56b2);
    }

    #[test]
    fn signed_image_maps_at_ro_base() {
        let mut buf = vec![0u8; 0x4_0000];
        put(&mut buf, 0, GSC_HEADER_MAGIC);
        put(&mut buf, HDR_RO_BASE, 0xa_0000);
        let total = buf.len() as u32;
        put(&mut buf, HDR_IMAGE_SIZE, total);
        put(&mut buf, HDR_ENTRY, 0xa_043c);
        put(&mut buf, 0x400, 0xdead_beef);

        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap();
        let info = load(&mem, &buf).unwrap();
        assert_eq!(info.load_addr, 0xa_0000);
        assert_eq!(info.entry_point, 0xa_043c);
        assert_eq!(info.image_size, buf.len() as u64);
        let mut word = [0u8; 4];
        mem.read_slice(&mut word, GuestAddress(0xa_0000 + 0x400))
            .unwrap();
        assert_eq!(u32::from_le_bytes(word), 0xdead_beef);
    }

    #[test]
    fn unsigned_blob_falls_back_to_address_zero() {
        let buf = vec![0x13u8, 0x05, 0x00, 0x00, 0xaa, 0xbb];
        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x1_0000usize)]).unwrap();
        let info = load(&mem, &buf).unwrap();
        assert_eq!(info.load_addr, 0);
        assert_eq!(info.entry_point, 0);
    }
}
