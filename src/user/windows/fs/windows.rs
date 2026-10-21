//! One handle identity contract for native NT and Win32-emulated file objects.
use super::FileIdentity;
use std::ffi::c_void;
use std::fs::File;
use std::os::windows::io::AsRawHandle;

#[repr(C)]
struct FileId {
    volume: u64,
    id: [u8; 16],
}

const _: [(); 24] = [(); std::mem::size_of::<FileId>()];

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandleEx(
        file: *mut c_void,
        class: i32,
        information: *mut c_void,
        bytes: u32,
    ) -> i32;
}

pub(super) fn identity(file: &File) -> std::io::Result<FileIdentity> {
    let mut id = FileId {
        volume: 0,
        id: [0; 16],
    };
    // SAFETY: the borrowed File owns a live handle. FileIdInfo writes exactly
    // this initialized 24-byte SDK structure and retains no pointer. Native
    // read/execute and synchronization-only grants were independently exercised.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            18,
            (&mut id as *mut FileId).cast(),
            24,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(FileIdentity::Windows(id.volume, id.id))
}
