use super::*;
use crate::user::windows::registry::{KeyTree, NLS_SORTING_VERSIONS_KEY, TreeRoot};

#[test]
fn native_nls_sorting_registry_known_absent_root_and_children_return_name_not_found_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        p.registry = p
            .registry
            .clone()
            .with_tree(TreeRoot::NlsSortingVersions, None)
            .unwrap();
        let attrs = base + PAGE_SIZE * 2;
        let before = (p.objects.handle_count(), p.objects.iter().count());
        for suffix in ["", "\\", "\\profile", "\\missing\\nested"] {
            for access in [1, 3, 0x20019, 0x20119, 0x20219] {
                unicode(
                    p,
                    attrs + 64,
                    attrs + 128,
                    &format!("{NLS_SORTING_VERSIONS_KEY}{suffix}"),
                );
                attributes(p, attrs, attrs + 64, 0, 0x240);
                assert_eq!(
                    call(p, &mut t, "NtOpenKey", &[base, access, attrs]),
                    STATUS_OBJECT_NAME_NOT_FOUND,
                    "{arch}/{suffix}"
                );
                assert_eq!(p.space.ptr(base, arch.ptr_size()).unwrap(), 0);
                assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            }
        }
    }
}

fn tree() -> KeyTree {
    KeyTree {
        values: vec![],
        children: vec![(
            "profile".encode_utf16().collect(),
            KeyTree {
                values: vec![Value {
                    name: "OptionValue".encode_utf16().collect(),
                    kind: 4,
                    data: 0xA1B2_C3D4u32.to_le_bytes().to_vec(),
                }],
                children: vec![("filter".encode_utf16().collect(), KeyTree::default())],
            },
        )],
    }
}
#[test]
fn native_nls_sorting_registry_opens_relative_paths_with_observed_absence_and_root_lifetime_all_abis()
 {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        p.registry = p
            .registry
            .clone()
            .with_tree(TreeRoot::NlsSortingVersions, Some(tree()))
            .unwrap();
        let attrs = base + PAGE_SIZE * 2;
        unicode(p, attrs + 64, attrs + 128, NLS_SORTING_VERSIONS_KEY);
        attributes(p, attrs, attrs + 64, 0, 0x240);
        assert_eq!(
            call(p, &mut t, "NtOpenKey", &[base, 0x20019, attrs]),
            STATUS_SUCCESS
        );
        let root = p.space.ptr(base, arch.ptr_size()).unwrap();
        p.registry = Registry::default();
        for (name, expected) in [
            ("", STATUS_SUCCESS),
            ("PROFILE", STATUS_SUCCESS),
            ("profile\\", STATUS_SUCCESS),
            ("profile\\\\FILTER\\", STATUS_SUCCESS),
            ("missing", STATUS_OBJECT_NAME_NOT_FOUND),
            ("missing\\nested", STATUS_OBJECT_NAME_NOT_FOUND),
            ("profile\\missing\\nested", STATUS_OBJECT_NAME_NOT_FOUND),
            (".", STATUS_OBJECT_NAME_NOT_FOUND),
            ("..", STATUS_OBJECT_NAME_NOT_FOUND),
            ("/", STATUS_OBJECT_NAME_NOT_FOUND),
            ("\\profile", STATUS_OBJECT_PATH_SYNTAX_BAD),
        ] {
            unicode(p, attrs + 64, attrs + 128, name);
            attributes(p, attrs, attrs + 64, root, 0x240);
            assert_eq!(
                call(p, &mut t, "NtOpenKey", &[base, 1, attrs]),
                expected,
                "{arch}/{name}"
            );
            let h = p.space.ptr(base, arch.ptr_size()).unwrap();
            if expected == STATUS_SUCCESS {
                assert_eq!(call(p, &mut t, "NtClose", &[h]), STATUS_SUCCESS);
            } else {
                assert_eq!(h, 0);
            }
        }
        unicode(p, attrs + 64, attrs + 128, "profile");
        attributes(p, attrs, attrs + 64, root, 0x240);
        assert_eq!(
            call(p, &mut t, "NtOpenKey", &[base, 2, attrs]),
            STATUS_ACCESS_DENIED
        );
        assert_eq!(call(p, &mut t, "NtClose", &[root]), STATUS_SUCCESS);
        assert_eq!(
            call(p, &mut t, "NtOpenKey", &[base, 1, attrs]),
            STATUS_INVALID_HANDLE
        );
    }
}
#[test]
fn native_nls_sorting_registry_absolute_children_serialize_raw_values_and_preserve_outside_frontier_all_abis()
 {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        p.registry = p
            .registry
            .clone()
            .with_tree(TreeRoot::NlsSortingVersions, Some(tree()))
            .unwrap();
        let attrs = base + PAGE_SIZE * 2;
        for view in [0, 0x100, 0x200] {
            unicode(
                p,
                attrs + 64,
                attrs + 128,
                &format!("{NLS_SORTING_VERSIONS_KEY}\\profile\\"),
            );
            attributes(p, attrs, attrs + 64, 0, 0x240);
            assert_eq!(
                call(p, &mut t, "NtOpenKey", &[base, 1 | view, attrs]),
                STATUS_SUCCESS
            );
            let h = p.space.ptr(base, arch.ptr_size()).unwrap();
            unicode(p, attrs + 64, attrs + 128, "optionvalue");
            assert_eq!(
                call(
                    p,
                    &mut t,
                    "NtQueryValueKey",
                    &[h, attrs + 64, 2, base + 128, 64, base + PAGE_SIZE]
                ),
                STATUS_SUCCESS
            );
            assert_eq!(p.space.u32(base + 132).unwrap(), 4);
            assert_eq!(p.space.u32(base + 136).unwrap(), 4);
            assert_eq!(p.space.u32(base + 140).unwrap(), 0xA1B2_C3D4);
            assert_eq!(p.space.u32(base + PAGE_SIZE).unwrap(), 16);
            assert_eq!(call(p, &mut t, "NtClose", &[h]), STATUS_SUCCESS);
        }
        unicode(
            p,
            attrs + 64,
            attrs + 128,
            &format!("{NLS_SORTING_VERSIONS_KEY}\\missing\\nested"),
        );
        attributes(p, attrs, attrs + 64, 0, 0x240);
        assert_eq!(
            call(p, &mut t, "NtOpenKey", &[base, 1, attrs]),
            STATUS_OBJECT_NAME_NOT_FOUND
        );
        unicode(
            p,
            attrs + 64,
            attrs + 128,
            &format!("{NLS_SORTING_VERSIONS_KEY}Suffix\\profile"),
        );
        arguments(p, &mut t, &[base, 1, attrs]);
        assert!(
            matches!(dispatch(p, &mut t), Outcome::Fail(reason) if reason.contains("outside selected runtime snapshot"))
        );
    }
}

const NATIVE: [&str; 4] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-nls-sorting/native/native-nls-sorting-v1-four-views-arm64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-nls-sorting/native/native-nls-sorting-v1-four-views-x64.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-nls-sorting/native/native-nls-sorting-v1-four-views-x86.log"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specifications/windows/native-nls-sorting/native/native-nls-sorting-v1-four-views-x86-laa.log"
    )),
];
fn field<'a>(line: &'a str, name: &str) -> &'a str {
    line.split(' ')
        .find_map(|part| part.strip_prefix(name))
        .unwrap()
}
fn number(line: &str, name: &str, radix: u32) -> u64 {
    u64::from_str_radix(field(line, name), radix).unwrap()
}
fn raw_values(log: &str) -> Vec<Value> {
    log.lines()
        .filter(|line| line.starts_with("VALUE view=00000000 "))
        .map(|line| {
            let name = field(line, "name=");
            let data = field(line, "data=");
            Value {
                name: (0..name.len())
                    .step_by(4)
                    .map(|i| u16::from_str_radix(&name[i..i + 4], 16).unwrap())
                    .collect(),
                kind: number(line, "kind=", 10) as u32,
                data: (0..data.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&data[i..i + 2], 16).unwrap())
                    .collect(),
            }
        })
        .collect()
}
#[test]
fn native_nls_sorting_registry_replays_recorded_opens_and_raw_values_all_abis() {
    let values = raw_values(NATIVE[0]);
    assert_eq!(values.len(), 10);
    for native in NATIVE {
        assert_eq!(raw_values(native), values);
        assert_eq!(
            native.lines().filter(|l| l.starts_with("OPEN ")).count(),
            80
        );
    }
    for arch in WinArch::ALL {
        for native in NATIVE {
            let (mut process, mut t, base) = setup(arch);
            let p = process.state_mut();
            p.registry = p
                .registry
                .clone()
                .with_tree(
                    TreeRoot::NlsSortingVersions,
                    Some(KeyTree {
                        values: values.clone(),
                        children: vec![],
                    }),
                )
                .unwrap();
            let attrs = base + PAGE_SIZE * 2;
            let root = NLS_SORTING_VERSIONS_KEY;
            let paths = [
                root.to_string(),
                format!("{root}\\"),
                format!("{root}\\missing-rax-child"),
                format!("{root}\\\\missing-rax-child\\nested"),
                format!("{root}Suffix"),
                root.to_ascii_lowercase(),
                format!("{root}\\."),
                format!("{root}\\.."),
            ];
            let before = (p.objects.handle_count(), p.objects.iter().count());
            for line in native.lines().filter(|l| l.starts_with("OPEN ")) {
                let path = number(line, "path=", 10) as usize;
                let access = number(line, "desired=", 16);
                let observed = number(line, "status=", 16) as u32;
                unicode(p, attrs + 64, attrs + 128, &paths[path]);
                attributes(p, attrs, attrs + 64, 0, 0x40);
                if path == 4 {
                    arguments(p, &mut t, &[base, access, attrs]);
                    assert!(
                        matches!(dispatch(p, &mut t), Outcome::Fail(reason) if reason.contains("outside selected runtime snapshot"))
                    );
                    continue;
                }
                // Host rights include KEY_SET_VALUE; guest snapshots deliberately
                // retain the existing read-only contract rather than host mutation.
                let expected = if observed == 0 && access & 2 != 0 {
                    STATUS_ACCESS_DENIED
                } else {
                    observed
                };
                assert_eq!(
                    call(p, &mut t, "NtOpenKey", &[base, access, attrs]),
                    expected,
                    "{arch}/{line}"
                );
                let h = p.space.ptr(base, arch.ptr_size()).unwrap();
                if expected == STATUS_SUCCESS {
                    for value in &values {
                        unicode(
                            p,
                            attrs + 64,
                            attrs + 128,
                            &String::from_utf16(&value.name).unwrap(),
                        );
                        assert_eq!(
                            call(
                                p,
                                &mut t,
                                "NtQueryValueKey",
                                &[h, attrs + 64, 2, base + 128, 512, base + PAGE_SIZE]
                            ),
                            STATUS_SUCCESS
                        );
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
                    }
                    assert_eq!(call(p, &mut t, "NtClose", &[h]), STATUS_SUCCESS);
                } else {
                    assert_eq!(h, 0);
                }
                assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
            }
        }
    }
}
