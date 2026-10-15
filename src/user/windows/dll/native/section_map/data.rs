//! Installed non-image section geometry; independent of image preferred bases.
use super::geometry;
use crate::user::windows::hle::Ctx;
use crate::user::windows::memory::{ALLOCATION_GRANULARITY, mem};
use crate::user::windows::nt::status::*;
use crate::user::windows::object_namespace::DataSection;
use std::sync::Arc;

pub(super) struct Request {
    pub(super) handle: u64,
    pub(super) base: u64,
    pub(super) size: u64,
    pub(super) zero: u64,
    pub(super) commit: u64,
    pub(super) offset: u64,
    pub(super) inherit: u32,
    pub(super) flags: u32,
    pub(super) protection: u32,
}
pub(super) enum Failure {
    Status(u32),
    Unsupported(&'static str),
}
impl From<u32> for Failure {
    fn from(status: u32) -> Self {
        Self::Status(status)
    }
}
pub(super) fn map(
    c: &mut Ctx,
    section: Arc<DataSection>,
    request: Request,
) -> Result<(u64, u64), Failure> {
    let Request {
        handle,
        base,
        size,
        zero,
        commit,
        offset,
        inherit,
        flags,
        protection,
    } = request;
    let required = match protection & 0xFF {
        1 | 2 | 8 => 4,
        4 => 2,
        0x10 => 8,
        0x20 | 0x80 => 12,
        0x40 => 10,
        _ => return Err(STATUS_INVALID_PAGE_PROTECTION.into()),
    };
    if c.p.objects.access(handle).unwrap_or(0) & required != required {
        return Err(STATUS_ACCESS_DENIED.into());
    }
    // Build29683 accepts 0x00400000 with unchanged data-view geometry. The
    // original CSR startup combines it with TOP_DOWN as 0x00500000. This
    // section remains the fixed selected data object with its captured rights.
    if !matches!(inherit, 1 | 2) || flags & !(mem::TOP_DOWN | mem::PHYSICAL | 1) != 0 {
        return Err(STATUS_INVALID_PARAMETER.into());
    }
    let high = geometry::ceiling(zero, c.p.vm.high())?;
    if base >= c.p.vm.high()
        || base % ALLOCATION_GRANULARITY != 0
        || offset % ALLOCATION_GRANULARITY != 0
    {
        return Err(if base >= c.p.vm.high() {
            STATUS_INVALID_PARAMETER
        } else {
            STATUS_MAPPED_ALIGNMENT
        }
        .into());
    }
    if commit > section.size {
        return Err(STATUS_INVALID_PARAMETER.into());
    }
    if offset >= section.size || size > section.size - offset {
        return Err(STATUS_INVALID_VIEW_SIZE.into());
    }
    let size = if size == 0 {
        section.size - offset
    } else {
        size.checked_add(4095).ok_or(STATUS_INVALID_PARAMETER)? & !4095
    };
    if size > section.size - offset {
        return Err(STATUS_INVALID_VIEW_SIZE.into());
    }
    if base != 0 && zero != 0 && base.checked_add(size).is_none_or(|end| end > high) {
        return Err(STATUS_INVALID_PARAMETER.into());
    }
    if commit != 0
        && section.regions.iter().any(|r| {
            r.offset < offset.saturating_add(commit)
                && r.offset + r.size > offset
                && r.bytes.is_none()
        })
    {
        return Err(Failure::Unsupported(
            "NtMapViewOfSection requests new commitment beyond selected CSR snapshot",
        ));
    }
    let address = if base == 0 {
        c.p.vm
            .find_free(
                size,
                ALLOCATION_GRANULARITY,
                c.p.vm.low(),
                high,
                flags & mem::TOP_DOWN != 0,
            )
            .ok_or(STATUS_NO_MEMORY)?
    } else {
        if !c.p.vm.is_free(base, size) {
            return Err(STATUS_CONFLICTING_ADDRESSES.into());
        }
        base
    };
    c.p.vm
        .map_data_section(section, address, offset, size, protection)
        .map_err(|e| Failure::Status(e.status()))?;
    Ok((address, size))
}
