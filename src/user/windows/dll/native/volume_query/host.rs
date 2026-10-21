//! Filesystem mechanics; synthetic device policy stays in the shared service.
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
use crate::user::windows::objects::FileObj;

pub(super) fn device(file: &FileObj) -> Result<(u32, u32), String> {
    #[cfg(windows)]
    {
        windows::device(file)
    }
    #[cfg(unix)]
    {
        unix::device(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = file;
        Err("host filesystem adapter unavailable".into())
    }
}
