//! Read-only selection through fixed WNF metadata, before guest execution.
use super::query::{Definition, Queries, capture};
use std::ffi::{c_char, c_void};
use std::io;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn GetCurrentProcessId() -> u32;
    fn ProcessIdToSessionId(process: u32, session: *mut u32) -> i32;
}
type Query = unsafe extern "system" fn(
    *const u64,
    *const u8,
    *const c_void,
    *mut u32,
    *mut u8,
    *mut u32,
) -> i32;

pub(super) fn snapshot(definitions: Vec<Definition>) -> io::Result<Queries> {
    let module_name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
    // SAFETY: fixed terminated library/export names. Neither handle is owned
    // or retained; the process's installed NTDLL remains loaded during capture.
    let address = unsafe {
        let module = GetModuleHandleW(module_name.as_ptr());
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }
        GetProcAddress(module, c"NtQueryWnfStateData".as_ptr())
    };
    if address.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the fixed NTDLL export uses the six-argument NTAPI prototype.
    let query: Query = unsafe { std::mem::transmute(address) };
    let mut session = 0;
    // SAFETY: current process identity and exclusive DWORD output.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    capture(definitions, session, |definition, bytes| {
        let mut stamp = 0;
        let mut length = bytes.len() as u32;
        let kind = definition
            .type_id
            .as_ref()
            .map_or(std::ptr::null(), |id| id.as_ptr());
        let output = if bytes.is_empty() {
            std::ptr::null_mut()
        } else {
            bytes.as_mut_ptr()
        };
        // SAFETY: all pointers refer to local owned metadata/output for this
        // fixed provisioned name. ExplicitScope is NULL. No guest inputs or
        // pointers enter acquisition, and native code cannot retain the slices.
        let status = unsafe {
            query(
                &definition.name,
                kind,
                std::ptr::null(),
                &mut stamp,
                output,
                &mut length,
            )
        };
        (status as u32, stamp, length as usize)
    })
}
