use super::*;
fn table() -> Arc<[u16]> {
    (0..=u16::MAX)
        .map(|u| if (97..=122).contains(&u) { u - 32 } else { u })
        .collect::<Vec<_>>()
        .into()
}
fn selected(forced: bool) -> Vec<SelectedDirectory> {
    vec![
        SelectedDirectory {
            path: "\\".encode_utf16().collect(),
            maximum_access: ALL_ACCESS,
            system_security_status: STATUS_PRIVILEGE_NOT_HELD,
            force_case_insensitive: forced,
            entries: vec![
                CapturedEntry {
                    name: "KnownDlls".encode_utf16().collect(),
                    kind: "Directory".encode_utf16().collect(),
                    target: None,
                    link_access: None,
                },
                CapturedEntry {
                    name: "Other".encode_utf16().collect(),
                    kind: "Directory".encode_utf16().collect(),
                    target: None,
                    link_access: None,
                },
            ],
        },
        SelectedDirectory {
            path: "\\KnownDlls".encode_utf16().collect(),
            maximum_access: 0x20003,
            system_security_status: STATUS_PRIVILEGE_NOT_HELD,
            force_case_insensitive: forced,
            entries: vec![CapturedEntry {
                name: "ntdll.dll".encode_utf16().collect(),
                kind: "Section".encode_utf16().collect(),
                target: None,
                link_access: None,
            }],
        },
    ]
}
fn lookup(n: &Namespace, s: &str, flags: u32) -> Result<Arc<Directory>, LookupError> {
    n.lookup(None, &s.encode_utf16().collect::<Vec<_>>(), flags)
}
#[test]
fn directory_catalog_bounds_collision_and_unselected_parent_are_explicit() {
    let n = Namespace::selected(table(), selected(false)).unwrap();
    assert!(matches!(lookup(&n, "\\KnownDlls", 0), Ok(_)));
    assert!(matches!(
        lookup(&n, "\\knowndlls", 0),
        Err(LookupError::Status(STATUS_OBJECT_NAME_NOT_FOUND))
    ));
    assert!(lookup(&n, "\\knowndlls", 0x40).is_ok());
    assert_eq!(
        lookup(&n, "\\Other\\x", 0).unwrap_err(),
        LookupError::Unselected
    );
    assert_eq!(
        lookup(&n, "\\Absent\\x", 0).unwrap_err(),
        LookupError::Status(STATUS_OBJECT_PATH_NOT_FOUND)
    );
    assert_eq!(
        lookup(&n, "\\KnownDlls\\ntdll.dll\\x", 0).unwrap_err(),
        LookupError::Status(STATUS_OBJECT_TYPE_MISMATCH)
    );
    let forced = Namespace::selected(table(), selected(true)).unwrap();
    assert!(lookup(&forced, "\\knowndlls", 0).is_ok());
    let mut bad = selected(true);
    let collision = bad[0].entries[0].clone();
    bad[0].entries.push(collision);
    assert!(Namespace::selected(table(), bad).is_err());
    let mut bad = selected(true);
    bad[1].path = "\\Outside".encode_utf16().collect();
    assert!(Namespace::selected(table(), bad).is_err());
    let mut bad = selected(true);
    bad[0].entries[0].kind = "Section".encode_utf16().collect();
    assert!(Namespace::selected(table(), bad).is_err());
    assert!(Namespace::selected(Arc::from([0u16]), selected(true)).is_err());
    let mut bad = selected(true);
    bad[1].entries[0].name = vec![65; MAX_UNITS + 1];
    assert!(Namespace::selected(table(), bad).is_err());
    let mut bad = selected(true);
    bad[1].entries[0].target = Some(vec![65]);
    assert!(Namespace::selected(table(), bad).is_err());
}

#[test]
fn directory_catalog_system_root_and_subject_capture_are_bounded() {
    let mut n = Namespace::selected(table(), selected(true)).unwrap();
    n.select_wow_system_root(&"C:\\WINDOWS".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    assert_eq!(
        n.wow_system_prefix,
        "\\??\\C:\\WINDOWS".encode_utf16().collect::<Vec<_>>()
    );
    let before = n.wow_system_prefix.clone();
    for bad in [vec![], vec![0], vec![65; 32753]] {
        assert!(n.select_wow_system_root(&bad).is_err());
        assert_eq!(n.wow_system_prefix, before);
    }
    let mut selected = selected(true);
    selected[0].system_security_status = STATUS_ACCESS_DENIED;
    let n = Namespace::selected(table(), selected).unwrap();
    assert_eq!(
        lookup(&n, "\\", 0).unwrap().grant(0x01000000, false),
        Err(STATUS_ACCESS_DENIED)
    );
    let mut bad = super::tests::selected(true);
    bad[0].system_security_status = STATUS_INVALID_HANDLE;
    assert!(Namespace::selected(table(), bad).is_err());
}

#[test]
fn symbolic_link_selection_refuses_uncaptured_grants_and_invalid_metadata() {
    let mut selected = selected(true);
    let entry = CapturedEntry {
        name: "KnownDllPath".encode_utf16().collect(),
        kind: "SymbolicLink".encode_utf16().collect(),
        target: Some("C:\\WINDOWS\\System32".encode_utf16().collect()),
        link_access: None,
    };
    selected[1].entries.push(entry);
    let name: Vec<_> = "\\KnownDlls\\KnownDllPath".encode_utf16().collect();
    let n = Namespace::selected(table(), selected.clone()).unwrap();
    assert_eq!(
        n.lookup_link(None, &name, 0x40).unwrap_err(),
        LookupError::Unselected
    );
    for access in [
        LinkAccess {
            maximum_access: 2,
            system_security_status: STATUS_PRIVILEGE_NOT_HELD,
        },
        LinkAccess {
            maximum_access: 1,
            system_security_status: STATUS_INVALID_HANDLE,
        },
    ] {
        let mut bad = selected.clone();
        bad[1].entries.last_mut().unwrap().link_access = Some(access);
        assert!(Namespace::selected(table(), bad).is_err());
    }
    let mut bad = selected;
    let link = bad[1].entries.last_mut().unwrap();
    link.target = None;
    link.link_access = Some(LinkAccess {
        maximum_access: 1,
        system_security_status: STATUS_PRIVILEGE_NOT_HELD,
    });
    assert!(Namespace::selected(table(), bad).is_err());
}
