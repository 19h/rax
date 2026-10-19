//! NT timeout capture precedes handle/access checks and dispatcher acquisition.
use super::query::guard_result;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::sync::{self, NativeDeadline, Wait, WaitTargetError};

pub(super) fn single(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    match guard_result(c, result) {
        Err(ApiErr::Fault(_)) => Flow::ret(STATUS_ACCESS_VIOLATION.into()),
        result => result,
    }
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (handle, alertable, timeout) = (c.ptr(0)?, c.u32(1)? as u8 != 0, c.ptr(2)?);
    // Unaligned LARGE_INTEGER pointers are accepted; reads may consume a guard.
    let ticks = if timeout == 0 {
        None
    } else {
        Some(c.mem().u64(timeout)? as i64)
    };
    let id = match sync::wait_object(c.p, c.t, handle) {
        Ok(id) => id,
        Err(WaitTargetError::InvalidHandle) => return Flow::ret(STATUS_INVALID_HANDLE.into()),
        Err(WaitTargetError::AccessDenied) => return Flow::ret(STATUS_ACCESS_DENIED.into()),
        Err(WaitTargetError::Unsupported) => {
            return Err(c.unsupported("native wait object outside the modeled dispatcher profile"));
        }
    };
    let apc = alertable && !c.t.apcs.is_empty();
    if !apc {
        if let Some(status) = sync::try_objects(c.p, c.t.tid, &[id], false)? {
            return Flow::ret(status);
        }
        if ticks == Some(0) {
            return Flow::ret(STATUS_TIMEOUT.into());
        }
    }
    let deadline = ticks
        .map(NativeDeadline::new)
        .transpose()
        .map_err(|error| ApiErr::Internal(format!("native wait clock: {error}")))?;
    if !apc {
        if let Some(deadline) = &deadline {
            if deadline.expired()? {
                return Flow::ret(STATUS_TIMEOUT.into());
            }
        }
    }
    Flow::block(
        Wait::NativeObjects {
            objs: vec![id],
            all: false,
            deadline,
            alertable,
        },
        |_, status| Flow::ret(status),
    )
}
