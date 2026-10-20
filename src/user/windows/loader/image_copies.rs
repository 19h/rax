//! Initial PE copy potential differs from an explicit later protection change.
use super::{LoadError, PeImage, Proc, region_protect};
use crate::user::image::pe::{IMAGE_SCN_MEM_SHARED, ImageRegion};
use crate::user::windows::memory::prot;
use crate::user::windows::nt::status::STATUS_NO_MEMORY;

fn shared(pe: &PeImage, region: &ImageRegion) -> bool {
    region
        .section
        .is_some_and(|index| pe.sections()[index].characteristics & IMAGE_SCN_MEM_SHARED != 0)
}

pub(super) fn enable(p: &Proc, pe: &PeImage, base: u64) -> Result<(), LoadError> {
    for region in pe.regions() {
        if region.protection.write && shared(pe, &region) {
            continue;
        }
        p.vm.space()
            .track_private_copies(base + u64::from(region.rva), u64::from(region.len))
            .map_err(|_| LoadError::new(STATUS_NO_MEMORY, "cannot track image private copies"))?;
    }
    Ok(())
}

pub(super) fn protect(p: &mut Proc, pe: &PeImage, base: u64) -> Result<(), LoadError> {
    for region in pe.regions() {
        let protection = if region.protection.write && shared(pe, &region) {
            if region.protection.execute {
                prot::EXECUTE_READWRITE
            } else {
                prot::READWRITE
            }
        } else {
            region_protect(&region.protection)
        };
        p.vm.protect_image_initial(
            base + u64::from(region.rva),
            u64::from(region.len),
            protection,
        )
        .map_err(|e| LoadError::new(e.status(), "cannot apply image section protection"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
