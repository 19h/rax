//! Native29683 captures: native64 reads due/period/parameters; WoW64 converts
//! parameters first. Memory capture precedes typed-handle and value validation.
use super::*;
use crate::user::windows::objects::{ObjId, Timer2Schedule};

#[derive(Default)]
struct Parameters {
    version: u32,
    tolerance: i64,
}

fn parameters(c: &Ctx, pointer: u64) -> Result<Parameters, ApiErr> {
    if pointer == 0 {
        return Ok(Parameters::default());
    }
    // Capture the entire 16-byte input, including ignored Reserved. This
    // preserves cross-page faults and guard consumption at every alignment.
    let data = c.mem().bytes(pointer, 16)?;
    Ok(Parameters {
        version: u32::from_le_bytes(data[..4].try_into().unwrap()),
        tolerance: i64::from_le_bytes(data[8..16].try_into().unwrap()),
    })
}

fn object(c: &Ctx, handle: u64) -> Result<ObjId, u32> {
    if handle == c.arch().ptr(u64::MAX) || handle == c.arch().ptr(u64::MAX - 1) {
        return Err(STATUS_OBJECT_TYPE_MISMATCH);
    }
    let id = c.p.objects.id(handle).ok_or(STATUS_INVALID_HANDLE)?;
    if !matches!(c.p.objects.obj(id), Some(Object::IrTimer { .. })) {
        return Err(STATUS_OBJECT_TYPE_MISMATCH);
    }
    if c.p.objects.access(handle).unwrap_or(0) & 2 == 0 {
        return Err(STATUS_ACCESS_DENIED);
    }
    Ok(id)
}

pub(in super::super) fn set(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    nt_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (handle, due_ptr, period_ptr, parameters_ptr) =
        (c.ptr(0)?, c.ptr(1)?, c.ptr(2)?, c.ptr(3)?);
    let wow = c.arch() == WinArch::X86;
    let captured = if wow {
        Some(parameters(c, parameters_ptr)?)
    } else {
        None
    };
    if due_ptr == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER_2.into());
    }
    let due = c.mem().u64(due_ptr)? as i64;
    let period = if period_ptr == 0 {
        0
    } else {
        c.mem().u64(period_ptr)? as i64
    };
    let captured = match captured {
        Some(value) => value,
        None => parameters(c, parameters_ptr)?,
    };
    let id = match object(c, handle) {
        Ok(id) => id,
        Err(status) => return Flow::ret(status.into()),
    };
    if captured.version != 0 || captured.tolerance < -1 {
        return Flow::ret(STATUS_INVALID_PARAMETER_3.into());
    }
    if matches!(c.p.objects.obj(id), Some(Object::IrTimer { attributes, .. }) if attributes & IR != 0)
        && period != 0
    {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    let next = Timer2Schedule::new(due, period)
        .map_err(|error| ApiErr::Internal(format!("NtSetTimer2 clock: {error}")))?;
    let Some(Object::IrTimer {
        signaled, schedule, ..
    }) = c.p.objects.obj_mut(id)
    else {
        unreachable!("checked extended timer")
    };
    *signaled = false;
    *schedule = Some(next);
    // NoWakeTolerance permits coalescing; expiring at the requested due time
    // uses zero additional tolerance. No guest pointer reaches a host timer.
    Flow::ret(STATUS_SUCCESS.into())
}

pub(in super::super) fn cancel(c: &mut Ctx) -> ApiResult {
    let handle = c.ptr(0)?;
    // The second argument is reserved and never dereferenced on either ABI.
    let id = match object(c, handle) {
        Ok(id) => id,
        Err(status) => return Flow::ret(status.into()),
    };
    let Some(Object::IrTimer { schedule, .. }) = c.p.objects.obj_mut(id) else {
        unreachable!("checked extended timer")
    };
    *schedule = None;
    Flow::ret(STATUS_SUCCESS.into())
}
