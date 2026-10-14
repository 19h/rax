//! A byte-exact kernel image template, before user-loader imports or relocation.
use super::invalid;
use crate::user::image::pe::{DataDirectory, relocs};
use crate::user::windows::memory::{mem, valid_protection};
use std::borrow::Cow;
use std::io;
use std::sync::Arc;

pub(crate) const MAX_IMAGE_BYTES: u64 = 64 << 20;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImageRegion {
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(crate) state: u32,
    pub(crate) protection: u32,
}
pub(crate) struct ImageTemplate {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) regions: Vec<ImageRegion>,
    pub(crate) preferred: u64,
    pub(crate) machine: u16,
    nt_offset: usize,
    wide: bool,
    relocations: DataDirectory,
}
impl std::fmt::Debug for ImageTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageTemplate")
            .field("bytes", &self.bytes.len())
            .field("regions", &self.regions)
            .field("preferred", &self.preferred)
            .field("machine", &self.machine)
            .finish()
    }
}
impl ImageTemplate {
    /// O(B + R) validation/storage for B bytes and R contiguous region runs.
    /// The preferred base and relocation words are preserved from the kernel
    /// view; this operation does not apply a second relocation delta.
    pub(crate) fn selected(bytes: Vec<u8>, regions: Vec<ImageRegion>) -> io::Result<Arc<Self>> {
        let size = bytes.len() as u64;
        if !(4096..=MAX_IMAGE_BYTES).contains(&size)
            || size % 4096 != 0
            || regions.is_empty()
            || regions.len() > 256
            || bytes.get(..2) != Some(b"MZ")
        {
            return Err(invalid(
                "selected kernel image extent/signature exceeds bounds",
            ));
        }
        let u16_at = |at: usize| {
            bytes
                .get(at..at.checked_add(2)?)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
        };
        let u32_at = |at: usize| {
            bytes
                .get(at..at.checked_add(4)?)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        };
        let at = u32_at(0x3C).ok_or_else(|| invalid("selected kernel DOS header"))? as usize;
        if at < 64
            || at > bytes.len().saturating_sub(264)
            || bytes.get(at..at + 4) != Some(b"PE\0\0")
        {
            return Err(invalid("selected kernel PE header is outside the image"));
        }
        let machine = u16_at(at + 4).unwrap();
        let wide = u16_at(at + 24).unwrap() == 0x20B;
        let preferred = match u16_at(at + 24).unwrap() {
            0x10B => u64::from(u32_at(at + 52).unwrap()),
            0x20B => u64::from_le_bytes(bytes[at + 48..at + 56].try_into().unwrap()),
            _ => return Err(invalid("selected kernel optional-header format")),
        };
        if u64::from(u32_at(at + 80).unwrap()) != size || preferred % 65536 != 0 {
            return Err(invalid(
                "selected kernel image extent/preferred-base mismatch",
            ));
        }
        let directory = at + 24 + if wide { 112 } else { 96 } + 5 * 8;
        let relocations = DataDirectory {
            rva: u32_at(directory).unwrap(),
            size: u32_at(directory + 4).unwrap(),
        };
        if relocations.size > (1 << 20)
            || u64::from(relocations.rva) + u64::from(relocations.size) > size
        {
            return Err(invalid(
                "selected kernel relocation directory exceeds the image",
            ));
        }
        let mut end = 0u64;
        for region in &regions {
            if region.offset != end
                || region.size == 0
                || region.size % 4096 != 0
                || !matches!(region.state, mem::COMMIT | mem::RESERVE)
                || region.state == mem::COMMIT && !valid_protection(region.protection)
                || region.state == mem::RESERVE && region.protection != 0
            {
                return Err(invalid("selected kernel image region contract"));
            }
            end = end
                .checked_add(region.size)
                .filter(|&n| n <= size)
                .ok_or_else(|| invalid("selected kernel image region extent"))?;
        }
        if end != size {
            return Err(invalid(
                "selected kernel image regions do not cover its extent",
            ));
        }
        Ok(Arc::new(Self {
            bytes: bytes.into(),
            regions,
            preferred,
            machine,
            nt_offset: at,
            wide,
            relocations,
        }))
    }
    /// MEM_DIFFERENT_IMAGE_BASE_OK performs this kernel rebase before slicing a
    /// partial view. Other allocation flags leave source-relative bytes intact.
    pub(crate) fn view_bytes(&self, image_base: u64, relocate: bool) -> io::Result<Cow<'_, [u8]>> {
        if !relocate || image_base == self.preferred {
            return Ok(Cow::Borrowed(&self.bytes));
        }
        if !self.wide && image_base > u64::from(u32::MAX) {
            return Err(invalid("selected PE32 rebase exceeds its pointer width"));
        }
        let mut bytes = self.bytes.to_vec();
        relocs::apply(
            &mut bytes,
            self.relocations,
            image_base.wrapping_sub(self.preferred),
        )
        .map_err(|error| invalid(&format!("selected kernel image rebase: {error}")))?;
        let at = self.nt_offset + if self.wide { 48 } else { 52 };
        let width = if self.wide { 8 } else { 4 };
        bytes[at..at + width].copy_from_slice(&image_base.to_le_bytes()[..width]);
        Ok(Cow::Owned(bytes))
    }
}
