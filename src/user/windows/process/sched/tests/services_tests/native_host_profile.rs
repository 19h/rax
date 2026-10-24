//! Independent fixed host controls for installed-leaf tests, never guest input.
use std::ffi::c_void;

#[path = "native_host_subject.rs"]
mod subject;

pub(super) fn primary_token_subject() -> crate::user::windows::security::Subject {
    subject::capture()
}

#[repr(C)]
struct Version {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(version: *mut Version) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
    fn GetTokenInformation(
        token: *mut c_void,
        class: u32,
        output: *mut c_void,
        bytes: u32,
        got: *mut u32,
    ) -> i32;
}

pub(super) fn build() -> u32 {
    let mut version = Version {
        size: std::mem::size_of::<Version>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    assert_eq!(version.size, 276);
    // SAFETY: initialized, exclusive OSVERSIONINFOW with its exact ABI size.
    assert_eq!(unsafe { RtlGetVersion(&mut version) }, 0);
    assert!(version.major >= 10);
    version.build
}

pub(super) fn primary_token_session_id() -> u32 {
    let mut token = std::ptr::null_mut();
    // SAFETY: fixed current-process pseudo-handle, TOKEN_QUERY and initialized
    // exclusive output. The acquired token handle is closed before assertions.
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut token) },
        0
    );
    let mut session = 0u32;
    let mut got = 0u32;
    // SAFETY: owned query-only token, TokenSessionId and exclusive DWORD output.
    let queried =
        unsafe { GetTokenInformation(token, 12, (&mut session as *mut u32).cast(), 4, &mut got) };
    // SAFETY: exactly one owned successful OpenProcessToken reference.
    let closed = unsafe { CloseHandle(token) };
    assert_ne!(queried, 0);
    assert_ne!(closed, 0);
    assert_eq!(got, 4);
    session
}
