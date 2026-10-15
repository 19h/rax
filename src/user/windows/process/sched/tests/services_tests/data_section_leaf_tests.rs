//! Actual installed NT section leaves; copied host data never becomes a host pointer.
use super::*;

#[test]
fn native_data_section_installed_leaves_open_map_protect_close_and_unmap() {
    let host = if cfg!(target_arch = "aarch64") {
        WinArch::Arm64
    } else if cfg!(target_arch = "x86_64") {
        WinArch::X64
    } else {
        WinArch::X86
    };
    for arch in if host == WinArch::X86 {
        vec![host]
    } else {
        vec![host, WinArch::X86]
    } {
        let bytes: &[u8] = match arch {
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
        let mut config =
            WindowsConfig::embedded("C:\\native-data-section.exe", vec![], vec![], 4096).unwrap();
        config.native_libraries = true;
        config.arena_bytes = 256 << 20;
        let mut process = WindowsProcess::spawn_image(config, bytes.to_vec()).unwrap();
        let p = process.state_mut();
        let tid = *p.threads.keys().next().unwrap();
        let mut thread = p.threads.remove(&tid).unwrap();
        let b = scratch(p);
        let name: Vec<_> = "\\Windows\\SharedSection"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        p.space.wr(b + 1024, &name).unwrap();
        p.space.w16(b + 256, name.len() as u16).unwrap();
        p.space.w16(b + 258, name.len() as u16).unwrap();
        ptr(p, b + 256 + if arch.is64() { 8 } else { 4 }, b + 1024);
        p.space
            .w32(b + 512, if arch.is64() { 48 } else { 24 })
            .unwrap();
        ptr(p, b + 512 + if arch.is64() { 16 } else { 8 }, b + 256);
        p.space
            .w32(b + 512 + if arch.is64() { 24 } else { 12 }, 0x40)
            .unwrap();
        let selected = match p
            .object_namespace
            .lookup_installed_section(
                None,
                &"\\Windows\\SharedSection"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
                0x40,
                arch == WinArch::X86,
            )
            .unwrap()
        {
            crate::user::windows::object_namespace::SelectedSection::Data(section) => section,
            _ => panic!("installed SharedSection must be data"),
        };
        assert_eq!(
            leaf(p, &mut thread, "NtOpenSection", &[b, 5, b + 512]),
            STATUS_ACCESS_DENIED
        );
        assert_eq!(get(p, b), 0);
        assert_eq!(
            leaf(p, &mut thread, "NtOpenSection", &[b, 4, b + 512]),
            STATUS_SUCCESS
        );
        let handle = get(p, b);
        ptr(p, b + 16, 0);
        ptr(p, b + 32, 0);
        assert_eq!(
            leaf(
                p,
                &mut thread,
                "NtMapViewOfSection",
                &[
                    handle,
                    arch.ptr(u64::MAX),
                    b + 16,
                    0,
                    0,
                    0,
                    b + 32,
                    2,
                    0x500000,
                    2
                ]
            ),
            STATUS_SUCCESS
        );
        let base = get(p, b + 16);
        assert_eq!(get(p, b + 32), selected.size);
        for region in &selected.regions {
            let query = p.vm.query(base + region.offset).unwrap();
            assert_eq!(query.kind, mem::MAPPED);
            if let Some(bytes) = &region.bytes {
                let mut actual = vec![0; bytes.len()];
                p.space.rd(base + region.offset, &mut actual).unwrap();
                assert_eq!(&actual, bytes.as_ref());
                assert_eq!((query.state, query.protect), (mem::COMMIT, prot::READONLY));
            } else {
                assert_eq!((query.state, query.protect), (mem::RESERVE, 0));
                assert!(p.space.u32(base + region.offset).is_err());
            }
        }
        ptr(p, b + 48, base);
        ptr(p, b + 64, 4096);
        p.space.w32(b + 80, 0xA5A5A5A5).unwrap();
        assert_eq!(
            leaf(
                p,
                &mut thread,
                "NtProtectVirtualMemory",
                &[arch.ptr(u64::MAX), b + 48, b + 64, 4, b + 80]
            ),
            0xC000004E
        );
        assert_eq!(p.space.u32(b + 80).unwrap(), prot::NOACCESS);
        assert_eq!(leaf(p, &mut thread, "NtClose", &[handle]), STATUS_SUCCESS);
        assert!(p.vm.is_data_view(base));
        assert_eq!(
            leaf(
                p,
                &mut thread,
                "NtUnmapViewOfSection",
                &[arch.ptr(u64::MAX), base + 1]
            ),
            STATUS_SUCCESS
        );
        assert!(!p.vm.is_data_view(base));
    }
}
