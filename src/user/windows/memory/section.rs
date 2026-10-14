//! Private guest frames for immutable pinned kernel image sections.
use super::*;
use crate::user::windows::object_namespace::{ImageSection, ImageTemplate};

#[derive(Debug)]
pub(crate) struct SectionView {
    pub(crate) section: Arc<ImageSection>,
    pub(crate) image_base: u64,
    pub(crate) offset: u64,
    pub(crate) size: u64,
}
#[derive(Debug)]
pub(super) struct LowPlacement {
    // Retain identity so an address key cannot be reused by a different Section.
    _section: Arc<ImageSection>,
    base: u64,
}
impl VirtualMemory {
    pub(crate) fn section_low_placement(&self, section: &Arc<ImageSection>) -> Option<u64> {
        self.section_low_placements
            .get(&(Arc::as_ptr(section) as usize))
            .map(|p| p.base)
            .or(self.default_low_image_placement)
    }
    pub(crate) fn record_section_low_placement(&mut self, section: Arc<ImageSection>, base: u64) {
        self.default_low_image_placement.get_or_insert(base);
        self.section_low_placements
            .entry(Arc::as_ptr(&section) as usize)
            .or_insert(LowPlacement {
                _section: section,
                base,
            });
    }
    /// Maps the caller's already validated image plan. No import binding,
    /// module registration, or PEB publication occurs here. A requested kernel
    /// rebase is applied before slicing the logical view.
    /// O(B + R log P) time for B copied bytes/R region runs/P page-state runs,
    /// plus address-space mapping cost; O(R) retained policy and B guest frames.
    pub(crate) fn map_section_image(
        &mut self,
        section: Arc<ImageSection>,
        image: &ImageTemplate,
        allocation_base: u64,
        image_base: u64,
        allocation_size: u64,
        offset: u64,
        size: u64,
        large: bool,
        relocate: bool,
    ) -> Result<(), VmError> {
        if allocation_base % ALLOCATION_GRANULARITY != 0
            || image_base < allocation_base
            || image_base % PAGE_SIZE != 0
            || size == 0
            || size % PAGE_SIZE != 0
            || offset % PAGE_SIZE != 0
            || offset
                .checked_add(size)
                .is_none_or(|end| end > image.bytes.len() as u64)
            || image_base
                .checked_sub(allocation_base)
                .and_then(|prefix| prefix.checked_add(size))
                .is_none_or(|end| end > allocation_size)
            || allocation_size % PAGE_SIZE != 0
            || section.size != image.bytes.len() as u64
        {
            return Err(VmError::InvalidParameter);
        }
        let data = image
            .view_bytes(
                image_base
                    .checked_sub(offset)
                    .ok_or(VmError::InvalidParameter)?,
                relocate,
            )
            .map_err(|_| VmError::NotSupported)?;
        let name: Arc<str> = String::from_utf16_lossy(&section.path).into();
        let base = self.reserve(
            Some(allocation_base),
            allocation_size,
            prot::EXECUTE_WRITECOPY,
            AllocKind::Image,
            false,
            Some(name),
        )?;
        let result = (|| {
            if large {
                self.commit(base, allocation_size, prot::EXECUTE_WRITECOPY)?;
                self.poke(image_base, &data[offset as usize..(offset + size) as usize])
                    .map_err(|_| VmError::NoMemory)?;
            } else {
                for region in &image.regions {
                    let begin = region.offset.max(offset);
                    let end = (region.offset + region.size).min(offset + size);
                    if begin >= end || region.state != mem::COMMIT {
                        continue;
                    }
                    let address = image_base + begin - offset;
                    self.commit(address, end - begin, prot::READWRITE)?;
                    self.poke(address, &data[begin as usize..end as usize])
                        .map_err(|_| VmError::NoMemory)?;
                    self.protect(address, end - begin, region.protection)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = self.release(base);
            return Err(error);
        }
        self.section_views.insert(
            base,
            SectionView {
                section,
                image_base,
                offset,
                size,
            },
        );
        Ok(())
    }
    pub(crate) fn section_view(&self, address: u64) -> Option<&SectionView> {
        self.allocation(address)
            .and_then(|allocation| self.section_views.get(&allocation.base))
    }
    pub(crate) fn section_image_information(&self, address: u64) -> Option<(u64, u64, u32)> {
        let view = self.section_view(address)?;
        let partial = view.offset != 0 || view.size != view.section.size;
        Some((view.image_base, view.section.size, u32::from(partial)))
    }
}
