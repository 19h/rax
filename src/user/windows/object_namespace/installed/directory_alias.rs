//! Resolve only the current session's selected BaseNamedObjects identity.
//! No arbitrary link target is opened, and no native handle reaches the guest.
use std::ffi::c_void;
use std::io;

use super::{Handle, Owned, basic, open, target};
use crate::user::windows::object_namespace::{SelectedDirectory, invalid, is};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetProcAddress(module: Handle, name: *const u8) -> *mut c_void;
}

type Compare = unsafe extern "system" fn(Handle, Handle) -> i32;

fn compare_function() -> io::Result<Compare> {
    let name: Vec<u16> = "ntdll.dll".encode_utf16().chain([0]).collect();
    // SAFETY: live NUL-terminated UTF-16 module name; the returned borrowed
    // module is already process-owned and remains loaded throughout selection.
    let module = unsafe { GetModuleHandleW(name.as_ptr()) };
    if module.is_null() {
        return Err(invalid(
            "selected directory alias cannot locate native NTDLL",
        ));
    }
    // SAFETY: live native module and static NUL-terminated export name; no
    // caller pointer is retained. This does not make the export a static import.
    let address = unsafe { GetProcAddress(module, c"NtCompareObjects".as_ptr().cast()) };
    if address.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "selected directory alias requires native NtCompareObjects identity evidence",
        ));
    }
    // SAFETY: this exact NTDLL export has NTSTATUS(HANDLE,HANDLE), with the
    // current process's system ABI. It remains executable while NTDLL is loaded;
    // no Rust data representation or unwinding crosses the native boundary.
    Ok(unsafe { std::mem::transmute::<*mut c_void, Compare>(address) })
}

pub(super) fn verified(
    selected: &[SelectedDirectory],
    handles: &[(Vec<u16>, Owned)],
    session: u32,
) -> io::Result<Vec<(Vec<u16>, Vec<u16>)>> {
    if session == 0 {
        return Ok(Vec::new());
    }
    let parent_path: Vec<u16> = format!("\\Sessions\\{session}").encode_utf16().collect();
    let parent = selected
        .iter()
        .find(|directory| directory.path == parent_path)
        .ok_or_else(|| invalid("selected session alias parent capture is absent"))?;
    let Some(entry) = parent
        .entries
        .iter()
        .find(|entry| is(&entry.name, "BaseNamedObjects") && is(&entry.kind, "SymbolicLink"))
    else {
        return Ok(Vec::new());
    };
    let captured_target = entry
        .target
        .as_ref()
        .ok_or_else(|| invalid("selected session alias target was not captured"))?;
    let access = entry
        .link_access
        .ok_or_else(|| invalid("selected session alias authorization was not captured"))?;
    let source: Vec<u16> = format!("\\Sessions\\{session}\\BaseNamedObjects")
        .encode_utf16()
        .collect();
    let destination: Vec<u16> = "\\BaseNamedObjects".encode_utf16().collect();
    let handle = |path: &[u16]| {
        handles
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, handle)| handle)
            .ok_or_else(|| invalid("selected alias identity handle is absent"))
    };
    let original = handle(&source)?;
    let resolved = handle(&destination)?;
    let parent = handle(&parent_path)?;
    let compare = compare_function()?;
    // SAFETY: both handles are exclusive owned successful directory opens from
    // the same bounded acquisition. Neither can close until this call returns.
    let status = unsafe { compare(original.0, resolved.0) };
    if status != 0 {
        return Err(invalid(&format!(
            "selected session alias has no fixed target identity: source={:?} captured_target={:?} candidate={:?} comparison={:#010x}",
            String::from_utf16_lossy(&source),
            String::from_utf16_lossy(captured_target),
            String::from_utf16_lossy(&destination),
            status as u32,
        )));
    }
    let refreshed = open(&entry.name, parent.0, 0x0200_0000, false, 0x40)?.map_err(|status| {
        invalid(&format!(
            "selected session alias verification open: {status:#010x}"
        ))
    })?;
    // SAFETY: original remains live and refreshed is another exclusive owned
    // open under the captured parent. Comparison retains neither handle.
    let status = unsafe { compare(original.0, refreshed.0) };
    if status != 0 || basic(original)? != basic(&refreshed)? {
        return Err(invalid(
            "selected session alias identity/grant changed during capture",
        ));
    }
    refreshed.close()?;
    let (latest_target, latest_access) = target(parent, &entry.name)?;
    if latest_target != *captured_target
        || latest_access.maximum_access != access.maximum_access
        || latest_access.system_security_status != access.system_security_status
    {
        return Err(invalid(
            "selected session alias link metadata changed during capture",
        ));
    }
    Ok(vec![(source, destination)])
}

#[cfg(test)]
mod tests;
