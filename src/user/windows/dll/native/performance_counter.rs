//! Measured sequential LARGE_INTEGER stores, each with ULONG alignment.
use super::query::write_guard_result;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::{STATUS_DATATYPE_MISALIGNMENT, STATUS_SUCCESS};

pub(super) fn query(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    write_guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (counter_output, frequency_output) = (c.ptr(0)?, c.ptr(1)?);
    if counter_output % 4 != 0 {
        return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
    }
    let counter =
        c.p.performance_clock
            .counter(c.p.start_time)
            .map_err(|error| c.unsupported(format!("performance clock: {error}")))?;
    c.mem().w64(counter_output, counter)?;

    // An optional frequency fault must retain the counter already published.
    // Aliases observe the same counter-first, frequency-second store order.
    if frequency_output != 0 {
        if frequency_output % 4 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        c.mem()
            .w64(frequency_output, c.p.performance_clock.frequency())?;
    }
    Flow::ret(STATUS_SUCCESS.into())
}
