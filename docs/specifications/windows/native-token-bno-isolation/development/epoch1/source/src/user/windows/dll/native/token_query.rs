//! Captured primary-token properties with measured caller-probe ordering.
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;
use crate::user::windows::security::token::scalars::{self, CapturedScalar};

pub(super) fn information(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (token, class, output, length, returned) =
        (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?, c.ptr(4)?);
    if !matches!(class, 12 | 29 | 44) && !scalars::is_class(class) {
        return Err(c.unsupported(format!("NtQueryInformationToken class {class}")));
    }
    // These WoW64 wrappers reject the selector without touching either
    // destination, unlike the native64 class checks below.
    if c.arch() == WinArch::X86 && matches!(class, 40 | 45) {
        return Flow::ret(STATUS_INVALID_INFO_CLASS.into());
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
    let property = c.p.objects.primary_token_scalar(class);
    // A captured native invalid-class result follows both probes but precedes
    // handle/type/access checks. It never publishes ReturnLength.
    if property == Some(CapturedScalar::InvalidClass) {
        return Flow::ret(STATUS_INVALID_INFO_CLASS.into());
    }
    // TOKEN_ELEVATION publishes its exact required size before validating
    // the handle, and rejects both shorter and longer buffers.
    if class == 20 {
        c.mem().w32(returned, 4)?;
        if length != 4 {
            return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
        }
    }
    let process_token = c.arch().ptr(u64::MAX - 3);
    let thread_token = c.arch().ptr(u64::MAX - 4);
    let effective_token = c.arch().ptr(u64::MAX - 5);
    if token == thread_token {
        return Flow::ret(STATUS_NO_TOKEN.into());
    }
    let mut captured_value = None;
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
                if class != 29 {
                    return Err(c.unsupported(if class == 12 {
                        "impersonation-token SessionId is not captured".to_owned()
                    } else {
                        format!("impersonation-token class {class} is not captured")
                    }));
                }
                captured_value = Some(u32::from(subject.app_container));
            }
            Some(Object::Opaque("Token")) => {
                return Err(c.unsupported("opaque token has no modeled primary subject"));
            }
            Some(_) => return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into()),
        }
    }
    if class == 44 {
        return bno_information(c, output, length, returned);
    }
    let primary = match class {
        12 if c.p.objects.primary_token_app_container().is_some() => {
            c.p.objects.primary_token_session_id()
        }
        29 => c.p.objects.primary_token_app_container().map(u32::from),
        _ if c.p.objects.primary_token_app_container().is_some() => match property {
            Some(CapturedScalar::Value(value)) => Some(value),
            _ => None,
        },
        _ => None,
    };
    let Some(value) = captured_value.or(primary) else {
        return Err(c.unsupported(if class == 12 {
            "primary-token SessionId is not captured"
        } else if class == 29 {
            "primary-token subject is not captured"
        } else {
            "primary-token scalar property is not captured"
        }));
    };
    // Native writes the required length before either short-buffer return or
    // successful data publication. Overlapping output therefore wins.
    let required = scalars::width(class).unwrap_or(4);
    c.mem().w32(returned, required)?;
    if length < required {
        return Flow::ret(STATUS_BUFFER_TOO_SMALL.into());
    }
    if required == 1 {
        c.mem().w8(output, value as u8)?;
    } else {
        c.mem().w32(output, value)?;
    }
    Flow::ret(STATUS_SUCCESS.into())
}

fn bno_information(c: &mut Ctx, output: u64, length: u32, returned: u64) -> ApiResult {
    if c.p.objects.primary_token_app_container().is_none() {
        return Err(c.unsupported("primary-token subject is not captured"));
    }
    let value =
        c.p.objects
            .primary_token_bno()
            .ok_or_else(|| c.unsupported("primary-token BNO isolation is not captured"))?;
    let width = c.psize() as usize;
    let required = value
        .required_bytes(width)
        .ok_or_else(|| c.unsupported("TokenBnoIsolation pointer width is unsupported"))?;
    c.mem().w32(returned, required)?;
    if length < required {
        return Flow::ret(STATUS_BUFFER_TOO_SMALL.into());
    }
    let header = (2 * width) as u64;
    let pointer = if value.prefix().is_some() {
        output + header
    } else {
        0
    };
    c.write_ptr(output, pointer)?;
    c.mem()
        .w8(output + width as u64, u8::from(value.prefix().is_some()))?;
    if let Some(prefix) = value.prefix() {
        for (i, unit) in prefix.iter().copied().chain(std::iter::once(0)).enumerate() {
            c.mem().w16(output + header + 2 * i as u64, unit)?;
        }
    }
    Flow::ret(STATUS_SUCCESS.into())
}
