//! XNU macOS reclaim policy for user-visible dispatch pressure levels.
//! Kernel Warning and Urgent both become dispatch WARN(2), not their enum values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Policy {
    pub(super) period_ns: u64,
    pub(super) trim_percent: u64,
}
const NORMAL: Policy = Policy {
    period_ns: 10_000_000_000,
    trim_percent: 10,
};

fn select(host_services: bool, query: impl FnOnce() -> Option<u32>) -> Policy {
    if !host_services {
        return NORMAL;
    }
    match query() {
        Some(2) => Policy {
            period_ns: 1_000_000_000,
            trim_percent: 5,
        },
        Some(4) => Policy {
            period_ns: 0,
            trim_percent: 1,
        },
        _ => NORMAL,
    }
}
pub(super) fn current(host_services: bool) -> Policy {
    select(host_services, || {
        crate::user::darwin::syscall::bsd::sysctl::vm_pressure_level()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dispatch_levels_use_kernel_periods_and_trim_thresholds() {
        for (level, period_ns, trim_percent) in
            [(1, 10_000_000_000, 10), (2, 1_000_000_000, 5), (4, 0, 1)]
        {
            assert_eq!(
                select(true, || Some(level)),
                Policy {
                    period_ns,
                    trim_percent
                }
            );
        }
        for level in [None, Some(0), Some(3), Some(u32::MAX)] {
            assert_eq!(select(true, || level), NORMAL);
        }
    }
    #[test]
    fn embedded_policy_never_queries_host_state() {
        assert_eq!(
            select(false, || panic!("closed profile reached host pressure")),
            NORMAL
        );
    }
}
