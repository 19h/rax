//! Fixed pointer-free system metadata; guest arguments never enter host queries.

#[cfg(windows)]
pub(super) fn capture() -> Result<u32, String> {
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQuerySystemInformation(
            class: u32,
            output: *mut std::ffi::c_void,
            bytes: u32,
            returned: *mut u32,
        ) -> i32;
    }
    let query = || {
        let mut words = [8u32, 0];
        let mut returned = 0;
        // SAFETY: fixed SystemCodeIntegrityInformation query (observed class103),
        // exclusive aligned eight-byte
        // structure with caller Length8, and exclusive ULONG ReturnLength.
        let status =
            unsafe { NtQuerySystemInformation(103, words.as_mut_ptr().cast(), 8, &mut returned) };
        if status != 0 || words[0] != 8 || returned != 8 {
            return Err(format!(
                "native code integrity query status={status:#010x}, length={}, returned={returned}",
                words[0]
            ));
        }
        Ok(words[1])
    };
    for _ in 0..3 {
        let before = query()?;
        if query()? == before {
            return Ok(before);
        }
    }
    Err("native code integrity changed during bounded capture".into())
}

#[cfg(not(windows))]
pub(super) fn capture() -> Result<u32, String> {
    Err("native Windows code integrity requires a Windows host".into())
}
