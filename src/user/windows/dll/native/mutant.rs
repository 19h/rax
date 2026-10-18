//! Native mutants use guest Mutex ownership, waits and reference lifetime.
mod attributes;
mod query;

use super::query::{guard_result, probe_write, write_guard_result};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::{Mem, MemFault};
use crate::user::windows::nt::status::*;
use crate::user::windows::object_namespace::{LookupError, NativeName};
use crate::user::windows::objects::{ObjId, Object};

pub(super) use query::information as query;

const ALL_ACCESS: u32 = 0x001F_0001;

enum Failure {
    Status(u32),
    Api(ApiErr),
}
impl From<MemFault> for Failure {
    fn from(value: MemFault) -> Self {
        Self::Api(value.into())
    }
}
impl From<ApiErr> for Failure {
    fn from(value: ApiErr) -> Self {
        Self::Api(value)
    }
}
type Checked<T> = Result<T, Failure>;
fn status<T>(value: u32) -> Checked<T> {
    Err(Failure::Status(value))
}
fn finish(c: &mut Ctx, result: Checked<u32>) -> ApiResult {
    finish_with(c, result, false)
}
fn finish_with(c: &mut Ctx, result: Checked<u32>, caller_write: bool) -> ApiResult {
    let result = match result {
        Ok(s) | Err(Failure::Status(s)) => Flow::ret(s.into()),
        Err(Failure::Api(e)) => Err(e),
    };
    let result = if caller_write {
        write_guard_result(c, result)
    } else {
        guard_result(c, result)
    };
    match result {
        Err(ApiErr::Fault(_)) => Flow::ret(STATUS_ACCESS_VIOLATION.into()),
        result => result,
    }
}

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
fn name(c: &Ctx, attrs: &attributes::Attributes, opening: bool) -> Checked<Option<NativeName>> {
    let Some(units) = &attrs.name else {
        return if attrs.root != 0 {
            status(STATUS_OBJECT_NAME_INVALID)
        } else if opening {
            status(STATUS_OBJECT_PATH_SYNTAX_BAD)
        } else {
            Ok(None)
        };
    };
    if units.is_empty() && !opening {
        return Ok(None);
    }
    let root = if attrs.root == 0 {
        None
    } else {
        let pseudo =
            attrs.root == c.arch().ptr(u64::MAX) || attrs.root == c.arch().ptr(u64::MAX - 1);
        let object = c.p.objects.get(attrs.root);
        if !pseudo && object.is_none() {
            return status(STATUS_INVALID_HANDLE);
        }
        if units.first() == Some(&92) {
            return status(STATUS_OBJECT_PATH_SYNTAX_BAD);
        }
        match object {
            Some(Object::Directory(directory)) => Some(directory.clone()),
            _ => return status(STATUS_OBJECT_TYPE_MISMATCH),
        }
    };
    if units.is_empty() {
        return status(if root.is_some() {
            STATUS_OBJECT_TYPE_MISMATCH
        } else {
            STATUS_OBJECT_PATH_SYNTAX_BAD
        });
    }
    c.p.object_namespace
        .named(root, units, attrs.flags)
        .map(Some)
        .map_err(|error| match error {
            LookupError::Status(code) => Failure::Status(code),
            LookupError::Unselected => c
                .unsupported("NT mutant name outside selected guest directory namespace")
                .into(),
        })
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
    attributes::supported(c, &attrs)?;
    let name = name(c, &attrs, opening)?;
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
