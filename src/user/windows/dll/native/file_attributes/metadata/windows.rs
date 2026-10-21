//! Host-owned native arguments preserve NTFS timestamps and allocation size.
//! Neither guest addresses, guest handles nor unvalidated flags cross this FFI.

use super::super::super::named_object::{Checked, status};
use super::Information;
use crate::user::windows::hle::Ctx;
use std::ffi::c_void;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

#[repr(C)]
struct Unicode {
    length: u16,
    maximum: u16,
    buffer: *const u16,
}

#[repr(C)]
struct Attributes {
    length: u32,
    root: *mut c_void,
    name: *const Unicode,
    flags: u32,
    security: *mut c_void,
    qos: *mut c_void,
}

#[repr(C, align(8))]
#[derive(Default)]
struct Network {
    times: [u64; 4],
    allocation: u64,
    size: u64,
    attributes: u32,
    padding: u32,
}

const _: [(); 56] = [(); std::mem::size_of::<Network>()];

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryFullAttributesFile(attributes: *const Attributes, info: *mut Network) -> i32;

}

pub(super) fn query(
    c: &Ctx,
    path: &Path,
    file: Option<&File>,
    relative: Option<&Path>,
    no_reparse: bool,
) -> Checked<Information> {
    let mut info = Network::default();
    let result = if let Some(file) = file {
        let units: Vec<u16> =
            relative.map_or_else(Vec::new, |name| name.as_os_str().encode_wide().collect());
        if units.len() > 32766 || units.contains(&0) {
            return Err(c
                .unsupported("retained NT metadata name is not representable")
                .into());
        }
        let unicode = Unicode {
            length: (units.len() * 2) as u16,
            maximum: (units.len() * 2) as u16,
            buffer: units.as_ptr(),
        };
        let attrs = Attributes {
            length: std::mem::size_of::<Attributes>() as u32,
            root: file.as_raw_handle(),
            name: &unicode,
            flags: 0x40 | if no_reparse { 0x1000 } else { 0 },
            security: std::ptr::null_mut(),
            qos: std::ptr::null_mut(),
        };
        // SAFETY: the borrowed owned handle and every host-local argument stay
        // live through the synchronous query. An empty counted name queries the
        // retained object without requiring READ_ATTRIBUTES on its open grant.
        unsafe { NtQueryFullAttributesFile(&attrs, &mut info) }
    } else {
        let absolute = std::path::absolute(path)
            .map_err(|error| c.unsupported(format!("absolute host metadata path: {error}")))?;
        let raw: Vec<u16> = absolute.as_os_str().encode_wide().collect();
        let mut native: Vec<u16> = "\\??\\".encode_utf16().collect();
        if raw.starts_with(&[92, 92, 63, 92]) || raw.starts_with(&[92, 63, 63, 92]) {
            native.extend_from_slice(&raw[4..]);
        } else if raw.starts_with(&[92, 92]) {
            native.extend("UNC\\".encode_utf16());
            native.extend_from_slice(&raw[2..]);
        } else if raw.len() >= 3 && raw[1] == 58 && raw[2] == 92 {
            native.extend_from_slice(&raw);
        } else {
            return Err(c
                .unsupported("host metadata path has no absolute NT DOS spelling")
                .into());
        }
        if native.contains(&0) || native.len() > 32766 {
            return Err(c
                .unsupported("host metadata path exceeds counted NT name bounds")
                .into());
        }
        let unicode = Unicode {
            length: (native.len() * 2) as u16,
            maximum: (native.len() * 2) as u16,
            buffer: native.as_ptr(),
        };
        let attrs = Attributes {
            length: std::mem::size_of::<Attributes>() as u32,
            root: std::ptr::null_mut(),
            name: &unicode,
            flags: 0x40 | if no_reparse { 0x1000 } else { 0 },
            security: std::ptr::null_mut(),
            qos: std::ptr::null_mut(),
        };
        // SAFETY: every pointer addresses a host-owned live initialized buffer,
        // whose length/layout is exact. The call is synchronous, read-only and
        // retains no pointer. The path has already passed guest policy checks.
        unsafe { NtQueryFullAttributesFile(&attrs, &mut info) }
    };
    if result != 0 {
        return status(result as u32);
    }
    Ok(Information {
        times: info.times,
        allocation: info.allocation,
        size: info.size,
        attributes: info.attributes,
    })
}
