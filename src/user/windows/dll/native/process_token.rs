//! Primary-token opening from captured rights, with native caller-store order.

use super::query::{probe_write, write_guard_result};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

pub(super) fn open(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    write_guard_result(c, result)
}

fn process_status(c: &Ctx, handle: u64) -> Result<u32, ApiErr> {
    if handle == c.arch().ptr(u64::MAX) {
        return Ok(STATUS_SUCCESS);
    }
    if handle == c.arch().ptr(u64::MAX - 1) {
        return Ok(STATUS_OBJECT_TYPE_MISMATCH);
    }
    match c.p.objects.get(handle) {
        None => Ok(STATUS_INVALID_HANDLE),
        Some(Object::Process { pid, .. }) => {
            if c.p.objects.access(handle).unwrap_or(0) & 0x1400 == 0 {
                return Ok(STATUS_ACCESS_DENIED);
            }
            if *pid != c.p.pid {
                return Err(c.unsupported("primary token of another process is not captured"));
            }
            Ok(STATUS_SUCCESS)
        }
        Some(_) => Ok(STATUS_OBJECT_TYPE_MISMATCH),
    }
}

fn failure(c: &Ctx, output: u64, status: u32) -> ApiResult {
    // WoW64 copies its zero temporary HANDLE even on a kernel error. A NULL
    // caller pointer skips that copy; faults of a non-NULL copy override status.
    if c.arch() == WinArch::X86 && output != 0 {
        c.mem().w32(output, 0)?;
    }
    Flow::ret(status.into())
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (process, desired, output) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?);
    let status = process_status(c, process)?;
    if status != STATUS_SUCCESS {
        return failure(c, output, status);
    }
    let Some(profile) = c.p.objects.primary_token_access() else {
        return Err(c.unsupported("primary-token opening rights are not captured"));
    };
    if c.p.objects.primary_token_app_container().is_none() {
        return Err(c.unsupported("primary-token subject is not captured"));
    }
    let grant = match profile.grant(desired) {
        Ok(grant) => grant,
        Err(status) => return failure(c, output, status),
    };
    // Successful native64 and WoW64 calls probe the complete HANDLE before
    // publication. No failed output access creates a guest object or handle.
    probe_write(c, output, c.psize() as usize)?;
    let Some(handle) = c.p.objects.open_primary_token(grant) else {
        return failure(c, output, STATUS_INSUFFICIENT_RESOURCES);
    };
    if let Err(fault) = c.write_ptr(output, u64::from(handle)) {
        let _ = c.p.objects.close(u64::from(handle));
        return Err(fault.into());
    }
    Flow::ret(STATUS_SUCCESS.into())
}
