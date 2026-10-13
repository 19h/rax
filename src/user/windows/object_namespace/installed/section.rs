//! Fixed absolute host opens avoid the measured WoW64 relative-root redirect.
use super::*;
#[repr(C)]
#[derive(Default, PartialEq, Eq)]
struct Basic {
    base: usize,
    attributes: u32,
    size: i64,
}
const _: [(); if std::mem::size_of::<usize>() == 8 {
    24
} else {
    16
}] = [(); std::mem::size_of::<Basic>()];
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtOpenSection(out: *mut Handle, access: u32, attributes: *mut Attributes) -> i32;
    fn NtQuerySection(
        handle: Handle,
        class: u32,
        buffer: *mut c_void,
        length: usize,
        returned: *mut usize,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetProcAddress(module: Handle, name: *const u8) -> *mut c_void;
}
pub(super) fn arm64() -> io::Result<Option<bool>> {
    let module_name: Vec<_> = "kernel32.dll\0".encode_utf16().collect();
    // SAFETY: owned terminated module name; the already-loaded system module is
    // process-lifetime storage. No reference is acquired or released here.
    let module = unsafe { GetModuleHandleW(module_name.as_ptr()) };
    if module.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: live loaded module and static terminated ASCII export name.
    let address = unsafe { GetProcAddress(module, c"IsWow64Process2".as_ptr().cast()) };
    if address.is_null() {
        return Ok(None);
    }
    type QueryMachine = unsafe extern "system" fn(Handle, *mut u16, *mut u16) -> i32;
    // SAFETY: the exact system export's documented BOOL(HANDLE,USHORT*,USHORT*)
    // ABI; code address stays live with kernel32. No guest pointer is involved.
    let query: QueryMachine = unsafe { std::mem::transmute(address) };
    let (mut process, mut native) = (0u16, 0u16);
    // SAFETY: current-process pseudo handle and two exclusive initialized native
    // USHORT outputs, alive through the synchronous call; no retained pointers.
    if unsafe { query((-1isize) as Handle, &mut process, &mut native) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Some(native == 0xAA64))
}
fn open_section(path: &[u16], access: u32, flags: u32) -> io::Result<Result<Owned, u32>> {
    if path.is_empty() || path.len() > MAX_UNITS {
        return Err(invalid("selected section absolute path exceeds bounds"));
    }
    let mut storage = path.to_vec();
    let mut name = Unicode {
        length: (storage.len() * 2) as u16,
        maximum: (storage.len() * 2) as u16,
        buffer: storage.as_mut_ptr(),
    };
    let mut attrs = Attributes {
        length: std::mem::size_of::<Attributes>() as u32,
        root: null_mut(),
        name: &mut name,
        flags,
        security: null_mut(),
        qos: null_mut(),
    };
    let mut handle = null_mut();
    // SAFETY: bounded live UTF-16 storage, initialized native repr(C) structures,
    // exclusive HANDLE output and NULL root. NtOpenSection retains no pointers.
    let s = unsafe { NtOpenSection(&mut handle, access, &mut attrs) };
    if s < 0 {
        return Ok(Err(s as u32));
    }
    if handle.is_null() {
        return Err(invalid(
            "native section open returned a NULL successful handle",
        ));
    }
    Ok(Ok(Owned(handle)))
}
fn info(h: &Owned) -> io::Result<Basic> {
    let mut value = Basic::default();
    let mut returned = 0;
    // SAFETY: live queryable selected section and exclusive initialized native
    // SECTION_BASIC_INFORMATION/SIZE_T outputs with their exact ABI sizes.
    let s = unsafe {
        NtQuerySection(
            h.0,
            0,
            (&mut value as *mut Basic).cast(),
            std::mem::size_of::<Basic>(),
            &mut returned,
        )
    };
    check(s, "NtQuerySection selected installed image")?;
    if returned != std::mem::size_of::<Basic>() || value.base != 0 || value.size <= 0 {
        return Err(invalid(
            "selected native section basic-information layout/base/extent changed",
        ));
    }
    Ok(value)
}
pub(super) fn capture(parent: &[u16], name: &[u16]) -> io::Result<CapturedSection> {
    let path: Vec<_> = parent
        .iter()
        .copied()
        .chain([92])
        .chain(name.iter().copied())
        .collect();
    let h = open_section(&path, 0x0200_0000, 0x40)?
        .map_err(|s| invalid(&format!("selected installed section open: {s:#010x}")))?;
    let maximum_access = basic(&h)?;
    let original = info(&h)?;
    let system_security_status = match open_section(&path, 0x0100_0000, 0x40)? {
        Ok(security) => {
            if basic(&security)? != 0x0100_0000 {
                return Err(invalid("selected section system-security grant changed"));
            }
            security.close()?;
            0
        }
        Err(s @ (STATUS_PRIVILEGE_NOT_HELD | STATUS_ACCESS_DENIED)) => s,
        Err(s) => {
            return Err(invalid(&format!(
                "selected section system-security capture: {s:#010x}"
            )));
        }
    };
    let altered: Vec<_> = path
        .iter()
        .map(|&u| {
            if (65..=90).contains(&u) {
                u + 32
            } else if (97..=122).contains(&u) {
                u - 32
            } else {
                u
            }
        })
        .collect();
    let explicit = open_section(&altered, 0x0200_0000, 0x40)?
        .map_err(|s| invalid(&format!("selected section explicit case probe: {s:#010x}")))?;
    if basic(&explicit)? != maximum_access || info(&explicit)? != original {
        return Err(invalid(
            "selected section explicit case identity/grant changed",
        ));
    }
    explicit.close()?;
    let force_case_insensitive = match open_section(&altered, 0x0200_0000, 0)? {
        Ok(folded) => {
            if basic(&folded)? != maximum_access || info(&folded)? != original {
                return Err(invalid(
                    "selected section folded case identity/grant changed",
                ));
            }
            folded.close()?;
            true
        }
        Err(STATUS_OBJECT_NAME_NOT_FOUND | STATUS_OBJECT_PATH_NOT_FOUND) => false,
        Err(s) => {
            return Err(invalid(&format!(
                "selected section case policy capture: {s:#010x}"
            )));
        }
    };
    if basic(&h)? != maximum_access || info(&h)? != original {
        return Err(invalid(
            "selected section metadata/grant changed during capture",
        ));
    }
    h.close()?;
    let captured = CapturedSection {
        maximum_access,
        system_security_status,
        force_case_insensitive,
        attributes: original.attributes,
        size: original.size as u64,
    };
    if !captured.valid() {
        return Err(invalid(
            "selected installed section is outside the bounded image profile",
        ));
    }
    Ok(captured)
}
