use super::base_named_object_tests::{captures, table};
use super::*;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn link(name: &str, target: &str) -> CapturedEntry {
    CapturedEntry {
        name: units(name),
        kind: units("SymbolicLink"),
        target: Some(units(target)),
        link_access: Some(LinkAccess {
            maximum_access: 0x000F_0001,
            system_security_status: STATUS_PRIVILEGE_NOT_HELD,
        }),
        section: None,
    }
}

fn selected(session: u32) -> Vec<SelectedDirectory> {
    let mut dirs = captures(session);
    for directory in &mut dirs {
        if directory.path.ends_with(&units("\\BaseNamedObjects")) {
            let path = String::from_utf16(&directory.path).unwrap();
            directory.entries.extend([
                link("Local", &path),
                link("Global", "\\BaseNamedObjects"),
                link("Session", "\\Sessions\\BNOLINKS"),
                link("DosPath", "C:\\Windows"),
            ]);
        }
    }
    dirs
}

pub(crate) fn sample(session: u32) -> Namespace {
    Namespace::selected_for_session(table(), selected(session), Some(session)).unwrap()
}

#[test]
fn native_object_reparse_preserves_local_global_absolute_relative_identity() {
    for session in [0, 1, 42, u32::MAX] {
        let namespace = sample(session);
        let base_path = if session == 0 {
            "\\BaseNamedObjects".to_owned()
        } else {
            format!("\\Sessions\\{session}\\BaseNamedObjects")
        };
        let local = namespace.lookup(None, &units(&base_path), 0).unwrap();
        let global = namespace
            .lookup(None, &units("\\BaseNamedObjects"), 0)
            .unwrap();
        for (name, expected) in [("Local", &local), ("Global", &global)] {
            let absolute = namespace
                .lookup(None, &units(&format!("{base_path}\\{name}")), 0)
                .unwrap();
            let relative = namespace
                .lookup(Some(local.clone()), &units(name), 0)
                .unwrap();
            assert!(Arc::ptr_eq(&absolute, expected));
            assert!(Arc::ptr_eq(&relative, expected));
        }
    }
}

#[test]
fn native_object_reparse_matches_the_sixty_three_link_boundary_and_refusal_order() {
    let namespace = sample(0);
    let expected = namespace
        .lookup(None, &units("\\BaseNamedObjects"), 0)
        .unwrap();
    for count in [0, 1, 2, 31, 32, 62, 63, 64, 65, 128, 1024] {
        let name = format!("\\BaseNamedObjects{}", "\\Local".repeat(count));
        for flags in [0, 0x40, 0x1000, 0x1040] {
            let result = namespace.lookup(None, &units(&name), flags);
            if count != 0 && flags & 0x1000 != 0 {
                assert_eq!(
                    result.unwrap_err(),
                    LookupError::Status(STATUS_REPARSE_POINT_ENCOUNTERED)
                );
            } else if count >= 64 {
                assert_eq!(
                    result.unwrap_err(),
                    LookupError::Status(STATUS_OBJECT_NAME_NOT_FOUND)
                );
            } else {
                assert!(Arc::ptr_eq(&result.unwrap(), &expected));
            }
        }
    }
}

#[test]
fn native_object_reparse_keeps_case_unknown_targets_and_link_queries_distinct() {
    let namespace = sample(0);
    assert_eq!(
        namespace
            .lookup(None, &units("\\BaseNamedObjects\\local"), 0)
            .unwrap_err(),
        LookupError::Status(STATUS_OBJECT_NAME_NOT_FOUND)
    );
    assert!(
        namespace
            .lookup(None, &units("\\BaseNamedObjects\\local"), 0x40)
            .is_ok()
    );
    assert_eq!(
        namespace
            .lookup(None, &units("\\BaseNamedObjects\\Session"), 0)
            .unwrap_err(),
        LookupError::Unselected
    );
    assert_eq!(
        namespace
            .lookup_link(None, &units("\\BaseNamedObjects\\Session"), 0)
            .unwrap()
            .target,
        units("\\Sessions\\BNOLINKS")
    );
    assert_eq!(
        namespace
            .lookup(None, &units("\\BaseNamedObjects\\DosPath"), 0)
            .unwrap_err(),
        LookupError::Status(STATUS_OBJECT_NAME_INVALID)
    );
    assert_eq!(
        namespace
            .lookup_link(None, &units("\\BaseNamedObjects\\Local"), 0x1000)
            .unwrap()
            .target,
        units("\\BaseNamedObjects")
    );
}

#[test]
fn native_object_reparse_bounds_cycles_and_preserves_incomplete_membership() {
    let mut dirs = selected(0);
    dirs[1]
        .entries
        .iter_mut()
        .find(|entry| entry.name == units("Local"))
        .unwrap()
        .target = Some(units("\\BaseNamedObjects\\Local"));
    let cycle = Namespace::selected(table(), dirs).unwrap();
    assert_eq!(
        cycle
            .lookup(None, &units("\\BaseNamedObjects\\Local"), 0)
            .unwrap_err(),
        LookupError::Status(STATUS_OBJECT_NAME_NOT_FOUND)
    );
    let namespace = Namespace::selected_for_session_partial(
        table(),
        selected(0),
        Some(0),
        &[units("\\BaseNamedObjects")],
    )
    .unwrap();
    assert_eq!(
        namespace
            .lookup(None, &units("\\BaseNamedObjects\\Local\\Unknown"), 0)
            .unwrap_err(),
        LookupError::Unselected
    );
    assert_eq!(
        namespace
            .lookup(None, &units("\\BaseNamedObjects\\Local\\"), 0)
            .unwrap_err(),
        LookupError::Status(STATUS_OBJECT_NAME_INVALID)
    );
}
