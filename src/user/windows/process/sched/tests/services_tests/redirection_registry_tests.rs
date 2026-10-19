use super::*;
use crate::user::windows::registry::{KeyTree, REDIRECTION_KEYS_KEY, TreeRoot};

#[test]
fn native_redirection_registry_known_absent_root_and_children_return_name_not_found_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        p.registry = p
            .registry
            .clone()
            .with_tree(TreeRoot::RedirectionKeys, None)
            .unwrap();
        let attrs = base + PAGE_SIZE * 2;
        let before = (p.objects.handle_count(), p.objects.iter().count());
        for suffix in ["", "\\image.exe", "\\missing\\nested"] {
            unicode(
                p,
                attrs + 64,
                attrs + 128,
                &format!("{REDIRECTION_KEYS_KEY}{suffix}"),
            );
            attributes(p, attrs, attrs + 64, 0, 0x240);
            assert_eq!(
                call(p, &mut t, "NtOpenKey", &[base, 0x20019, attrs]),
                STATUS_OBJECT_NAME_NOT_FOUND,
                "{arch}/{suffix}"
            );
            assert_eq!(p.space.ptr(base, arch.ptr_size()).unwrap(), 0);
            assert_eq!((p.objects.handle_count(), p.objects.iter().count()), before);
        }
    }
}

fn tree() -> KeyTree {
    KeyTree {
        values: vec![],
        children: vec![(
            "image.exe".encode_utf16().collect(),
            KeyTree {
                values: vec![Value {
                    name: "GlobalFlag".encode_utf16().collect(),
                    kind: 4,
                    data: 0xA1B2_C3D4u32.to_le_bytes().to_vec(),
                }],
                children: vec![("filter".encode_utf16().collect(), KeyTree::default())],
            },
        )],
    }
}
#[test]
fn native_redirection_registry_opens_relative_paths_with_observed_absence_and_root_lifetime_all_abis()
 {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        p.registry = p
            .registry
            .clone()
            .with_tree(TreeRoot::RedirectionKeys, Some(tree()))
            .unwrap();
        let attrs = base + PAGE_SIZE * 2;
        unicode(p, attrs + 64, attrs + 128, REDIRECTION_KEYS_KEY);
        attributes(p, attrs, attrs + 64, 0, 0x240);
        assert_eq!(
            call(p, &mut t, "NtOpenKey", &[base, 0x20019, attrs]),
            STATUS_SUCCESS
        );
        let root = p.space.ptr(base, arch.ptr_size()).unwrap();
        p.registry = Registry::default();
        for (name, expected) in [
            ("", STATUS_SUCCESS),
            ("IMAGE.exe", STATUS_SUCCESS),
            ("image.exe\\", STATUS_SUCCESS),
            ("image.exe\\\\FILTER\\", STATUS_SUCCESS),
            ("missing", STATUS_OBJECT_NAME_NOT_FOUND),
            ("missing\\nested", STATUS_OBJECT_NAME_NOT_FOUND),
            ("image.exe\\missing\\nested", STATUS_OBJECT_NAME_NOT_FOUND),
            (".", STATUS_OBJECT_NAME_NOT_FOUND),
            ("..", STATUS_OBJECT_NAME_NOT_FOUND),
            ("/", STATUS_OBJECT_NAME_NOT_FOUND),
            ("\\image.exe", STATUS_OBJECT_PATH_SYNTAX_BAD),
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
        unicode(p, attrs + 64, attrs + 128, "image.exe");
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
fn native_redirection_registry_absolute_children_serialize_raw_values_and_preserve_outside_frontier_all_abis()
 {
    for arch in WinArch::ALL {
        let (mut process, mut t, base) = setup(arch);
        let p = process.state_mut();
        p.registry = p
            .registry
            .clone()
            .with_tree(TreeRoot::RedirectionKeys, Some(tree()))
            .unwrap();
        let attrs = base + PAGE_SIZE * 2;
        for view in [0, 0x100, 0x200] {
            unicode(
                p,
                attrs + 64,
                attrs + 128,
                &format!("{REDIRECTION_KEYS_KEY}\\image.exe\\"),
            );
            attributes(p, attrs, attrs + 64, 0, 0x240);
            assert_eq!(
                call(p, &mut t, "NtOpenKey", &[base, 1 | view, attrs]),
                STATUS_SUCCESS
            );
            let h = p.space.ptr(base, arch.ptr_size()).unwrap();
            unicode(p, attrs + 64, attrs + 128, "globalflag");
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
            &format!("{REDIRECTION_KEYS_KEY}\\missing\\nested"),
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
            &format!("{REDIRECTION_KEYS_KEY}Suffix\\image.exe"),
        );
        arguments(p, &mut t, &[base, 1, attrs]);
        assert!(
            matches!(dispatch(p, &mut t), Outcome::Fail(reason) if reason.contains("outside selected runtime snapshot"))
        );
    }
}
