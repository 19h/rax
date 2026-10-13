//! Measured ThreadSchedulerSharedDataSlot (57), native24 B / WoW64 12 B.
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

pub(super) fn information(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}
fn checked(c: &mut Ctx) -> ApiResult {
    let (handle, class, input, length) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?);
    if class != 57 {
        return Err(c.unsupported(format!("NtSetInformationThread class {class}")));
    }
    let wow = c.arch() == WinArch::X86;
    let size = if wow { 12 } else { 24 };
    if !wow && length != 0 {
        if input % 4 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        if input
            .checked_add(u64::from(length))
            .is_none_or(|end| end > c.p.vm.high())
        {
            return Flow::ret(STATUS_ACCESS_VIOLATION.into());
        }
        probe_write(c, input, length as usize)?;
    }
    if length != size {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    let mut data = [0; 24];
    c.mem().rd(input, &mut data[..size as usize])?;
    let action = u32::from_le_bytes(data[..4].try_into().unwrap());
    let offset = if wow { 4 } else { 8 };
    let n = c.psize() as usize;
    let mut raw = [0; 8];
    raw[..n].copy_from_slice(&data[offset..offset + n]);
    let owner = u64::from_le_bytes(raw);
    let status = apply(c, handle, action, owner, &mut data, offset + n)?;
    if wow {
        // The wrapper copies all 12 converted bytes even on a kernel status
        // failure. A fault can override the status after assignment/release.
        c.mem().wr(input, &data[..12])?;
    } else if status == STATUS_SUCCESS && action == 0 {
        c.mem().wr(input + 16, &data[16..24])?;
    }
    Flow::ret(status.into())
}
fn apply(
    c: &mut Ctx,
    handle: u64,
    action: u32,
    owner: u64,
    data: &mut [u8; 24],
    slot_at: usize,
) -> Result<u32, crate::user::windows::hle::ApiErr> {
    let tid = if handle == c.arch().ptr(u64::MAX - 1) {
        c.t.tid
    } else if handle == c.arch().ptr(u64::MAX) {
        return Ok(STATUS_OBJECT_TYPE_MISMATCH);
    } else {
        match c.p.objects.get(handle) {
            None => return Ok(STATUS_INVALID_HANDLE),
            Some(Object::Thread { tid, .. }) => {
                if c.p.objects.access(handle).unwrap_or(0) & 0x20 == 0 {
                    return Ok(STATUS_ACCESS_DENIED);
                }
                *tid
            }
            Some(_) => return Ok(STATUS_OBJECT_TYPE_MISMATCH),
        }
    };
    if action > 1 {
        return Ok(STATUS_NOT_SUPPORTED);
    }
    if action == 1 {
        if tid != c.t.tid {
            return Ok(STATUS_NOT_SUPPORTED);
        }
        c.p.objects.free_scheduler_slot(tid);
        return Ok(STATUS_SUCCESS);
    }
    if c.arch() != WinArch::X86 && matches!(owner, u64::MAX | 0xfffffffffffffffe) {
        return Ok(STATUS_OBJECT_TYPE_MISMATCH);
    }
    let Some(id) = c.p.objects.id(owner) else {
        return Ok(STATUS_INVALID_HANDLE);
    };
    match c.p.objects.obj(id) {
        Some(Object::SchedulerSharedData { pid, .. }) => {
            if c.p.objects.access(owner).unwrap_or(0) & 1 == 0 {
                return Ok(STATUS_ACCESS_DENIED);
            }
            if *pid != c.p.pid {
                return Err(c.unsupported("scheduler slot owner in another process"));
            }
        }
        Some(_) => return Ok(STATUS_OBJECT_TYPE_MISMATCH),
        None => return Ok(STATUS_INVALID_HANDLE),
    }
    if tid != c.t.tid {
        return Ok(STATUS_NOT_SUPPORTED);
    }
    match c.p.objects.assign_scheduler_slot(&mut c.p.vm, id, tid) {
        Ok(slot) => {
            let n = c.psize() as usize;
            data[slot_at..slot_at + n].copy_from_slice(&slot.to_le_bytes()[..n]);
            Ok(STATUS_SUCCESS)
        }
        Err(status) => Ok(status),
    }
}
