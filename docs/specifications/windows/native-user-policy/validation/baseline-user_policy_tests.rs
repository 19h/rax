use super::*;
use crate::user::windows::arch::WinArch;

fn native_sid_text(sid: &[u8]) -> Vec<u16> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn ConvertSidToStringSidW(sid: *const u8, output: *mut *mut u16) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(value: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    struct Owned(*mut u16);
    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: exclusive SDK-allocated string; never exposed to a guest.
            unsafe {
                LocalFree(self.0.cast());
            }
        }
    }
    assert_eq!(crate::user::windows::security::sid(sid), Some(sid));
    let mut raw = std::ptr::null_mut();
    // SAFETY: bounded valid immutable SID and exclusive initialized output.
    assert_ne!(unsafe { ConvertSidToStringSidW(sid.as_ptr(), &mut raw) }, 0);
    assert!(!raw.is_null());
    let text = Owned(raw);
    let mut units = Vec::new();
    for index in 0..256 {
        // SAFETY: successful SDK string is terminated; maximum valid SID text
        // is bounded below256 WCHARs. Only this owned allocation is read.
        let unit = unsafe { *text.0.add(index) };
        if unit == 0 {
            return units;
        }
        units.push(unit);
    }
    panic!("native SID text exceeds its bounded representation")
}

#[test]
fn installed_user_policy_snapshot_matches_primary_sid_and_native_views() {
    let arch = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let runtime = crate::user::windows::native::NativeRuntime::select(arch).unwrap();
    let subject = runtime.wnf_catalog.primary_subject().unwrap();
    let mut relative = native_sid_text(&subject.user);
    relative
        .extend("\\Software\\Policies\\Microsoft\\Windows\\Safer\\CodeIdentifiers".encode_utf16());
    let guest: Vec<_> = "\\Registry\\User\\"
        .encode_utf16()
        .chain(relative.iter().copied())
        .collect();
    relative.push(0);
    for view in [0, 0x100, 0x200] {
        let mut handle = null_mut();
        // SAFETY: fixed policy suffix below independently queried primary SID;
        // query/enumerate only, terminated UTF16 and exclusive native output.
        let status = unsafe {
            RegOpenKeyExW(
                (-2_147_483_645isize) as Hkey,
                relative.as_ptr(),
                0,
                9 | view,
                &mut handle,
            )
        };
        if status == 2 {
            assert!(matches!(
                runtime.registry.lookup(&guest),
                super::super::Lookup::Missing
            ));
        } else {
            assert_eq!(status, 0);
            let native = OwnedKey(handle);
            assert!(runtime.registry.key(&guest).is_some());
            super::tests::check_ifeo(&runtime.registry, &native, &guest, 0);
        }
    }
}
