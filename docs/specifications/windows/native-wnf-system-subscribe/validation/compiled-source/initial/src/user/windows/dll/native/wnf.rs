//! Installed-build29683 process subscriptions and notification Event ownership.
//! Catalog and subject selection occur before execution; guest calls remain local.
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;
use crate::user::windows::wnf::{Name, Permission};

pub(super) fn subscribe(c: &mut Ctx) -> ApiResult {
    let result = subscribe_checked(c);
    guard_result(c, result)
}
fn subscribe_checked(c: &mut Ctx) -> ApiResult {
    let (input, stamp, mask, output) = (c.ptr(0)?, c.u32(1)?, c.u32(2)?, c.ptr(3)?);
    if c.arch() == WinArch::X86 {
        // The measured wrapper first reads the output's starting DWORD,
        // captures the name, and calls with a private eight-byte output. Its
        // final full-span write can fail after publication, overriding the
        // captured-input status without partially changing the guest output.
        if output != 0 {
            c.mem().u32(output)?;
        }
        let captured = c.mem().u64(input).map_err(ApiErr::from);
        let result = captured.and_then(|name| subscribe_name(c, name, stamp, mask));
        let id = result.as_ref().map_or(0, |(_, id)| *id);
        let result = result.and_then(|(status, _)| Flow::ret(status.into()));
        // Consume an input guard before the final copy, even when that copy
        // overrides its status with an access violation.
        let result = guard_result(c, result);
        if output != 0 {
            probe_write(c, output, 8)?;
            c.mem().w64(output, id)?;
        }
        return result;
    }
    // Native64 probes and clears the entire output before capturing the name.
    // All measured profiles accept byte-aligned fixed eight-byte operands.
    if output != 0 {
        probe_write(c, output, 8)?;
        c.mem().w64(output, 0)?;
    }
    let name = c.mem().u64(input)?;
    let (status, id) = subscribe_name(c, name, stamp, mask)?;
    if output != 0 && status == STATUS_SUCCESS {
        c.mem().w64(output, id)?;
    }
    Flow::ret(status.into())
}
fn subscribe_name(c: &mut Ctx, name: u64, stamp: u32, mask: u32) -> Result<(u32, u64), ApiErr> {
    if Name::decode(name).is_none() || mask & !0x1F != 0 {
        return Ok((STATUS_INVALID_PARAMETER, 0));
    }
    match c.p.objects.wnf_permission(name, mask) {
        None => return Err(c.unsupported("WNF provisioning/subject metadata was not selected")),
        Some(Permission::Missing) => return Ok((STATUS_OBJECT_NAME_NOT_FOUND, 0)),
        Some(Permission::Denied) => return Ok((STATUS_ACCESS_DENIED, 0)),
        Some(Permission::Unmodeled) => {
            return Err(c.unsupported(
                "WNF state scope or security descriptor is outside the selected subscription profile",
            ));
        }
        Some(Permission::Allowed) => {}
    }
    c.p.objects
        .wnf_subscription_stamp(name, mask)
        .map_err(|message| c.unsupported(message))?;
    let id =
        c.p.objects
            .subscribe_wnf(name, stamp, mask)
            .map_err(|message| ApiErr::Internal(message.into()))?;
    Ok((STATUS_SUCCESS, id))
}
pub(super) fn unsubscribe(c: &mut Ctx) -> ApiResult {
    let result = unsubscribe_checked(c);
    guard_result(c, result)
}
fn unsubscribe_checked(c: &mut Ctx) -> ApiResult {
    let input = c.ptr(0)?;
    let name = c.mem().u64(input)?;
    if Name::decode(name).is_none() {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    Flow::ret(
        if c.p.objects.unsubscribe_wnf(name) {
            STATUS_SUCCESS
        } else {
            STATUS_OBJECT_NAME_NOT_FOUND
        }
        .into(),
    )
}

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
