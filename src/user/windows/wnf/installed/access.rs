//! Bounded read-only capture of the native primary token's opening ceiling.
use super::{Handle, Token, invalid};
use crate::user::windows::nt::status::{STATUS_ACCESS_DENIED, STATUS_PRIVILEGE_NOT_HELD};
use crate::user::windows::security::token::Access;
use std::ffi::c_void;
use std::io;
use std::ptr::null_mut;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtOpenProcessToken(process: Handle, desired: u32, output: *mut Handle) -> i32;
    fn NtQueryObject(
        handle: Handle,
        class: u32,
        output: *mut c_void,
        bytes: u32,
        returned: *mut u32,
    ) -> i32;
}

fn open(desired: u32) -> io::Result<Result<Token, u32>> {
    let mut handle = null_mut();
    // SAFETY: the real current-process pseudohandle and exclusive initialized
    // HANDLE output. The result becomes a private RAII owner, never guest data.
    let status = unsafe { NtOpenProcessToken(super::GetCurrentProcess(), desired, &mut handle) };
    if status < 0 {
        if !handle.is_null() {
            let _unexpected = Token(handle);
            return Err(invalid("failed primary-token capture returned a handle"));
        }
        return Ok(Err(status as u32));
    }
    if handle.is_null() {
        return Err(invalid("primary-token capture returned no handle"));
    }
    Ok(Ok(Token(handle)))
}

fn granted(token: &Token) -> io::Result<u32> {
    let mut data = [0u32; 14];
    let mut returned = 0;
    // SAFETY: live private token, initialized 56-byte OBJECT_BASIC_INFORMATION
    // and exclusive ULONG output. Only the fixed GrantedAccess field is kept.
    let status = unsafe { NtQueryObject(token.0, 0, data.as_mut_ptr().cast(), 56, &mut returned) };
    if status < 0 || returned != 56 {
        return Err(invalid("primary-token basic information is unavailable"));
    }
    Ok(data[1])
}

pub(super) fn select() -> io::Result<(Token, Access)> {
    let token = open(0x0200_0000)?.map_err(|status| {
        io::Error::other(format!(
            "primary-token maximum access failed: {status:#010x}"
        ))
    })?;
    let maximum = granted(&token)?;
    if maximum & 8 == 0 {
        return Err(invalid(
            "primary-token ceiling does not include TOKEN_QUERY",
        ));
    }
    let security_status = match open(0x0100_0000)? {
        Ok(security) => {
            if granted(&security)? != 0x0100_0000 {
                return Err(invalid("primary-token system-security grant changed"));
            }
            0
        }
        Err(status @ (STATUS_ACCESS_DENIED | STATUS_PRIVILEGE_NOT_HELD)) => status,
        Err(_) => {
            return Err(invalid(
                "primary-token system-security capture is unavailable",
            ));
        }
    };
    let access = Access::new(maximum, security_status)
        .ok_or_else(|| invalid("primary-token opening profile is outside the supported mask"))?;
    if granted(&token)? != maximum {
        return Err(invalid("primary-token grant changed during capture"));
    }
    Ok((token, access))
}
