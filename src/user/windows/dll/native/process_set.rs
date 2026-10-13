//! Build29683 scheduler-data constructor; no guest request reaches host NT.
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::prot;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

pub(super) fn information(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    // Native write validation denies READONLY|GUARD without visiting its
    // guard. A writable guarded page instead consumes one reached guard.
    if let Err(ApiErr::Fault(fault)) = &result
        && fault.write
        && let Some(region) = c.p.vm.query(fault.addr)
        && region.protect & prot::GUARD != 0
        && !matches!(
            region.protect & 0xff,
            prot::READWRITE | prot::WRITECOPY | prot::EXECUTE_READWRITE | prot::EXECUTE_WRITECOPY
        )
    {
        return Flow::ret(STATUS_ACCESS_VIOLATION.into());
    }
    guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (process, class, output, length) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?);
    if class != 112 {
        return Err(c.unsupported(format!("NtSetInformationProcess class {class}")));
    }
    let wow = c.arch() == WinArch::X86;
    if !wow && length != 0 && output % 4 != 0 {
        return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
    }
    if !wow
        && length != 0
        && output
            .checked_add(u64::from(length))
            .is_none_or(|end| end > c.p.vm.high())
    {
        return Flow::ret(STATUS_ACCESS_VIOLATION.into());
    }
    if u64::from(length) != c.psize() {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    let status = process_status(c, process)?;
    if status != STATUS_SUCCESS {
        return Flow::ret(status.into());
    }
    if !wow {
        probe_write(c, output, 8)?;
    }
    // The native operation is output-only. Caller contents never influence
    // the owner, fixed grant1 or noninheritable handle flags.
    let Some(id) =
        c.p.objects
            .try_create(Object::SchedulerSharedData { pid: c.p.pid })
    else {
        return Flow::ret(STATUS_INSUFFICIENT_RESOURCES.into());
    };
    let Some(handle) = c.p.objects.open_access(id, false, 1) else {
        c.p.objects.release(id);
        return Flow::ret(STATUS_INSUFFICIENT_RESOURCES.into());
    };
    if let Err(fault) = c.write_ptr(output, handle.into()) {
        // WoW64's successful kernel birth precedes its caller copy. A copy
        // fault leaves the unreturned handle live, as captured by exact handle
        // inventories. Native64 has already probed its stable destination.
        if !wow {
            let _ = c.p.objects.close(handle.into());
        }
        return Err(fault.into());
    }
    Flow::ret(STATUS_SUCCESS.into())
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
            if c.p.objects.access(handle).unwrap_or(0) & 0x200 == 0 {
                Ok(STATUS_ACCESS_DENIED)
            } else if *pid != c.p.pid {
                Err(c.unsupported("scheduler shared data for another process"))
            } else {
                Ok(STATUS_SUCCESS)
            }
        }
        Some(_) => Ok(STATUS_OBJECT_TYPE_MISMATCH),
    }
}
