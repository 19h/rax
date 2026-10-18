//! Native-reference-derived named-mutant, grant and lifecycle cases.
use super::event_tests::{attrs, handle, setup, status};
use super::*;
use crate::user::windows::object_namespace::reparse_tests;
use crate::user::windows::sync;

fn named(p: &mut Proc, at: u64, root: u64, name: &[u16], flags: u32) {
    let width = p.arch.ptr_size();
    attrs(p, at, if p.arch.is64() { 48 } else { 24 }, flags, root);
    let descriptor = at + 64;
    p.space.wptr(at + 2 * width, width, descriptor).unwrap();
    p.space.wr(descriptor, &[0; 16]).unwrap();
    p.space.w16(descriptor, (2 * name.len()) as u16).unwrap();
    // MaximumLength is deliberately smaller than Length.
    p.space.w16(descriptor + 2, 0).unwrap();
    p.space.wptr(descriptor + width, width, at + 128).unwrap();
    p.space.put_wstr(at + 128, name).unwrap();
}

fn text(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

#[test]
fn native_mutant_create_grants_boolean_and_last_close_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        let oa = out + 256;
        let initial_handles = p.objects.handle_count();
        let initial_objects = p.objects.iter().count();
        for owner in [0, 1, 2, 255, 256, 257, u32::MAX] {
            for (access, grant) in [
                (0, 0),
                (1, 1),
                (2, 0),
                (3, 1),
                (0x001F_0001, 0x001F_0001),
                (0x001F_0003, 0x001F_0001),
                (0x0200_0000, 0x001F_0001),
                (0x8000_0000, 0x0002_0001),
                (0x4000_0000, 0x0002_0000),
                (0x2000_0000, 0x0012_0000),
                (0x1000_0000, 0x001F_0001),
                (0x0020_0000, 0),
            ] {
                attrs(p, oa, if arch.is64() { 48 } else { 24 }, 2, 0);
                assert_eq!(
                    status(
                        p,
                        &mut t,
                        "NtCreateMutant",
                        &[out + 1, access, oa, owner.into()]
                    ),
                    0
                );
                let h = handle(p, out + 1);
                assert_eq!(p.objects.access(h), Some(grant));
                assert_eq!(p.objects.flags(h), Some(1));
                assert!(
                    matches!(p.objects.get(h), Some(Object::Mutex { owner: actual, count, abandoned: false })
                    if *actual == (owner as u8 != 0).then_some(t.tid) && *count == u32::from(owner as u8 != 0))
                );
                p.objects.close(h).unwrap();
                assert_eq!(p.objects.handle_count(), initial_handles);
                assert_eq!(p.objects.iter().count(), initial_objects);
            }
        }
    }
}

#[test]
fn native_mutant_named_aliases_case_collision_and_root_grants_all_abis() {
    for arch in WinArch::ALL {
        for session in [0, 42] {
            let (mut process, mut t, out) = setup(arch);
            let p = process.state_mut();
            p.object_namespace = reparse_tests::sample(session);
            let base = if session == 0 {
                "\\BaseNamedObjects".to_owned()
            } else {
                format!("\\Sessions\\{session}\\BaseNamedObjects")
            };
            let directory = p.object_namespace.lookup(None, &text(&base), 0).unwrap();
            let root = p.objects.open_directory(directory, false, 0).unwrap() as u64;
            let oa = out + 256;
            named(p, oa, root, &text("Measured"), 0);
            assert_eq!(
                status(p, &mut t, "NtCreateMutant", &[out, 0x1F0001, oa, 1]),
                0
            );
            let first = handle(p, out);
            let first_id = p.objects.id(first).unwrap();
            for path in [
                "Measured".to_owned(),
                "Local\\Measured".to_owned(),
                format!("{base}\\Measured"),
            ] {
                named(
                    p,
                    oa,
                    if path.starts_with('\\') { 0 } else { root },
                    &text(&path),
                    0x80,
                );
                assert_eq!(
                    status(p, &mut t, "NtCreateMutant", &[out, 1, oa, 0]),
                    0x4000_0000
                );
                let alias = handle(p, out);
                assert_eq!(p.objects.id(alias), Some(first_id));
                assert!(matches!(
                    p.objects.get(alias),
                    Some(Object::Mutex {
                        owner: Some(_),
                        count: 1,
                        ..
                    })
                ));
                p.objects.close(alias).unwrap();
            }
            named(p, oa, root, &text("Measured"), 0);
            assert_eq!(
                status(p, &mut t, "NtCreateMutant", &[out, 1, oa, 0]),
                STATUS_OBJECT_NAME_COLLISION
            );
            named(p, oa, root, &text("mEASURED"), 0);
            assert_eq!(status(p, &mut t, "NtCreateMutant", &[out, 1, oa, 0]), 0);
            let second = handle(p, out);
            let second_id = p.objects.id(second).unwrap();
            assert_ne!(first_id, second_id);
            named(p, oa, root, &text("MEASURED"), 0x40);
            assert_eq!(status(p, &mut t, "NtOpenMutant", &[out, 1, oa]), 0);
            let alias = handle(p, out);
            assert_eq!(p.objects.id(alias), Some(second_id));
            p.objects.close(alias).unwrap();
            p.objects.close(second).unwrap();
            named(p, oa, root, &text("Global\\Measured"), 0x80);
            assert_eq!(
                status(p, &mut t, "NtCreateMutant", &[out, 1, oa, 0]),
                if session == 0 { 0x4000_0000 } else { 0 }
            );
            let global = handle(p, out);
            assert_eq!(p.objects.id(global) == Some(first_id), session == 0);
            p.objects.close(global).unwrap();
            p.objects.close(first).unwrap();
            named(p, oa, root, &text("Measured"), 0);
            assert_eq!(
                status(p, &mut t, "NtOpenMutant", &[out, 1, oa]),
                STATUS_OBJECT_NAME_NOT_FOUND
            );
            p.objects.close(root).unwrap();
        }
    }
}

#[test]
fn native_mutant_empty_names_null_attributes_and_root_types_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        p.object_namespace = reparse_tests::sample(0);
        let oa = out + 256;
        for root in [0, 0x1234, arch.ptr(u64::MAX)] {
            named(p, oa, root, &[], 0);
            assert_eq!(status(p, &mut t, "NtCreateMutant", &[out, 1, oa, 1]), 0);
            p.objects.close(handle(p, out)).unwrap();
            attrs(p, oa, if arch.is64() { 48 } else { 24 }, 0, root);
            assert_eq!(
                status(p, &mut t, "NtCreateMutant", &[out, 1, oa, 0]),
                if root == 0 {
                    0
                } else {
                    STATUS_OBJECT_NAME_INVALID
                }
            );
            if root == 0 {
                p.objects.close(handle(p, out)).unwrap();
            }
        }
        assert_eq!(status(p, &mut t, "NtCreateMutant", &[out, 1, 0, 0]), 0);
        p.objects.close(handle(p, out)).unwrap();
        assert_eq!(
            status(p, &mut t, "NtOpenMutant", &[out, 1, 0]),
            STATUS_INVALID_PARAMETER
        );
    }
}

#[test]
fn native_mutant_query_release_wait_recursion_and_abandonment_all_abis() {
    for arch in WinArch::ALL {
        let (mut process, mut t, out) = setup(arch);
        let p = process.state_mut();
        assert_eq!(
            status(p, &mut t, "NtCreateMutant", &[out, 0x1F0001, 0, 1]),
            0
        );
        let h = handle(p, out);
        let id = p.objects.id(h).unwrap();
        let zero = p.objects.open_access(id, false, 0).unwrap() as u64;
        let data = out + 64;
        assert_eq!(sync::try_objects(p, t.tid, &[id], false).unwrap(), Some(0));
        p.space.wr(data, &[0xA5; 32]).unwrap();
        assert_eq!(
            status(p, &mut t, "NtQueryMutant", &[h, 0, data, 8, data + 17]),
            0
        );
        assert_eq!(p.space.u32(data).unwrap(), u32::MAX);
        assert_eq!(p.space.bytes(data + 4, 4).unwrap(), [1, 0, 0xA5, 0xA5]);
        assert_eq!(p.space.u32(data + 17).unwrap(), 8);
        assert_eq!(
            status(
                p,
                &mut t,
                "NtQueryMutant",
                &[h, 1, data, 2 * arch.ptr_size(), data + 17]
            ),
            0
        );
        assert_eq!(handle(p, data), u64::from(p.pid));
        assert_eq!(handle(p, data + arch.ptr_size()), u64::from(t.tid));
        assert_eq!(
            status(p, &mut t, "NtQueryMutant", &[zero, 0, data, 8, 0]),
            STATUS_ACCESS_DENIED
        );
        for previous in [u32::MAX, 0] {
            assert_eq!(status(p, &mut t, "NtReleaseMutant", &[zero, data + 1]), 0);
            assert_eq!(p.space.u32(data + 1).unwrap(), previous);
        }
        assert_eq!(
            status(p, &mut t, "NtReleaseMutant", &[h, data]),
            0xC000_0046
        );
        if let Some(Object::Mutex {
            owner,
            count,
            abandoned,
        }) = p.objects.obj_mut(id)
        {
            *owner = None;
            *count = 0;
            *abandoned = true;
        }
        assert_eq!(status(p, &mut t, "NtQueryMutant", &[h, 0, data, 8, 0]), 0);
        assert_eq!(p.space.bytes(data + 4, 2).unwrap(), [0, 1]);
        assert_eq!(
            sync::try_objects(p, t.tid, &[id], false).unwrap(),
            Some(0x80)
        );
        assert_eq!(
            status(p, &mut t, "NtQueryMutant", &[h, 0, data, 8, data]),
            0
        );
        assert_eq!(
            p.space.u32(data).unwrap(),
            8,
            "ReturnLength is the final aliased write"
        );
        p.objects.close(zero).unwrap();
        p.objects.close(h).unwrap();
    }
}
