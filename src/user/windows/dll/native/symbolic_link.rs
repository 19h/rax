//! Snapshot-backed NtQuerySymbolicLinkObject; build29683 native/WoW64 ordering.
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow, Value};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

fn finish(c: &mut Ctx, result: ApiResult) -> ApiResult {
    match guard_result(c, result) {
        Err(ApiErr::Fault(_)) => Flow::ret(STATUS_ACCESS_VIOLATION.into()),
        r => r,
    }
}
pub(super) fn query(c: &mut Ctx) -> ApiResult {
    let result = captured(c);
    finish(c, result)
}
fn captured(c: &mut Ctx) -> ApiResult {
    let (handle, descriptor, returned) = (c.ptr(0)?, c.ptr(1)?, c.ptr(2)?);
    let wow = c.arch() == WinArch::X86;
    if !wow {
        if descriptor % 2 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        probe_write(c, descriptor, 4)?;
    }
    // WoW64 captures the input descriptor into a native temporary first. Its
    // Length store follows every kernel status, including BUFFER_TOO_SMALL.
    let original_length = c.mem().u16(descriptor)?;
    let maximum = c.mem().u16(descriptor + 2)?;
    let buffer = c.read_ptr(descriptor + if wow { 4 } else { 8 })?;
    let result = queried(c, handle, buffer, maximum, descriptor, returned, wow);
    if wow {
        let (flow, length) = match result {
            Ok((flow, length)) => (Ok(flow), length),
            // Resolve the private kernel phase before copying the WoW64
            // descriptor. A later caller-copy fault can replace its status,
            // but cannot undo consumption of a kernel buffer/length guard.
            Err(error) => (finish(c, Err(error)), None),
        };
        // Copy the complete captured 8-byte descriptor atomically, including
        // unchanged MaximumLength/Buffer. Cross-page write denial leaves it
        // intact; aliasing ReturnLength cannot persist a kernel descriptor edit.
        let mut converted = [0u8; 8];
        converted[..2].copy_from_slice(&length.unwrap_or(original_length).to_le_bytes());
        converted[2..4].copy_from_slice(&maximum.to_le_bytes());
        converted[4..].copy_from_slice(&(buffer as u32).to_le_bytes());
        c.mem().wr(descriptor, &converted)?;
        flow
    } else {
        result.map(|(flow, _)| flow)
    }
}
fn queried(
    c: &mut Ctx,
    handle: u64,
    buffer: u64,
    maximum: u16,
    descriptor: u64,
    returned: u64,
    wow: bool,
) -> Result<(Flow, Option<u16>), ApiErr> {
    // Buffer is byte-aligned and its entire declared capacity is probed before
    // handle/type/grant checks. A zero capacity does not touch Buffer.
    probe_write(c, buffer, usize::from(maximum))?;
    if returned != 0 {
        probe_write(c, returned, 4)?;
    }
    let Some(object) = c.p.objects.get(handle) else {
        let pseudo = handle == c.arch().ptr(u64::MAX) || handle == c.arch().ptr(u64::MAX - 1);
        return Ok((
            Flow::Ret(Value::Int(u64::from(if pseudo {
                STATUS_OBJECT_TYPE_MISMATCH
            } else {
                STATUS_INVALID_HANDLE
            }))),
            None,
        ));
    };
    let Object::SymbolicLink(link) = object else {
        return Ok((
            Flow::Ret(Value::Int(STATUS_OBJECT_TYPE_MISMATCH.into())),
            None,
        ));
    };
    if c.p.objects.access(handle).unwrap_or(0) & 1 == 0 {
        return Ok((Flow::Ret(Value::Int(STATUS_ACCESS_DENIED.into())), None));
    }
    let length = (link.target.len() * 2) as u16;
    let required = u32::from(length) + if returned == 0 { 0 } else { 2 };
    let status = if u32::from(maximum) < required {
        STATUS_BUFFER_TOO_SMALL
    } else {
        let mut bytes: Vec<_> = link.target.iter().flat_map(|u| u.to_le_bytes()).collect();
        if returned != 0 {
            bytes.extend_from_slice(&[0, 0]);
        }
        c.mem().wr(buffer, &bytes)?;
        if !wow {
            c.mem().w16(descriptor, length)?;
        }
        STATUS_SUCCESS
    };
    if returned != 0 {
        c.mem().w32(returned, required)?;
    }
    Ok((
        Flow::Ret(Value::Int(status.into())),
        (status == STATUS_SUCCESS).then_some(length),
    ))
}
