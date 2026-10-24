//! Keep a transferred socket alive until its receiver owns the descriptor.
//! The old route is observed only for hangup; no second reader consumes it.
#[cfg(unix)]
use std::os::fd::{AsRawFd, OwnedFd};

#[cfg(unix)]
#[derive(Debug)]
pub(super) struct PendingAdoption {
    _endpoint: OwnedFd,
    watch: OwnedFd,
}

#[cfg(unix)]
impl PendingAdoption {
    pub(super) fn new(endpoint: OwnedFd, watch: OwnedFd) -> Self {
        Self {
            _endpoint: endpoint,
            watch,
        }
    }

    pub(super) fn watch_fd(&self) -> i32 {
        self.watch.as_raw_fd()
    }

    pub(super) fn abandoned(&self) -> bool {
        super::super::host::poll(&[(self.watch_fd(), false, false)], 0)
            .is_ok_and(|events| events.iter().any(|event| event.hangup || event.error))
    }
}
