//! Installed-build29683 process notification Event registration.
//! No host registration, notification delivery, or manufactured signal occurs.
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

pub(super) fn register_event(c: &mut Ctx) -> ApiResult {
    let handle = c.ptr(0)?;
    if handle == c.arch().ptr(u64::MAX) || handle == c.arch().ptr(u64::MAX - 1) {
        return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
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
            .register_wnf_event(id)
            .map_err(|message| ApiErr::Internal(message.into()))?;
    Flow::ret(
        if registered {
            STATUS_SUCCESS
        } else {
            STATUS_ALREADY_REGISTERED
        }
        .into(),
    )
}
