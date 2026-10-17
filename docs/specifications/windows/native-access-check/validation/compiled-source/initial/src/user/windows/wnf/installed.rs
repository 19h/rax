//! Query-only token acquisition during installed-runtime selection.
use super::catalog::{Group, Subject, sid};
use crate::user::windows::security::Privilege;
use std::ffi::c_void;
use std::io;
use std::ptr::null_mut;

type Handle = *mut c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, out: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        buffer: *mut u8,
        bytes: u32,
        returned: *mut u32,
    ) -> i32;
}
struct Token(Handle);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: exclusive query-only token handle from OpenProcessToken.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn information(token: &Token, class: u32) -> io::Result<Vec<u8>> {
    let mut bytes = 0;
    // SAFETY: live token, NULL/zero query buffer, exclusive DWORD output.
    let ok = unsafe { GetTokenInformation(token.0, class, null_mut(), 0, &mut bytes) };
    if ok != 0
        || io::Error::last_os_error().raw_os_error() != Some(122)
        || bytes == 0
        || bytes > 65536
    {
        return Err(invalid(
            "WNF token information size is unavailable or exceeds 64 KiB",
        ));
    }
    let mut buffer = vec![0; bytes as usize];
    // SAFETY: exclusive initialized buffer of exact byte capacity; returned
    // pointers are validated within this buffer and copied before it drops.
    if unsafe { GetTokenInformation(token.0, class, buffer.as_mut_ptr(), bytes, &mut bytes) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if bytes as usize > buffer.len() {
        return Err(invalid("WNF token output exceeds capacity"));
    }
    buffer.truncate(bytes as usize);
    Ok(buffer)
}
fn group(buffer: &[u8], at: usize) -> io::Result<Group> {
    let width = std::mem::size_of::<usize>();
    let address = usize::from_ne_bytes(
        buffer
            .get(at..at + width)
            .ok_or_else(|| invalid("truncated WNF token SID pointer"))?
            .try_into()
            .unwrap(),
    );
    let attributes = u32::from_ne_bytes(
        buffer
            .get(at + width..at + width + 4)
            .ok_or_else(|| invalid("truncated WNF token SID attributes"))?
            .try_into()
            .unwrap(),
    );
    let offset = address
        .checked_sub(buffer.as_ptr() as usize)
        .ok_or_else(|| invalid("WNF token SID precedes its buffer"))?;
    let value = sid(buffer
        .get(offset..)
        .ok_or_else(|| invalid("WNF token SID escapes its buffer"))?)
    .ok_or_else(|| invalid("malformed WNF token SID"))?;
    Ok(Group {
        sid: value.to_vec(),
        attributes,
    })
}
fn groups(buffer: &[u8]) -> io::Result<Vec<Group>> {
    let count = u32::from_ne_bytes(
        buffer
            .get(..4)
            .ok_or_else(|| invalid("truncated WNF token group count"))?
            .try_into()
            .unwrap(),
    ) as usize;
    let width = std::mem::size_of::<usize>();
    let start = 4usize.next_multiple_of(width);
    let stride = (width + 4).next_multiple_of(width);
    if count > 4096
        || start
            .checked_add(count * stride)
            .is_none_or(|end| end > buffer.len())
    {
        return Err(invalid("WNF token groups exceed their bounded buffer"));
    }
    (0..count)
        .map(|i| group(buffer, start + i * stride))
        .collect()
}
fn capture(token: &Token) -> io::Result<Subject> {
    let user = group(&information(token, 1)?, 0)?.sid;
    let enabled_groups = groups(&information(token, 2)?)?;
    let restricted = groups(&information(token, 11)?)?;
    let raw_privileges = information(token, 3)?;
    let count = raw_privileges
        .get(..4)
        .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()) as usize)
        .ok_or_else(|| invalid("truncated token privilege count"))?;
    if count > 4096 || raw_privileges.len() != 4 + 12 * count {
        return Err(invalid("token privileges exceed their bounded buffer"));
    }
    let privileges = raw_privileges[4..]
        .chunks_exact(12)
        .map(|record| Privilege {
            luid: u64::from_ne_bytes(record[..8].try_into().unwrap()),
            attributes: u32::from_ne_bytes(record[8..].try_into().unwrap()),
        })
        .collect();
    let flags = information(token, 29)?;
    if flags.len() != 4 {
        return Err(invalid("WNF AppContainer flag is not a DWORD"));
    }
    Ok(Subject {
        user,
        groups: enabled_groups,
        restricted,
        app_container: u32::from_ne_bytes(flags.try_into().unwrap()) != 0,
        privileges,
    })
}
pub(super) fn subject() -> io::Result<Subject> {
    let mut handle = null_mut();
    // SAFETY: pseudo current-process handle; TOKEN_QUERY=8 only; exclusive
    // output becomes an RAII owner and never enters guest state.
    if unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut handle) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Token(handle);
    for _ in 0..3 {
        let first = capture(&token)?;
        if first == capture(&token)? {
            return Ok(first);
        }
    }
    Err(invalid(
        "native WNF subject changed during bounded acquisition",
    ))
}
