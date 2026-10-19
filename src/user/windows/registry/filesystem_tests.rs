use super::*;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn registry() -> Registry {
    Registry::selected(
        (0..=u16::MAX)
            .map(|u| if (97..=122).contains(&u) { u - 32 } else { u })
            .collect(),
        vec![],
    )
    .unwrap()
}
fn value(name: &str) -> Value {
    Value {
        name: units(name),
        kind: 3,
        data: vec![0, 255, 17],
    }
}

#[test]
fn native_filesystem_registry_absence_is_selected_per_root_and_does_not_admit_siblings() {
    let old = registry().with_absent_ifeo().unwrap();
    let selected = old.clone().with_tree(TreeRoot::FileSystem, None).unwrap();
    for suffix in ["", "\\", "\\\\child\\nested"] {
        let path = units(&format!("{FILESYSTEM_KEY}{suffix}"));
        assert!(matches!(old.lookup(&path), Lookup::Unselected));
        assert!(matches!(selected.lookup(&path), Lookup::Missing));
        assert!(matches!(
            selected.lookup(&units(&format!("{IFEO_KEY}{suffix}"))),
            Lookup::Missing
        ));
    }
    for suffix in ["Suffix", "/child", "\0\\child"] {
        assert!(matches!(
            selected.lookup(&units(&format!("{FILESYSTEM_KEY}{suffix}"))),
            Lookup::Unselected
        ));
    }
    for path in [
        "\\Registry\\Machine\\System\\CurrentControlSet",
        "\\Registry\\Machine\\System\\CurrentControlSet\\Control",
        "\\Registry\\Machine\\System\\CurrentControlSet\\Control\\FileSystemOther",
    ] {
        assert!(matches!(selected.lookup(&units(path)), Lookup::Unselected));
    }
    assert!(matches!(
        selected.lookup(&units(&FILESYSTEM_KEY.to_ascii_lowercase())),
        Lookup::Missing
    ));
}

#[test]
fn native_filesystem_registry_present_tree_keeps_raw_values_and_survives_registry_drop() {
    let registry = registry()
        .with_tree(
            TreeRoot::FileSystem,
            Some(KeyTree {
                values: vec![value("Root")],
                children: vec![(
                    units("child"),
                    KeyTree {
                        values: vec![value("Raw")],
                        children: vec![(units("nested"), KeyTree::default())],
                    },
                )],
            }),
        )
        .unwrap()
        .with_ifeo(KeyTree::default())
        .unwrap();
    let root = registry.key(&units(FILESYSTEM_KEY)).unwrap();
    assert_eq!(root.children, 1);
    assert_eq!(registry.value_budget().unwrap().count, 2);
    let child = registry
        .key(&units(&format!("{FILESYSTEM_KEY}\\CHILD")))
        .unwrap();
    assert_eq!(child.value(&units("RAW")).unwrap().data, [0, 255, 17]);
    assert!(Arc::ptr_eq(&root.upcase, &child.upcase));
    assert!(matches!(
        root.relative(&units("\\\\CHILD\\nested\\")),
        Lookup::Present(_)
    ));
    assert!(matches!(
        root.relative(&units("child\\missing\\nested")),
        Lookup::Missing
    ));
    assert!(matches!(root.relative(&units(".")), Lookup::Missing));
    assert!(registry.key(&units(IFEO_KEY)).is_some());
    drop(registry);
    drop(root);
    assert_eq!(child.value(&units("raw")).unwrap().kind, 3);
    assert!(matches!(child.relative(&[]), Lookup::Present(_)));
}

#[test]
fn native_filesystem_registry_selections_share_value_budget_and_reject_duplicates() {
    assert!(
        Registry::default()
            .with_tree(TreeRoot::FileSystem, None)
            .is_err()
    );
    let absent = registry().with_tree(TreeRoot::FileSystem, None).unwrap();
    assert!(
        absent
            .clone()
            .with_tree(TreeRoot::FileSystem, None)
            .is_err()
    );
    assert!(
        absent
            .with_tree(TreeRoot::FileSystem, Some(KeyTree::default()))
            .is_err()
    );
    let full = registry()
        .with_ifeo(KeyTree {
            values: (0..MAX_VALUES - 6)
                .map(|i| value(&format!("v{i}")))
                .collect(),
            children: vec![],
        })
        .unwrap()
        .with_tree(
            TreeRoot::RedirectionKeys,
            Some(KeyTree {
                values: vec![value("other-root")],
                children: vec![],
            }),
        )
        .unwrap();
    let full = full
        .with_tree(
            TreeRoot::SafeBootOption,
            Some(KeyTree {
                values: vec![value("boot-root")],
                children: vec![],
            }),
        )
        .unwrap();
    let full = full
        .with_tree(
            TreeRoot::SrpDll,
            Some(KeyTree {
                values: vec![value("srp-root")],
                children: vec![],
            }),
        )
        .unwrap();
    let full = full
        .with_tree(
            TreeRoot::SaferCodeIdentifiers,
            Some(KeyTree {
                values: vec![value("machine-policy")],
                children: vec![],
            }),
        )
        .unwrap()
        .with_user_policy(
            &[1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0],
            Some(KeyTree {
                values: vec![value("user-policy")],
                children: vec![],
            }),
        )
        .unwrap();
    let one = KeyTree {
        values: vec![value("last")],
        children: vec![],
    };
    assert_eq!(
        full.clone()
            .with_tree(TreeRoot::FileSystem, Some(one))
            .unwrap()
            .value_budget()
            .unwrap()
            .count,
        MAX_VALUES
    );
    let two = KeyTree {
        values: vec![value("a"), value("b")],
        children: vec![],
    };
    assert!(full.with_tree(TreeRoot::FileSystem, Some(two)).is_err());
    let duplicate = KeyTree {
        values: vec![value("a"), value("A")],
        children: vec![],
    };
    assert!(
        registry()
            .with_tree(TreeRoot::FileSystem, Some(duplicate))
            .is_err()
    );
    let duplicate_children = KeyTree {
        values: vec![],
        children: vec![
            (units("a"), KeyTree::default()),
            (units("A"), KeyTree::default()),
        ],
    };
    assert!(
        registry()
            .with_tree(TreeRoot::FileSystem, Some(duplicate_children))
            .is_err()
    );
}
