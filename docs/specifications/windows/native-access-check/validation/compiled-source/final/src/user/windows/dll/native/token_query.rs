//! Captured primary-token properties with measured caller-probe ordering.
use super::query::{guard_result, probe_write};
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

pub(super) fn information(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (token, class, output, length, returned) =
        (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?, c.ptr(4)?);
    if class != 29 {
        return Err(c.unsupported(format!("NtQueryInformationToken class {class}")));
    }
    // Both native64 and WoW64 probe the complete supplied span before token
    // validation. ReturnLength is mandatory and permits unaligned ULONG stores.
    // Do not reject a long span before visiting its first guarded page.
    if length != 0 {
        if output % 4 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        probe_write(c, output, length as usize)?;
    }
    probe_write(c, returned, 4)?;
    let process_token = c.arch().ptr(u64::MAX - 3);
    let thread_token = c.arch().ptr(u64::MAX - 4);
    let effective_token = c.arch().ptr(u64::MAX - 5);
    if token == thread_token {
        return Flow::ret(STATUS_NO_TOKEN.into());
    }
    let mut captured_flag = None;
    if token != process_token && token != effective_token {
        if token == c.arch().ptr(u64::MAX) || token == c.arch().ptr(u64::MAX - 1) {
            return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
        }
        match c.p.objects.get(token) {
            None => return Flow::ret(STATUS_INVALID_HANDLE.into()),
            Some(Object::PrimaryToken) => {
                if c.p.objects.access(token).unwrap_or(0) & 8 == 0 {
                    return Flow::ret(STATUS_ACCESS_DENIED.into());
                }
            }
            Some(Object::ImpersonationToken(subject)) => {
                if c.p.objects.access(token).unwrap_or(0) & 8 == 0 {
                    return Flow::ret(STATUS_ACCESS_DENIED.into());
                }
                captured_flag = Some(subject.app_container);
            }
            Some(Object::Opaque("Token")) => {
                return Err(c.unsupported("opaque token has no modeled primary subject"));
            }
            Some(_) => return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into()),
        }
    }
    let Some(app_container) = captured_flag.or_else(|| c.p.objects.primary_token_app_container())
    else {
        return Err(c.unsupported("primary-token subject is not captured"));
    };
    // Native writes the required length before either short-buffer return or
    // successful data publication. Overlapping output therefore wins.
    c.mem().w32(returned, 4)?;
    if length < 4 {
        return Flow::ret(STATUS_BUFFER_TOO_SMALL.into());
    }
    c.mem().w32(output, u32::from(app_container))?;
    Flow::ret(STATUS_SUCCESS.into())
}
