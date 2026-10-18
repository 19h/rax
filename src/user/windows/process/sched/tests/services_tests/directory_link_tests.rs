use super::*;
use crate::user::windows::object_namespace::reparse_tests::sample;

#[test]
fn native_object_reparse_open_replays_all_2432_native_chain_and_flag_rows() {
    let inputs = [
        (
            WinArch::Arm64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-object-reparse/native-object-reparse-v2-arm64.log"
            ),
        ),
        (
            WinArch::X64,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-object-reparse/native-object-reparse-v2-x64.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-object-reparse/native-object-reparse-v2-x86.log"
            ),
        ),
        (
            WinArch::X86,
            include_str!(
                "../../../../../../../docs/specifications/windows/native-object-reparse/native-object-reparse-v2-x86-laa.log"
            ),
        ),
    ];
    let mut checked = 0;
    for (arch, text) in inputs {
        let (mut process, mut t, b) = setup(arch, 8);
        let p = process.state_mut();
        p.object_namespace = sample(0);
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for row in rows(text, "reparse ") {
            let count = field(&row, "links", 10) as usize;
            let mut name = "\\BaseNamedObjects".to_owned();
            for index in 0..count {
                name.push_str(if index % 2 == 0 {
                    "\\Local"
                } else {
                    "\\Global"
                });
            }
            let units: Vec<_> = name.encode_utf16().collect();
            p.space.put_wunits(b + 128, &units).unwrap();
            p.space.wr(b, &[0; 8]).unwrap();
            descriptor(
                p,
                b + 64,
                b + 128,
                (units.len() * 2) as u16,
                (units.len() * 2) as u16,
            );
            attributes(
                p,
                b + 16,
                b + 64,
                0,
                field(&row, "flags", 16) as u32,
                if arch.is64() { 48 } else { 24 },
            );
            let expected = field(&row, "status", 16) as u32;
            assert_eq!(
                call(p, &mut t, [b, 3, b + 16]),
                expected,
                "{arch:?} row={row:?}"
            );
            if expected == STATUS_SUCCESS {
                let handle = p.space.ptr(b, arch.ptr_size()).unwrap();
                let Some(Object::Directory(directory)) = p.objects.get(handle) else {
                    panic!("not a selected directory");
                };
                assert_eq!(
                    directory.path,
                    "\\BaseNamedObjects".encode_utf16().collect::<Vec<_>>()
                );
                p.objects.close(handle).unwrap();
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 2432);
}

#[cfg(windows)]
#[test]
fn native_object_reparse_installed_leaves_match_independent_native_directory_opens() {
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
            WindowsConfig::embedded("C:\\object-links.exe", vec![], vec![], 4096).unwrap();
        cfg.native_libraries = true;
        cfg.arena_bytes = 256 << 20;
        let mut process = WindowsProcess::spawn_image(cfg, image.to_vec()).unwrap();
        let p = process.state_mut();
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
        for tail in [
            "\\Local".to_owned(),
            "\\Global".to_owned(),
            "\\Local\\Global".to_owned(),
            "\\Local".repeat(63),
            "\\Local".repeat(64),
        ] {
            let path = format!("\\BaseNamedObjects{tail}");
            let units: Vec<_> = path.encode_utf16().collect();
            let expected = super::session_directories::native_reference(&units, 3);
            p.space.put_wunits(base + 128, &units).unwrap();
            p.space.wr(base, &[0; 8]).unwrap();
            descriptor(
                p,
                base + 64,
                base + 128,
                (units.len() * 2) as u16,
                (units.len() * 2) as u16,
            );
            attributes(
                p,
                base + 16,
                base + 64,
                0,
                0x40,
                if arch.is64() { 48 } else { 24 },
            );
            super::leaf::run_leaf(p, &mut thread, [base, 3, base + 16], expected);
            if expected == STATUS_SUCCESS {
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
}
