//! Fixed host initialization query; no guest values or host pointers escape.
use super::{MAX_SECTION_BYTES, View, invalid, invalid_status};
use std::ffi::c_void;
use std::io;

#[repr(C)]
#[derive(Default)]
struct Region {
    base: usize,
    allocation: usize,
    allocation_protection: u32,
    #[cfg(target_pointer_width = "64")]
    partition_and_padding: u32,
    size: usize,
    state: u32,
    protection: u32,
    kind: u32,
}
const _: [(); if std::mem::size_of::<usize>() == 8 {
    48
} else {
    28
}] = [(); std::mem::size_of::<Region>()];

#[link(name = "ntdll")]
unsafe extern "system" {
    #[cfg(target_pointer_width = "64")]
    fn NtInitializeNlsFiles(
        base: *mut *mut c_void,
        locale: *mut u32,
        casing_size: *mut i64,
        version: *mut u32,
    ) -> i32;
    // The actual x86 NTDLL leaf is RET12, so a 32-bit host must not call
    // the historical four-argument PHNT declaration with stdcall cleanup.
    #[cfg(target_pointer_width = "32")]
    #[link_name = "NtInitializeNlsFiles"]
    fn initialize_x86(base: *mut *mut c_void, locale: *mut u32, casing_size: *mut i64) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualQuery(address: *const c_void, region: *mut Region, length: usize) -> usize;
    fn ReadProcessMemory(
        process: *mut c_void,
        address: *const c_void,
        output: *mut c_void,
        bytes: usize,
        returned: *mut usize,
    ) -> i32;
}

pub(super) fn capture() -> io::Result<(u32, Vec<u8>)> {
    let (mut base, mut locale, mut casing, mut version) = (
        std::ptr::null_mut(),
        0,
        0x5353_5353_5353_5353i64,
        0x7474_7474,
    );
    // SAFETY: installed four-argument NT ABI with exclusive initialized outputs;
    // no caller-selected inputs, guest requests, pointers or callbacks.
    #[cfg(target_pointer_width = "64")]
    let status = unsafe { NtInitializeNlsFiles(&mut base, &mut locale, &mut casing, &mut version) };
    // SAFETY: actual x86 stdcall ABI has three exclusive initialized outputs;
    // no fourth caller stack slot is passed or cleaned twice.
    #[cfg(target_pointer_width = "32")]
    let status = unsafe { initialize_x86(&mut base, &mut locale, &mut casing) };
    if status < 0 {
        return Err(invalid_status("NtInitializeNlsFiles", status));
    }
    let mut view = View(base);
    let mut region = Region::default();
    // SAFETY: opaque native return value queried without dereferencing it;
    // exclusive initialized 48/28-byte native structure, selected by host ABI.
    let got = unsafe { VirtualQuery(base, &mut region, std::mem::size_of::<Region>()) };
    if base.is_null()
        || got != std::mem::size_of::<Region>()
        || region.base != base as usize
        || region.allocation != region.base
        || region.allocation_protection != 2
        || region.state != 0x1000
        || region.protection != 2
        || region.kind != 0x40000
        || region.size == 0
        || region.size > MAX_SECTION_BYTES
        || region.size % 4096 != 0
        || region.base.checked_add(region.size).is_none()
        || casing != 0x5353_5353_5353_5353
        || version != 0x7474_7474
    {
        return Err(invalid(
            "installed NLS initialization mapping/profile changed",
        ));
    }
    let mut bytes = vec![0; region.size];
    let mut returned = 0;
    // SAFETY: live uniquely owned readonly mapping; bounded exclusive initialized
    // destination and SIZE_T output. RPM checks the host extent; no raw host
    // dereference, alias mutation, guest data or unwind crosses this FFI.
    let ok = unsafe {
        ReadProcessMemory(
            (-1isize) as *mut c_void,
            base,
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            &mut returned,
        )
    };
    if ok == 0 || returned != bytes.len() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: consume the successful owned current-process mapping once, after
    // copying all bytes; no address escapes and Drop handles any error path.
    let status = unsafe { super::NtUnmapViewOfSection((-1isize) as *mut c_void, base) };
    if status < 0 {
        return Err(invalid_status("NtUnmapViewOfSection", status));
    }
    view.0 = std::ptr::null_mut();
    Ok((locale, bytes))
}
