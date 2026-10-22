use super::*;
use crate::user::windows::registry::{
    GRE_INITIALIZE_KEY, TreeRoot,
    gre_initialize_tests::{CAPTURES, capture, captured_tree, field},
};

fn select(p: &mut Proc, log: &str) {
    let nodes = capture(log, "0", "000");
    let upcase = (0..=u16::MAX)
        .map(|unit| {
            if (97..=122).contains(&unit) {
                unit - 32
            } else {
                unit
            }
        })
        .collect();
    p.registry = Registry::selected(upcase, vec![])
        .unwrap()
        .with_tree(TreeRoot::GreInitialize, Some(captured_tree(&nodes)))
        .unwrap();
}

#[test]
fn native_gre_initialize_registry_open_replays_native_read_grants_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup(arch);
        let p = process.state_mut();
        let attrs = base + PAGE_SIZE * 2;
        let error = thread.teb + crate::user::windows::layout::offsets(arch).teb_last_error;
        let initial = p.objects.handle_count();
        for log in CAPTURES {
            select(p, log);
            let mut calls = 0;
            for row in log.lines().filter(|row| row.starts_with("OPEN ")) {
                let access = u32::from_str_radix(field(row, "grant="), 16).unwrap();
                // Native write-capable open controls are retained verbatim.
                // The existing guest registry contract admits read grants only;
                // those write controls are checked separately below.
                if matches!(access, 2 | 0xF003F) {
                    continue;
                }
                let spelling: u32 = field(row, "spelling=").parse().unwrap();
                let mut path = GRE_INITIALIZE_KEY.to_owned();
                if spelling != 0 {
                    path.push('\\');
                }
                if spelling == 2 {
                    path.make_ascii_uppercase();
                }
                if spelling == 3 {
                    path.make_ascii_lowercase();
                }
                unicode(p, attrs + 64, attrs + 128, &path);
                let flags = u32::from_str_radix(field(row, "flags="), 16).unwrap();
                attributes(p, attrs, attrs + 64, 0, flags);
                p.space.wptr(base, arch.ptr_size(), 0xA5A5A5A5).unwrap();
                p.space.w32(error, 0x12345678).unwrap();
                let status = u32::from_str_radix(field(row, "status="), 16).unwrap();
                assert_eq!(field(row, "exception="), "00000000");
                assert_eq!(
                    call(
                        p,
                        &mut thread,
                        "NtOpenKey",
                        &[base, u64::from(access), attrs]
                    ),
                    status,
                    "{arch}: {row}"
                );
                let handle = p.space.ptr(base, arch.ptr_size()).unwrap();
                if status == STATUS_SUCCESS {
                    assert_ne!(handle, 0);
                    let Some(Object::Key(key)) = p.objects.get(handle) else {
                        panic!("successful open must return an owned key")
                    };
                    assert_eq!(
                        key.path,
                        GRE_INITIALIZE_KEY.encode_utf16().collect::<Vec<_>>()
                    );
                    assert_eq!(
                        p.objects.access(handle),
                        Some(if access & 0x0200_0000 != 0 {
                            0x20019
                        } else {
                            access & !0x300
                        })
                    );
                    assert_eq!(p.objects.handle_count(), initial + 1);
                    assert_eq!(call(p, &mut thread, "NtClose", &[handle]), STATUS_SUCCESS);
                } else {
                    assert_eq!(handle, 0);
                }
                assert_eq!(
                    p.space.u32(error).unwrap(),
                    u32::from_str_radix(field(row, "last-error="), 16).unwrap()
                );
                assert_eq!(p.objects.handle_count(), initial);
                assert!(thread.frames.is_empty());
                calls += 1;
            }
            assert_eq!(calls, 112);
        }
    }
}

#[test]
fn native_gre_initialize_registry_preserves_existing_guest_read_only_policy_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut thread, base) = setup(arch);
        let p = process.state_mut();
        select(p, CAPTURES[0]);
        let attrs = base + PAGE_SIZE * 2;
        unicode(p, attrs + 64, attrs + 128, GRE_INITIALIZE_KEY);
        attributes(p, attrs, attrs + 64, 0, 0x240);
        let initial = p.objects.handle_count();
        for access in [2, 0xF003F] {
            assert_eq!(
                call(p, &mut thread, "NtOpenKey", &[base, access, attrs]),
                STATUS_ACCESS_DENIED
            );
            assert_eq!(p.space.ptr(base, arch.ptr_size()).unwrap(), 0);
            assert_eq!(p.objects.handle_count(), initial);
        }
    }
}

#[test]
fn native_gre_initialize_registry_values_and_relative_handles_survive_namespace_drop_all_abis() {
    for arch in WinArch::ALL {
        for log in CAPTURES {
            for view in [0, 0x100, 0x200] {
                let (mut process, mut thread, base) = setup(arch);
                let p = process.state_mut();
                select(p, log);
                let nodes = capture(log, "0", "000");
                let attrs = base + PAGE_SIZE * 2;
                let initial = p.objects.handle_count();
                unicode(p, attrs + 64, attrs + 128, GRE_INITIALIZE_KEY);
                attributes(p, attrs, attrs + 64, 0, 0x240);
                assert_eq!(
                    call(p, &mut thread, "NtOpenKey", &[base, 0x20019 | view, attrs]),
                    STATUS_SUCCESS
                );
                let root = p.space.ptr(base, arch.ptr_size()).unwrap();
                p.registry = Registry::default();
                let mut queried = 0;
                for node in &nodes {
                    let path = String::from_utf16(&node.path).unwrap();
                    let handle = if path == GRE_INITIALIZE_KEY {
                        root
                    } else {
                        let relative = path
                            .strip_prefix(&format!("{GRE_INITIALIZE_KEY}\\"))
                            .unwrap();
                        unicode(p, attrs + 64, attrs + 128, relative);
                        attributes(p, attrs, attrs + 64, root, 0x240);
                        assert_eq!(
                            call(p, &mut thread, "NtOpenKey", &[base, 0x20019 | view, attrs]),
                            STATUS_SUCCESS
                        );
                        p.space.ptr(base, arch.ptr_size()).unwrap()
                    };
                    for value in &node.values {
                        let name = String::from_utf16(&value.name)
                            .unwrap()
                            .to_ascii_lowercase();
                        unicode(p, attrs + 64, attrs + 128, &name);
                        p.space.wr(base + 128, &[0xA5; 128]).unwrap();
                        assert_eq!(
                            call(
                                p,
                                &mut thread,
                                "NtQueryValueKey",
                                &[handle, attrs + 64, 2, base + 128, 128, base + PAGE_SIZE]
                            ),
                            STATUS_SUCCESS
                        );
                        assert_eq!(p.space.u32(base + 128).unwrap(), 0);
                        assert_eq!(p.space.u32(base + 132).unwrap(), value.kind);
                        assert_eq!(p.space.u32(base + 136).unwrap(), value.data.len() as u32);
                        assert_eq!(
                            p.space.bytes(base + 140, value.data.len()).unwrap(),
                            value.data
                        );
                        assert_eq!(
                            p.space.u32(base + PAGE_SIZE).unwrap(),
                            12 + value.data.len() as u32
                        );
                        assert_eq!(
                            p.space.u8(base + 140 + value.data.len() as u64).unwrap(),
                            0xA5
                        );
                        queried += 1;
                    }
                    if handle != root {
                        assert_eq!(call(p, &mut thread, "NtClose", &[handle]), STATUS_SUCCESS);
                    }
                }
                assert_eq!(queried, 7);
                assert_eq!(call(p, &mut thread, "NtClose", &[root]), STATUS_SUCCESS);
                assert_eq!(p.objects.handle_count(), initial);
            }
        }
    }
}
