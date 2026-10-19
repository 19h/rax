//! Owned user-alert consumption and synchronous guest APC delivery.
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::nt::status::{STATUS_ALERTED, STATUS_SUCCESS};
use crate::user::windows::sync::Wait;
use std::time::Instant;

pub(super) fn test(c: &mut Ctx) -> ApiResult {
    // An existing user alert wins over a queued APC and is consumed exactly
    // once. Guest thread state never queries or changes a host thread alert.
    if std::mem::take(&mut c.t.user_alerted) {
        return Flow::ret(u64::from(STATUS_ALERTED));
    }
    if c.t.apcs.is_empty() {
        return Flow::ret(u64::from(STATUS_SUCCESS));
    }
    // Reuse the scheduler's checked wait/APC frame ownership: it captures the
    // interrupted CPU, drains the queue (including nested/appended work), then
    // restores this continuation. The zero deadline cannot delay completion.
    // STATUS_USER_APC is an internal wake reason; NtTestAlert returns SUCCESS
    // after delivery. Callback changes to guest TEB memory remain observable.
    Flow::block(
        Wait::Sleep {
            deadline: Some(Instant::now()),
            alertable: true,
        },
        |_, _| Flow::ret(u64::from(STATUS_SUCCESS)),
    )
}
