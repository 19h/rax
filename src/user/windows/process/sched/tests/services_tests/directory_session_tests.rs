use super::*;
use crate::user::windows::object_namespace::base_named_object_tests::sample;

fn prepare(p: &mut Proc, session: u32) {
    p.object_namespace = sample(session);
    p.object_namespace
        .select_wow_system_root(&"C:\\WINDOWS".encode_utf16().collect::<Vec<_>>())
        .unwrap();
}

fn replay_access(inputs: [(WinArch, &str); 4], session: u32, ordinals: &[u64]) -> usize {
    let mut count = 0;
    for (arch, text) in inputs {
        let (mut process, mut t, b) = setup(arch, 3);
        let p = process.state_mut();
        prepare(p, session);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in rows(text, "open ")
            .into_iter()
            .filter(|r| ordinals.contains(&field(r, "path", 10)))
        {
            let path = rows(text, "path ")
                .into_iter()
                .find(|r| r["ordinal"] == row["path"])
                .unwrap();
            let name = units(path["name"]);
            p.space.put_wunits(b + 128, &name).unwrap();
            p.space.wr(b, &[0; 8]).unwrap();
            descriptor(
                p,
                b + 64,
                b + 128,
                (name.len() * 2) as u16,
                (name.len() * 2 + 2) as u16,
            );
            attributes(
                p,
                b + 16,
                b + 64,
                0,
                field(&row, "flags", 16) as u32,
                if arch.is64() { 48 } else { 24 },
            );
            assert_eq!(
                call(p, &mut t, [b, field(&row, "access", 16), b + 16]),
                field(&row, "status", 16) as u32
            );
            if field(&row, "status", 16) == 0 {
                let handle = p.space.ptr(b, arch.ptr_size()).unwrap();
                let basic = hex(row["basic"]);
                assert_eq!(
                    p.objects.access(handle),
                    Some(u32::from_le_bytes(basic[4..8].try_into().unwrap()))
                );
                p.objects.close(handle).unwrap();
            } else {
                assert_eq!(p.space.bytes(b, 8).unwrap(), [0; 8]);
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            count += 1;
        }
    }
    count
}

#[test]
fn native_base_named_object_open_replays_all_256_native_access_rows() {
    let inputs = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-directories-v1-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-directories-v1-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-directories-v1-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-directories-v1-x86-laa.log"
            ),
        ),
    ];
    assert_eq!(replay_access(inputs, 0, &[0]), 256);
}

#[test]
fn native_base_named_object_open_replays_all_1024_native_session_one_access_rows() {
    let inputs = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-one-directories-v1-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-one-directories-v1-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-one-directories-v1-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-base-named-objects/session-one-directories-v1-x86-laa.log"
            ),
        ),
    ];
    assert_eq!(replay_access(inputs, 1, &[0, 1, 2, 3]), 1024);
}

#[test]
fn native_base_named_object_nested_open_preserves_relative_identity_and_handle_lifetime() {
    for arch in [WinArch::Arm64, WinArch::X64, WinArch::X86] {
        for session in [1, 42, u32::MAX] {
            let (mut process, mut t, b) = setup(arch, 3);
            let p = process.state_mut();
            prepare(p, session);
            let baseline = (p.objects.handle_count(), p.objects.iter().count());
            let parent = opened(p, &mut t, b, &format!("\\Sessions\\{session}"), 3);
            let absolute = opened(
                p,
                &mut t,
                b,
                &format!("\\Sessions\\{session}\\BaseNamedObjects"),
                0xF,
            );
            let name: Vec<_> = "BaseNamedObjects".encode_utf16().collect();
            p.space.put_wunits(b + 128, &name).unwrap();
            descriptor(
                p,
                b + 64,
                b + 128,
                (name.len() * 2) as u16,
                (name.len() * 2 + 2) as u16,
            );
            attributes(
                p,
                b + 16,
                b + 64,
                parent,
                0x40,
                if arch.is64() { 48 } else { 24 },
            );
            assert_eq!(call(p, &mut t, [b, 0xF, b + 16]), 0);
            let relative = p.space.ptr(b, arch.ptr_size()).unwrap();
            match (p.objects.get(absolute), p.objects.get(relative)) {
                (Some(Object::Directory(a)), Some(Object::Directory(b))) => {
                    assert!(Arc::ptr_eq(a, b))
                }
                pair => panic!("unexpected directory pair {pair:?}"),
            }
            p.objects.close(parent).unwrap();
            p.objects.close(absolute).unwrap();
            assert_eq!(p.objects.get(relative).unwrap().type_name(), "Directory");
            p.objects.close(relative).unwrap();
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
        }
    }
}

#[cfg(windows)]
pub(super) fn native_reference(path: &[u16], access: u32) -> u32 {
    use std::ffi::c_void;
    #[repr(C)]
    struct Unicode {
        length: u16,
        maximum: u16,
        buffer: *mut u16,
    }
    #[repr(C)]
    struct Attributes {
        length: u32,
        root: *mut c_void,
        name: *mut Unicode,
        flags: u32,
        security: *mut c_void,
        qos: *mut c_void,
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtOpenDirectoryObject(
            handle: *mut *mut c_void,
            access: u32,
            attrs: *mut Attributes,
        ) -> i32;
        fn NtClose(handle: *mut c_void) -> i32;
    }
    let mut storage = path.to_vec();
    let mut name = Unicode {
        length: u16::try_from(path.len() * 2).unwrap(),
        maximum: u16::try_from(path.len() * 2).unwrap(),
        buffer: storage.as_mut_ptr(),
    };
    let mut attributes = Attributes {
        length: std::mem::size_of::<Attributes>() as u32,
        root: std::ptr::null_mut(),
        name: &mut name,
        flags: 0x40,
        security: std::ptr::null_mut(),
        qos: std::ptr::null_mut(),
    };
    let mut handle = std::ptr::null_mut();
    // SAFETY: bounded UTF-16 and initialized native-layout records remain live;
    // the private returned handle is closed before returning and never enters a guest.
    let status = unsafe { NtOpenDirectoryObject(&mut handle, access, &mut attributes) } as u32;
    if status == 0 {
        assert!(!handle.is_null());
        // SAFETY: the successful call above supplied this uniquely owned handle.
        assert_eq!(unsafe { NtClose(handle) }, 0);
    }
    status
}

#[cfg(windows)]
#[test]
fn native_base_named_object_installed_leaf_matches_independent_current_token_open() {
    use crate::user::windows::{WindowsConfig, WindowsProcess};
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    let profiles = if host == WinArch::X86 {
        vec![host]
    } else {
        vec![host, WinArch::X86]
    };
    for arch in profiles {
        let image: &[u8] = match arch {
            WinArch::Arm64 => include_bytes!(
                "../../../../../../../tests/fixtures/user/windows/bin/arm64/smoke.exe"
            ),
            WinArch::X64 => {
                include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x64/smoke.exe")
            }
            WinArch::X86 => {
                include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
            }
        };
        let mut cfg =
            WindowsConfig::embedded("C:\\base-directory.exe", vec![], vec![], 4096).unwrap();
        cfg.native_libraries = true;
        cfg.arena_bytes = 256 << 20;
        let mut process = WindowsProcess::spawn_image(cfg, image.to_vec()).unwrap();
        let p = process.state_mut();
        let session = p.objects.primary_token_session_id().unwrap();
        let path = if session == 0 {
            "\\BaseNamedObjects".to_owned()
        } else {
            format!("\\Sessions\\{session}\\BaseNamedObjects")
        };
        let units: Vec<_> = path.encode_utf16().collect();
        let expected = native_reference(&units, 0xF);
        let tid = *p.threads.keys().next().unwrap();
        let mut thread = p.threads.remove(&tid).unwrap();
        let base =
            p.vm.allocate(
                None,
                3 * PAGE_SIZE,
                mem::RESERVE | mem::COMMIT,
                prot::READWRITE,
            )
            .unwrap()
            .0;
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        p.space.put_wunits(base + 128, &units).unwrap();
        descriptor(
            p,
            base + 64,
            base + 128,
            (units.len() * 2) as u16,
            (units.len() * 2 + 2) as u16,
        );
        attributes(
            p,
            base + 16,
            base + 64,
            0,
            0x40,
            if arch.is64() { 48 } else { 24 },
        );
        super::leaf::run_leaf(p, &mut thread, [base, 0xF, base + 16], expected);
        if expected == 0 {
            let handle = p.space.ptr(base, arch.ptr_size()).unwrap();
            assert_eq!(p.objects.get(handle).unwrap().type_name(), "Directory");
            p.objects.close(handle).unwrap();
        }
        assert_eq!(
            (p.objects.handle_count(), p.objects.iter().count()),
            baseline
        );
    }
}
