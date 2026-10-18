//! Capture the native pointer-bearing token result into an owned value.
use super::{Handle, Token, invalid};
use crate::user::windows::nt::status::STATUS_BUFFER_TOO_SMALL;
use crate::user::windows::security::token::bno::Isolation;
use std::ffi::c_void;
use std::io;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationToken(
        token: Handle,
        class: u32,
        output: *mut c_void,
        bytes: u32,
        returned: *mut u32,
    ) -> i32;
}

pub(super) fn capture(token: &Token) -> io::Result<Isolation> {
    let mut required = 0u32;
    // SAFETY: live private query token, empty buffer and exclusive ULONG.
    let status =
        unsafe { NtQueryInformationToken(token.0, 44, std::ptr::null_mut(), 0, &mut required) }
            as u32;
    let width = std::mem::size_of::<usize>();
    if status != STATUS_BUFFER_TOO_SMALL || required < (width * 2) as u32 || required > 65536 {
        return Err(io::Error::other(format!(
            "native TokenBnoIsolation size query failed: status={status:#010x}, bytes={required}"
        )));
    }
    let mut storage = vec![usize::MAX; (required as usize).div_ceil(width)];
    let mut returned = 0u32;
    // SAFETY: storage is initialized and pointer-aligned, with at least
    // `required` writable bytes. No native pointer escapes the decoder below.
    let status = unsafe {
        NtQueryInformationToken(
            token.0,
            44,
            storage.as_mut_ptr().cast(),
            required,
            &mut returned,
        )
    } as u32;
    if status != 0 || returned != required {
        return Err(io::Error::other(format!(
            "native TokenBnoIsolation capture changed: status={status:#010x}, bytes={returned}"
        )));
    }
    // SAFETY: initialized owned storage covers the exact successful byte count.
    let bytes =
        unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), returned as usize) };
    Isolation::from_native(bytes, storage.as_ptr() as u64, width)
        .ok_or_else(|| invalid("native TokenBnoIsolation layout or prefix is invalid"))
}
