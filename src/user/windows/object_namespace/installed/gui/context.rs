//! Fixed pointer-free process context from the selected native GUI helper.
use super::{GetProcAddress, Handle, Module};

pub(super) fn capture(win32u: &Module) -> Result<[u32; 2], String> {
    // SAFETY: live selected module and a static terminated export name.
    let address =
        unsafe { GetProcAddress(win32u.0, c"NtUserGetProcessUIContextInformation".as_ptr()) };
    if address.is_null() {
        return Err("selected Win32u has no process UI context query".into());
    }
    // SAFETY: measured HANDLE/output ABI. Only the fixed current-process pseudo
    // handle and an exclusive initialized eight-byte host buffer enter this call.
    let query: unsafe extern "system" fn(Handle, *mut u32) -> i32 =
        unsafe { std::mem::transmute(address) };
    let mut words = [0u32; 2];
    if unsafe { query((-1isize) as Handle, words.as_mut_ptr()) } == 0 {
        return Err(format!(
            "native current-process UI context: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(words)
}
