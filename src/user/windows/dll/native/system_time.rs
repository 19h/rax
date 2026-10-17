//! Native ULONG-aligned, all-or-nothing LARGE_INTEGER system-time store.
use super::query::write_guard_result;
use crate::user::clock::{HostClock, read};
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::{STATUS_DATATYPE_MISALIGNMENT, STATUS_SUCCESS};

pub(super) fn query(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    write_guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let output = c.ptr(0)?;
    if output % 4 != 0 {
        return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
    }
    let (seconds, nanoseconds) = read(HostClock::Realtime)
        .map_err(|error| ApiErr::Internal(format!("NtQuerySystemTime: {error}")))?;
    let ticks = super::super::sysinfo::file_time(seconds, nanoseconds).ok_or_else(|| {
        ApiErr::Internal("NtQuerySystemTime: host clock is outside the FILETIME range".into())
    })?;
    c.mem().w64(output, ticks)?;
    Flow::ret(STATUS_SUCCESS.into())
}
