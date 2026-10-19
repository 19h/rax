//! NT system time has 100 ns units and the 1601 UTC epoch.

/// Convert a normalized Unix-epoch sample without narrowing or wrapping.
pub(crate) fn file_time(seconds: i64, nanoseconds: i64) -> Option<u64> {
    const UNIX_EPOCH_SECONDS: i128 = 11_644_473_600;
    let ticks =
        (i128::from(seconds) + UNIX_EPOCH_SECONDS) * 10_000_000 + i128::from(nanoseconds) / 100;
    u64::try_from(ticks).ok()
}
