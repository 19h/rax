//! Native mutants use guest Mutex ownership, waits and reference lifetime.
mod query;

use super::named_object::{self, Checked, Failure, attributes, finish, finish_with, status};
use super::query::probe_write;
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::{ObjId, Object};

pub(super) use query::information as query;

const ALL_ACCESS: u32 = 0x001F_0001;

fn grant(c: &Ctx, desired: u32) -> Checked<u32> {
    if desired & 0x0100_0000 != 0 {
        return Err(c
            .unsupported("NT mutant ACCESS_SYSTEM_SECURITY privilege")
            .into());
    }
    let mut access = desired & ALL_ACCESS;
    for (generic, mapped) in [
        (0x8000_0000, 0x0002_0001),
        (0x4000_0000, 0x0002_0000),
        (0x2000_0000, 0x0012_0000),
        (0x1000_0000, ALL_ACCESS),
        (0x0200_0000, ALL_ACCESS),
    ] {
        if desired & generic != 0 {
            access |= mapped;
        }
    }
    Ok(access)
}

pub(super) fn create(c: &mut Ctx) -> ApiResult {
    let result = create_or_open(c, false);
    finish_with(c, result, c.arch() == WinArch::X86)
}
pub(super) fn open(c: &mut Ctx) -> ApiResult {
    let result = create_or_open(c, true);
    finish_with(c, result, c.arch() == WinArch::X86)
}
fn create_or_open(c: &mut Ctx, opening: bool) -> Checked<u32> {
    let (out, desired, address) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?);
    let owner = if opening { false } else { c.u32(3)? as u8 != 0 };
    let wow = c.arch() == WinArch::X86;
    if !wow {
        probe_write(c, out, c.psize() as usize)?;
    }
    let attrs = attributes::capture(c, address, wow)?;
    if wow {
        probe_write(c, out, 4)?;
    }
    let result = create_object(c, desired, attrs, owner, opening);
    let (code, handle) = match result {
        Ok((code, handle)) => (code, Some(handle)),
        Err(Failure::Status(code)) => (code, None),
        Err(error) => return Err(error),
    };
    if handle.is_some() || wow {
        if let Err(fault) = c.write_ptr(out, handle.map_or(0, u64::from)) {
            if let Some(handle) = handle {
                let _ = c.p.objects.close(u64::from(handle));
            }
            return Err(fault.into());
        }
    }
    Ok(code)
}
fn create_object(
    c: &mut Ctx,
    desired: u32,
    attrs: Option<attributes::Attributes>,
    owner: bool,
    opening: bool,
) -> Checked<(u32, u32)> {
    if opening && attrs.is_none() {
        return status(STATUS_INVALID_PARAMETER);
    }
    let attrs = attrs.unwrap_or_default();
    if attrs.flags & !0x1EF2 != 0 {
        return status(STATUS_INVALID_PARAMETER);
    }
    if attrs.flags & 0x10 != 0 {
        return Err(c
            .unsupported("permanent NT mutant lifetime and privilege")
            .into());
    }
    attributes::supported(c, &attrs, "mutant")?;
    let name = named_object::name(c, &attrs, opening, "mutant")?;
    let existing = name
        .as_ref()
        .and_then(|name| c.p.objects.by_native_name(name));
    if let Some(id) = existing {
        if !matches!(c.p.objects.obj(id), Some(Object::Mutex { .. })) {
            return status(STATUS_OBJECT_TYPE_MISMATCH);
        }
        if !opening && attrs.flags & 0x80 == 0 {
            return status(STATUS_OBJECT_NAME_COLLISION);
        }
    } else if opening {
        return status(STATUS_OBJECT_NAME_NOT_FOUND);
    }
    let access = grant(c, desired)?;
    let object = Object::Mutex {
        owner: owner.then_some(c.t.tid),
        count: u32::from(owner),
        abandoned: false,
    };
    let (id, existed) = if let Some(id) = existing {
        (id, true)
    } else if let Some(name) = name {
        c.p.objects
            .try_create_native_named(name, object)
            .ok_or(Failure::Status(STATUS_INSUFFICIENT_RESOURCES))?
    } else {
        (
            c.p.objects
                .try_create(object)
                .ok_or(Failure::Status(STATUS_INSUFFICIENT_RESOURCES))?,
            false,
        )
    };
    let Some(handle) = c.p.objects.open_access(id, attrs.flags & 2 != 0, access) else {
        if !existed {
            c.p.objects.release(id);
        }
        return status(STATUS_INSUFFICIENT_RESOURCES);
    };
    Ok((if existed && !opening { 0x4000_0000 } else { 0 }, handle))
}

fn object(c: &Ctx, handle: u64, query: bool) -> Checked<ObjId> {
    if handle == c.arch().ptr(u64::MAX) || handle == c.arch().ptr(u64::MAX - 1) {
        return status(STATUS_OBJECT_TYPE_MISMATCH);
    }
    let id =
        c.p.objects
            .id(handle)
            .ok_or(Failure::Status(STATUS_INVALID_HANDLE))?;
    if !matches!(c.p.objects.obj(id), Some(Object::Mutex { .. })) {
        return status(STATUS_OBJECT_TYPE_MISMATCH);
    }
    if query && c.p.objects.access(handle).unwrap_or(0) & 1 == 0 {
        return status(STATUS_ACCESS_DENIED);
    }
    Ok(id)
}
pub(super) fn release(c: &mut Ctx) -> ApiResult {
    let result = release_checked(c);
    finish(c, result)
}
fn release_checked(c: &mut Ctx) -> Checked<u32> {
    let (handle, previous) = (c.ptr(0)?, c.ptr(1)?);
    if previous != 0 {
        probe_write(c, previous, 4)?;
    }
    let id = object(c, handle, false)?;
    let Some(Object::Mutex { owner, count, .. }) = c.p.objects.obj_mut(id) else {
        unreachable!("checked mutant")
    };
    if *owner != Some(c.t.tid) || *count == 0 {
        return status(0xC000_0046);
    }
    let old = 1u32.wrapping_sub(*count);
    *count -= 1;
    if *count == 0 {
        *owner = None;
    }
    if previous != 0 {
        c.mem().w32(previous, old)?;
    }
    Ok(0)
}
