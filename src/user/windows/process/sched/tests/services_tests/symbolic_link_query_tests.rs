//! Full native post-call bytes, protection states and grant ordering, replayed without host calls.
use super::*;
fn profiles_query(family: u8) -> [(WinArch, &'static str); 4] {
    if family == 3 {
        [
            (
                WinArch::Arm64,
                corpus!("native-symbolic-link-query-dual-faults-arm64.log"),
            ),
            (
                WinArch::X64,
                corpus!("native-symbolic-link-query-dual-faults-x64.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-dual-faults-x86.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-dual-faults-x86-laa.log"),
            ),
        ]
    } else if family == 1 {
        [
            (
                WinArch::Arm64,
                corpus!("native-symbolic-link-query-alignment-arm64.log"),
            ),
            (
                WinArch::X64,
                corpus!("native-symbolic-link-query-alignment-x64.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-alignment-x86.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-alignment-x86-laa.log"),
            ),
        ]
    } else if family == 2 {
        [
            (
                WinArch::Arm64,
                corpus!("native-symbolic-link-query-copy-boundaries-arm64.log"),
            ),
            (
                WinArch::X64,
                corpus!("native-symbolic-link-query-copy-boundaries-x64.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-copy-boundaries-x86.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-copy-boundaries-x86-laa.log"),
            ),
        ]
    } else {
        [
            (
                WinArch::Arm64,
                corpus!("native-symbolic-link-query-matrix-arm64.log"),
            ),
            (
                WinArch::X64,
                corpus!("native-symbolic-link-query-matrix-x64.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-matrix-x86.log"),
            ),
            (
                WinArch::X86,
                corpus!("native-symbolic-link-query-matrix-x86-laa.log"),
            ),
        ]
    }
}
fn call_query(p: &mut Proc, t: &mut Thread, args: [u64; 3]) -> u32 {
    // Portable replay replaces only the synthetic table; installed-leaf tests
    // independently execute the selected host DLL and its actual service number.
    let previous = p.modules.nt_services.take().unwrap();
    p.modules.nt_services = Some((
        previous.0,
        table(p.arch, "NtQuerySymbolicLinkObject", SERVICE),
    ));
    let result = call(p, t, args);
    p.modules.nt_services = Some(previous);
    result
}
fn protect_mode(p: &mut Proc, at: u64, kind: u64) {
    p.vm.protect(
        at,
        PAGE_SIZE,
        match kind {
            5 => prot::NOACCESS,
            6 => prot::READONLY,
            7 => prot::READWRITE | prot::GUARD,
            _ => panic!("protection"),
        },
    )
    .unwrap();
}
#[test]
fn native_symbolic_link_query_replays_all_15192_capacity_optional_length_fault_grant_alias_and_guard_rows()
 {
    replay_query(0, 3798);
}
#[test]
fn native_symbolic_link_query_replays_all_900_descriptor_alignment_rows() {
    replay_query(1, 225);
}
#[test]
fn native_symbolic_link_query_replays_all_25536_dual_fault_guard_copy_and_alias_rows() {
    replay_query(3, 6384);
}
fn replay_query(family: u8, expected: usize) {
    let mut total = 0;
    for (arch, text) in profiles_query(family) {
        let rs = rows(text, "case=");
        assert_eq!(rs.len(), expected);
        assert!(text.contains(&format!("complete cases={expected}")));
        let (mut process, mut t, b) = setup(arch, 54);
        let p = process.state_mut();
        let baseline = (p.objects.handle_count(), p.objects.iter().count());
        for r in rs {
            p.vm.protect(b, 54 * PAGE_SIZE, prot::READWRITE).unwrap();
            p.space.wr(b, &vec![0xA5; 54 * PAGE_SIZE as usize]).unwrap();
            let hk = field(&r, "handle", 10);
            let mut owned = None;
            let handle = match hk {
                0 | 1 | 2 | 3 | 9 | 10 => {
                    let h = opened(
                        p,
                        &mut t,
                        b + 16 * PAGE_SIZE,
                        if hk == 1 {
                            "\\KnownDlls32\\KnownDllPath"
                        } else {
                            "\\KnownDlls\\KnownDllPath"
                        },
                        if hk == 3 { 0x20000 } else { 1 },
                    );
                    let h = if hk == 2 {
                        let id = p.objects.id(h).unwrap();
                        let z = p.objects.open_access(id, false, 0).unwrap().into();
                        p.objects.close(h).unwrap();
                        z
                    } else {
                        h
                    };
                    if hk == 9 {
                        p.objects.close(h).unwrap();
                    } else {
                        owned = Some(h);
                    }
                    if hk == 10 { h | 3 } else { h }
                }
                4 => {
                    let h = opened_directory(p, "\\KnownDlls", 0x20000);
                    owned = Some(h);
                    h
                }
                5 => {
                    let h = p.objects.insert(Object::Event {
                        manual: false,
                        signaled: 0,
                    }) as u64;
                    owned = Some(h);
                    h
                }
                6 => 0x12345678,
                7 => arch.ptr(u64::MAX),
                8 => arch.ptr(u64::MAX - 1),
                _ => panic!("handle"),
            };
            assert_eq!(field(&r, "open", 16), 0);
            let (d, buf, ret) = (b, b + 18 * PAGE_SIZE, b + 36 * PAGE_SIZE);
            let (dm, bm, rm) = (
                field(&r, "descriptor-mode", 10),
                field(&r, "buffer-mode", 10),
                field(&r, "returned-mode", 10),
            );
            let descriptor_address = match dm {
                20..=35 => d + PAGE_SIZE - (dm - 19),
                1 => 0,
                2 => 1,
                3 => d + 65,
                4 => d + PAGE_SIZE - if arch.is64() { 8 } else { 4 },
                8 => d + 66,
                9 => d + 68,
                10 => d + 72,
                _ => d + 64,
            };
            let buffer = match bm {
                1 => 0,
                2 => 1,
                3 => buf + 65,
                4 => buf + PAGE_SIZE - 1,
                _ => buf + 64,
            };
            if dm != 1 && dm != 2 {
                descriptor(
                    p,
                    descriptor_address,
                    buffer,
                    field(&r, "length", 10) as u16,
                    field(&r, "maximum", 10) as u16,
                );
            }
            if (20..=35).contains(&dm) {
                p.vm.protect(d + PAGE_SIZE, PAGE_SIZE, prot::READONLY)
                    .unwrap();
            } else if dm == 4 {
                p.vm.protect(d + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            } else if (5..=7).contains(&dm) {
                protect_mode(p, d, dm);
            }
            if bm == 4 {
                p.vm.protect(buf + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            } else if (5..=7).contains(&bm) {
                protect_mode(p, buf, bm);
            }
            let returned = match rm {
                1 => 0,
                2 => 1,
                3 => ret + 65,
                4 => ret + PAGE_SIZE - 2,
                8 => descriptor_address,
                9 => buffer,
                _ => ret + 64,
            };
            if rm == 4 {
                p.vm.protect(ret + PAGE_SIZE, PAGE_SIZE, prot::NOACCESS)
                    .unwrap();
            } else if (5..=7).contains(&rm) {
                protect_mode(p, ret, rm);
            }
            let status = call_query(p, &mut t, [handle, descriptor_address, returned]);
            assert_eq!(status, field(&r, "status", 16) as u32, "row={r:?}");
            assert_eq!(field(&r, "exception", 16), 0);
            for (key, at) in [
                ("descriptor-protection", d),
                ("descriptor-next", d + PAGE_SIZE),
                ("buffer-protection", buf),
                ("buffer-next", buf + PAGE_SIZE),
                ("returned-protection", ret),
                ("returned-next", ret + PAGE_SIZE),
            ] {
                assert_eq!(
                    p.vm.query(at).unwrap().protect as u64,
                    field(&r, key, 16),
                    "{key} row={r:?}"
                );
            }
            p.vm.protect(b, 54 * PAGE_SIZE, prot::READWRITE).unwrap();
            for (key, at, n) in [
                ("descriptor-data", d + 48, 96),
                ("descriptor-edge", d + PAGE_SIZE - 32, 64),
                ("buffer-data", buf + 48, 128),
                ("buffer-edge", buf + PAGE_SIZE - 32, 64),
                ("returned-data", ret + 48, 96),
                ("returned-edge", ret + PAGE_SIZE - 32, 64),
            ] {
                let mut expected = hex(r[key]);
                // Relocate only the measured original Buffer field. All other
                // returned bytes, including aliases and padding, are compared verbatim.
                if dm != 1
                    && dm != 2
                    && (((dm == 4 || dm >= 20) && key == "descriptor-edge")
                        || (dm != 4 && dm < 20 && key == "descriptor-data"))
                {
                    let offset =
                        (descriptor_address + if arch.is64() { 8 } else { 4 } - at) as usize;
                    let width = arch.ptr_size() as usize;
                    let old = field(&r, "buffer-address", 16).to_le_bytes();
                    assert_eq!(&expected[offset..offset + width], &old[..width]);
                    expected[offset..offset + width]
                        .copy_from_slice(&buffer.to_le_bytes()[..width]);
                }
                assert_eq!(p.space.bytes(at, n).unwrap(), expected, "{key} row={r:?}");
            }
            if let Some(h) = owned {
                p.objects.close(h).unwrap();
            }
            assert_eq!(
                (p.objects.handle_count(), p.objects.iter().count()),
                baseline,
                "row={r:?}"
            );
            total += 1;
        }
    }
    assert_eq!(total, expected * 4);
}

#[test]
fn native_symbolic_link_query_replays_all_3072_descriptor_copy_boundary_and_atomicity_rows() {
    replay_query(2, 768);
}
