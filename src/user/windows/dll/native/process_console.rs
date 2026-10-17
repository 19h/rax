//! Build29683 process-owner information. All state and identifiers are guest-owned.
use super::query::{probe_write, return_length, write_guard_result};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::nt::status::{
    STATUS_ACCESS_DENIED, STATUS_ACCESS_VIOLATION, STATUS_DATATYPE_MISALIGNMENT,
    STATUS_INFO_LENGTH_MISMATCH, STATUS_INVALID_HANDLE, STATUS_INVALID_INFO_CLASS,
    STATUS_INVALID_PARAMETER, STATUS_OBJECT_TYPE_MISMATCH, STATUS_SUCCESS,
};
use crate::user::windows::objects::Object;

pub(super) fn set(c: &mut Ctx, process: u64, input: u64, length: u32) -> ApiResult {
    let wow = c.arch() == WinArch::X86;
    if !wow && length != 0 {
        if input % 8 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        if input
            .checked_add(u64::from(length))
            .is_none_or(|end| end > c.p.vm.high())
        {
            return Flow::ret(STATUS_ACCESS_VIOLATION.into());
        }
    }
    // WoW64 captures one non-NULL unaligned ULONG before the native handle check and
    // ignores excess caller length. NULL passes through to native handle validation.
    // Native64 requires exactly one ULONG_PTR.
    if (wow && length < 4) || (!wow && length != 8) {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    let captured = if wow && input != 0 {
        Some(c.read_ptr(input)?)
    } else {
        None
    };
    if process != c.arch().ptr(u64::MAX) {
        // Even a real self handle with PROCESS_ALL_ACCESS is rejected.
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    let owner = match captured {
        Some(value) => value,
        None => c.read_ptr(input)?,
    };
    if owner & 3 != 1 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    c.p.console_host_process = owner;
    Flow::ret(STATUS_SUCCESS.into())
}

pub(super) fn query(
    c: &mut Ctx,
    process: u64,
    output: u64,
    length: u32,
    returned: u64,
) -> ApiResult {
    if c.arch() == WinArch::X86 {
        // Installed WoW64 has no conversion for this query. Neither caller
        // destination is visited, including guards and malformed pointers.
        return Flow::ret(STATUS_INVALID_INFO_CLASS.into());
    }
    if length != 0 {
        if output % 4 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        if output
            .checked_add(u64::from(length))
            .is_none_or(|end| end > c.p.vm.high())
        {
            return Flow::ret(STATUS_ACCESS_VIOLATION.into());
        }
    }
    // ReturnLength is visited before class length, handle and output pages.
    // A READONLY|GUARD ReturnLength consumes its guard; an output with that
    // protection instead fails write validation without consuming the guard.
    if returned != 0 {
        probe_write(c, returned, 4)?;
    }
    let result = query_checked(c, process, output, length, returned);
    write_guard_result(c, result)
}

fn query_checked(c: &mut Ctx, process: u64, output: u64, length: u32, returned: u64) -> ApiResult {
    if length != 8 {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    if process != c.arch().ptr(u64::MAX) {
        if process == c.arch().ptr(u64::MAX - 1) {
            return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
        }
        let Some(object) = c.p.objects.get(process) else {
            return Flow::ret(STATUS_INVALID_HANDLE.into());
        };
        let Object::Process { pid, .. } = object else {
            return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
        };
        if c.p.objects.access(process).unwrap_or(0) & (0x400 | 0x1000) == 0 {
            return Flow::ret(STATUS_ACCESS_DENIED.into());
        }
        if *pid != c.p.pid {
            return Err(c.unsupported("ProcessConsoleHostProcess for an unmodeled peer"));
        }
    }
    probe_write(c, output, 8)?;
    c.write_ptr(output, c.p.console_host_process)?;
    return_length(c, returned, 8)?;
    Flow::ret(STATUS_SUCCESS.into())
}
