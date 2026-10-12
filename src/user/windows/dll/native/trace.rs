//! Measured build29683 NtTraceControl/EtwAddNotificationEvent (function27).
//! This retains a guest Event; it does not create a host ETW registration.
use super::query::{guard_result, probe_write};
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

pub(super) fn control(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}
fn checked(c: &mut Ctx) -> ApiResult {
    let function = c.u32(0)?;
    if function != 27 {
        return Err(c.unsupported(format!("NtTraceControl function {function}")));
    }
    let (input, input_bytes, output, output_bytes, returned) =
        (c.ptr(1)?, c.u32(2)?, c.ptr(3)?, c.u32(4)?, c.ptr(5)?);
    // All four measured profiles ignore a NULL optional output, even with a
    // nonzero length. Nonempty non-NULL output is probed before ReturnLength.
    if output != 0 && output_bytes != 0 {
        probe_write(c, output, output_bytes as usize)?;
    }
    if returned == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    probe_write(c, returned, 4)?;
    // The private input is exactly one ULONG in native64 as well as WoW64.
    // Length validation precedes capture; byte-aligned buffers are accepted.
    if input_bytes != 4 || input == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    let handle = u64::from(c.mem().u32(input)?);
    if handle == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    let Some(id) = c.p.objects.id(handle) else {
        return Flow::ret(STATUS_INVALID_HANDLE.into());
    };
    if !matches!(c.p.objects.obj(id), Some(Object::Event { .. })) {
        return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
    }
    if c.p.objects.access(handle).unwrap_or(0) & 2 == 0 {
        return Flow::ret(STATUS_ACCESS_DENIED.into());
    }
    let registered =
        c.p.objects
            .register_etw_event(id)
            .map_err(|message| ApiErr::Internal(message.into()))?;
    if !registered {
        return Flow::ret(STATUS_ALREADY_REGISTERED.into());
    }
    // Capture and publication precede this store, so input/output aliases
    // observe the zero only after the original handle has been retained.
    c.mem().w32(returned, 0)?;
    Flow::ret(STATUS_SUCCESS.into())
}
