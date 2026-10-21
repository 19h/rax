//! Private buffers and the retained host file; no guest pointer or handle enters NT.
use crate::user::windows::fs::FileIdentity;
use crate::user::windows::objects::FileObj;
use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;

#[repr(C)]
struct Io {
    status: usize,
    information: usize,
}
#[repr(C)]
struct Overlapped {
    io: Io,
    offset: [u32; 2],
    event: *mut c_void,
}
#[repr(C, align(8))]
struct Buffer([u8; 8]);
const _: () = assert!(std::mem::offset_of!(Overlapped, offset) == 2 * std::mem::size_of::<usize>());

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryVolumeInformationFile(
        file: *mut c_void,
        io: *mut Io,
        output: *mut c_void,
        length: u32,
        class: u32,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetOverlappedResult(
        file: *mut c_void,
        io: *mut Overlapped,
        bytes: *mut u32,
        wait: i32,
    ) -> i32;
    fn GetLastError() -> u32;
}

pub(super) fn device(file: &FileObj) -> Result<(u32, u32), String> {
    let reopened;
    let handle = if let Some(handle) = &file.host {
        handle
    } else {
        // Legacy metadata-only objects may lack a retained File. A new
        // metadata handle is admitted only when its identity still matches.
        reopened = std::fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(7)
            .custom_flags(0x02000000)
            .open(&file.host_path)
            .map_err(|e| format!("metadata-only file open: {e}"))?;
        if FileIdentity::retained(&reopened).map_err(|e| format!("metadata identity: {e}"))?
            != file.lifetime.identity
        {
            return Err("metadata-only handle pathname was replaced".into());
        }
        &reopened
    };
    // The IOSB is the OVERLAPPED prefix. Canonical STATUS_PENDING permits
    // GetOverlappedResult(FALSE) to observe completion without SYNCHRONIZE
    // access or waiting on an unrelated file operation. UnsafeCell permits
    // kernel mutation; both boxes and the borrowed handle outlive completion.
    let io = Box::new(UnsafeCell::new(Overlapped {
        io: Io {
            status: 0x103,
            information: 0,
        },
        offset: [0; 2],
        event: std::ptr::null_mut(),
    }));
    let output = Box::new(UnsafeCell::new(Buffer([0u8; 8])));
    // SAFETY: pointers name owned, aligned, stable allocations of the native
    // ABI sizes. NT receives only the retained host handle and fixed class4.
    let status = unsafe {
        NtQueryVolumeInformationFile(
            handle.as_raw_handle(),
            io.get().cast(),
            output.get().cast(),
            8,
            4,
        )
    } as u32;
    if status == 0x103 {
        loop {
            let mut bytes = 0;
            // SAFETY: the complete OVERLAPPED and output allocations remain
            // live until this API reports terminal completion.
            if unsafe { GetOverlappedResult(handle.as_raw_handle(), io.get(), &mut bytes, 0) } != 0
            {
                break;
            }
            let error = unsafe { GetLastError() };
            if error != 996 {
                return Err(format!("native volume completion error {error}"));
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    } else if status != 0 {
        return Err(format!("native volume status {status:#010x}"));
    }
    // SAFETY: successful synchronous return or terminal completion establishes
    // that the kernel no longer accesses the private allocations.
    let (result, data) = unsafe { (&*io.get(), (*output.get()).0) };
    if result.io.status as u32 != 0 || result.io.information != 8 {
        return Err("unexpected native volume completion payload".into());
    }
    Ok((
        u32::from_le_bytes(data[..4].try_into().unwrap()),
        u32::from_le_bytes(data[4..].try_into().unwrap()),
    ))
}
