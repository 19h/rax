//! Selection-only native acquisition into the ABI-neutral attribute snapshot.
use super::{Attributes, MAX_CAPTURE};
use crate::user::windows::nt::status::STATUS_BUFFER_TOO_SMALL;
use std::ffi::c_void;
use std::io;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySecurityAttributesToken(
        token: *mut c_void,
        names: *const c_void,
        count: u32,
        output: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

/// The caller exclusively owns a live TOKEN_QUERY handle for this call.
/// This adapter runs during runtime selection, never for a guest API request.
pub(crate) fn capture(token: *mut c_void) -> io::Result<Attributes> {
    let mut needed = 0;
    // SAFETY: caller-owned live token, NULL/zero names and output, exclusive
    // initialized ULONG. System ABI returns an NTSTATUS and does not unwind.
    let status = unsafe {
        NtQuerySecurityAttributesToken(
            token,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    } as u32;
    if status != STATUS_BUFFER_TOO_SMALL || !(16..=MAX_CAPTURE as u32).contains(&needed) {
        return Err(io::Error::other(format!(
            "native token security-attribute size unavailable: status={status:#010x}, bytes={needed}"
        )));
    }
    // Eight-byte aligned on native32 as well as native64. Padding is initialized
    // but excluded from the successful returned slice.
    let mut storage = vec![0u64; (needed as usize).div_ceil(8)];
    let mut returned = 0;
    // SAFETY: initialized exclusive aligned storage covers `needed` writable
    // bytes. All native pointers are decoded while this allocation is live.
    let status = unsafe {
        NtQuerySecurityAttributesToken(
            token,
            std::ptr::null(),
            0,
            storage.as_mut_ptr().cast(),
            needed,
            &mut returned,
        )
    } as u32;
    if status != 0 || returned == 0 || returned > needed {
        return Err(io::Error::other(format!(
            "native token security-attribute capture changed: status={status:#010x}, bytes={returned}"
        )));
    }
    // SAFETY: initialized owned storage covers the successful byte count;
    // this shared byte borrow ends before storage drops and never escapes.
    let bytes =
        unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), returned as usize) };
    Attributes::from_native(bytes, storage.as_ptr() as u64, std::mem::size_of::<usize>())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid native token security-attribute layout",
            )
        })
}
