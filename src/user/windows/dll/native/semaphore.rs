//! Native semaphores share guest counts, named identity and waits with Win32.
use super::named_object::{self, Checked, Failure, attributes, finish, finish_with, status};
use super::query::{probe_write, return_length};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::{ObjId, Object};

pub(super) fn create(c: &mut Ctx) -> ApiResult {
    let result = create_or_open(c, false);
    finish_with(c, result, c.arch() == WinArch::X86)
}
pub(super) fn open(c: &mut Ctx) -> ApiResult {
    let result = create_or_open(c, true);
    finish_with(c, result, c.arch() == WinArch::X86)
}
fn counts(initial: i32, maximum: i32) -> Checked<()> {
    if maximum <= 0 || initial < 0 || initial > maximum {
        status(STATUS_INVALID_PARAMETER)
    } else {
        Ok(())
    }
}
fn create_or_open(c: &mut Ctx, opening: bool) -> Checked<u32> {
    let (out, desired, address) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?);
    let (initial, maximum) = if opening {
        (0, 1)
    } else {
        (c.i32(3)?, c.i32(4)?)
    };
    let wow = c.arch() == WinArch::X86;
    if !wow {
        probe_write(c, out, c.psize() as usize)?;
        counts(initial, maximum)?;
    }
    let attrs = attributes::capture(c, address, wow)?;
    if wow {
        probe_write(c, out, 4)?;
    }
    // WoW64 converts attributes before the NT call. Count validation belongs
    // to that call, after the caller HANDLE probe and before name lookup.
    let result = counts(initial, maximum)
        .and_then(|()| create_object(c, desired, attrs, initial, maximum, opening));
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
    initial: i32,
    maximum: i32,
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
            .unsupported("permanent NT semaphore lifetime and privilege")
            .into());
    }
    attributes::supported(c, &attrs, "semaphore")?;
    let name = named_object::name(c, &attrs, opening, "semaphore")?;
    let existing = name
        .as_ref()
        .and_then(|name| c.p.objects.by_native_name(name));
    if let Some(id) = existing {
        if !matches!(c.p.objects.obj(id), Some(Object::Semaphore { .. })) {
            return status(STATUS_OBJECT_TYPE_MISMATCH);
        }
        if !opening && attrs.flags & 0x80 == 0 {
            return status(STATUS_OBJECT_NAME_COLLISION);
        }
    } else if opening {
        return status(STATUS_OBJECT_NAME_NOT_FOUND);
    }
    let access = super::object_create::desired_access(c, desired, "semaphore")?;
    let object = Object::Semaphore {
        count: initial,
        max: maximum,
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
fn object(c: &Ctx, handle: u64, required: u32) -> Checked<ObjId> {
    if handle == c.arch().ptr(u64::MAX) || handle == c.arch().ptr(u64::MAX - 1) {
        return status(STATUS_OBJECT_TYPE_MISMATCH);
    }
    let id =
        c.p.objects
            .id(handle)
            .ok_or(Failure::Status(STATUS_INVALID_HANDLE))?;
    if !matches!(c.p.objects.obj(id), Some(Object::Semaphore { .. })) {
        return status(STATUS_OBJECT_TYPE_MISMATCH);
    }
    if c.p.objects.access(handle).unwrap_or(0) & required != required {
        return status(STATUS_ACCESS_DENIED);
    }
    Ok(id)
}

pub(super) fn release(c: &mut Ctx) -> ApiResult {
    let result = release_checked(c);
    finish(c, result)
}
fn release_checked(c: &mut Ctx) -> Checked<u32> {
    let (handle, delta, previous) = (c.ptr(0)?, c.i32(1)?, c.ptr(2)?);
    if previous != 0 {
        probe_write(c, previous, 4)?;
    }
    if delta <= 0 {
        return status(STATUS_INVALID_PARAMETER);
    }
    let id = object(c, handle, 2)?;
    let Some(Object::Semaphore { count, max }) = c.p.objects.obj_mut(id) else {
        unreachable!("checked semaphore")
    };
    let old = *count;
    let Some(next) = count.checked_add(delta).filter(|next| *next <= *max) else {
        return status(STATUS_SEMAPHORE_LIMIT_EXCEEDED);
    };
    *count = next;
    if previous != 0 {
        c.mem().w32(previous, old as u32)?;
    }
    Ok(0)
}

pub(super) fn query(c: &mut Ctx) -> ApiResult {
    let result = query_checked(c);
    finish(c, result)
}
fn query_checked(c: &mut Ctx) -> Checked<u32> {
    let (handle, class, output, length, returned) =
        (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?, c.ptr(4)?);
    if c.arch() == WinArch::X86 && class != 0 {
        return status(STATUS_INVALID_INFO_CLASS);
    }
    // The native service always probes SEMAPHORE_BASIC_INFORMATION, even for
    // a zero or invalid declared length. WoW64 rejects unknown classes first.
    if output % 4 != 0 {
        return status(STATUS_DATATYPE_MISALIGNMENT);
    }
    probe_write(c, output, 8)?;
    if returned != 0 {
        probe_write(c, returned, 4)?;
    }
    if class != 0 {
        return status(STATUS_INVALID_INFO_CLASS);
    }
    if length != 8 {
        return status(STATUS_INFO_LENGTH_MISMATCH);
    }
    let id = object(c, handle, 1)?;
    let Some(Object::Semaphore { count, max }) = c.p.objects.obj(id) else {
        unreachable!("checked semaphore")
    };
    let (count, maximum) = (*count, *max);
    c.mem().w32(output, count as u32)?;
    c.mem().w32(output + 4, maximum as u32)?;
    return_length(c, returned, 8)?;
    Ok(0)
}
