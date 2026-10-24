//! Mach thread accounting uses explicit seconds/microseconds on native and
//! translated macOS processes. The representation below remains nanoseconds.
use std::io;

pub(super) fn read_thread_cpu() -> io::Result<(i64, i64)> {
    // SAFETY: pthread_self refers to this live thread. Its Mach port is borrowed
    // from libpthread and must not be deallocated by this query.
    let thread = unsafe { libc::pthread_mach_thread_np(libc::pthread_self()) };
    // SAFETY: thread_basic_info contains integer fields, for which zero is valid.
    let mut info: libc::thread_basic_info = unsafe { std::mem::zeroed() };
    let mut count = libc::THREAD_BASIC_INFO_COUNT;
    // SAFETY: the output buffer has the requested flavor's complete size; the
    // kernel writes at most count integer_t values into its writable storage.
    let status = unsafe {
        libc::thread_info(
            thread,
            libc::THREAD_BASIC_INFO as libc::thread_flavor_t,
            (&mut info as *mut libc::thread_basic_info).cast(),
            &mut count,
        )
    };
    if status != libc::KERN_SUCCESS {
        return Err(io::Error::other(format!(
            "thread_info(THREAD_BASIC_INFO) failed: {status}"
        )));
    }
    if count < libc::THREAD_BASIC_INFO_COUNT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "host thread CPU accounting is incomplete",
        ));
    }
    normalize(
        info.user_time.seconds,
        info.user_time.microseconds,
        info.system_time.seconds,
        info.system_time.microseconds,
    )
}

fn normalize(
    user_seconds: i32,
    user_microseconds: i32,
    system_seconds: i32,
    system_microseconds: i32,
) -> io::Result<(i64, i64)> {
    if user_seconds < 0
        || system_seconds < 0
        || !(0..1_000_000).contains(&user_microseconds)
        || !(0..1_000_000).contains(&system_microseconds)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "host thread CPU accounting has invalid time components",
        ));
    }
    // Two i32 second fields plus at most one carry fit in i64. Each fractional
    // field is in microseconds; widening precedes summation and multiplication.
    let microseconds = i64::from(user_microseconds) + i64::from(system_microseconds);
    Ok((
        i64::from(user_seconds) + i64::from(system_seconds) + microseconds / 1_000_000,
        (microseconds % 1_000_000) * 1_000,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thread_accounting_normalizes_units_and_carry_without_overflow() {
        assert_eq!(normalize(1, 999_999, 2, 1).unwrap(), (4, 0));
        assert_eq!(normalize(0, 1, 0, 2).unwrap(), (0, 3_000));
        assert_eq!(
            normalize(i32::MAX, 999_999, i32::MAX, 999_999).unwrap(),
            (4_294_967_295, 999_998_000)
        );
        for values in [
            (-1, 0, 0, 0),
            (0, -1, 0, 0),
            (0, 1_000_000, 0, 0),
            (0, 0, -1, 0),
            (0, 0, 0, -1),
            (0, 0, 0, 1_000_000),
        ] {
            assert_eq!(
                normalize(values.0, values.1, values.2, values.3)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
}
