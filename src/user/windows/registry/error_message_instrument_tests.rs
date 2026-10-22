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

#[test]
fn native_error_message_instrument_registry_absence_is_confined_to_selected_root() {
    let original = registry();
    let selected = original
        .clone()
        .with_tree(TreeRoot::ErrorMessageInstrument, None)
        .unwrap();
    for suffix in ["", "\\", "\\\\child\\nested\\"] {
        let path = format!("{ERROR_MESSAGE_INSTRUMENT_KEY}{suffix}");
        assert!(matches!(original.lookup(&units(&path)), Lookup::Unselected));
        assert!(matches!(selected.lookup(&units(&path)), Lookup::Missing));
        assert!(matches!(
            selected.lookup(&units(&path.to_ascii_lowercase())),
            Lookup::Missing
        ));
    }
    for suffix in ["Suffix", "/child", "\0\\child"] {
        assert!(matches!(
            selected.lookup(&units(&format!("{ERROR_MESSAGE_INSTRUMENT_KEY}{suffix}"))),
            Lookup::Unselected
        ));
    }
    for path in [
        "\\Registry\\Machine\\System\\CurrentControlSet\\Control",
        "\\Registry\\Machine\\System\\CurrentControlSet\\Control\\Error Message",
        "\\Registry\\Machine\\System\\CurrentControlSet\\Control\\Error Message Instruments",
        NLS_SORTING_VERSIONS_KEY,
    ] {
        assert!(matches!(selected.lookup(&units(path)), Lookup::Unselected));
    }
}

#[test]
fn native_error_message_instrument_registry_present_tree_owns_raw_data_and_descendants() {
    let registry = registry()
        .with_tree(
            TreeRoot::ErrorMessageInstrument,
            Some(KeyTree {
                values: vec![],
                children: vec![(
                    units("literal.child"),
                    KeyTree {
                        values: vec![Value {
                            name: units("Raw"),
                            kind: 3,
                            data: vec![0, 255, 17],
                        }],
                        children: vec![],
                    },
                )],
            }),
        )
        .unwrap();
    let root = registry.key(&units(ERROR_MESSAGE_INSTRUMENT_KEY)).unwrap();
    let child = registry
        .key(&units(&format!(
            "{ERROR_MESSAGE_INSTRUMENT_KEY}\\LITERAL.CHILD\\"
        )))
        .unwrap();
    assert_eq!(root.children, 1);
    assert!(Arc::ptr_eq(&root.upcase, &child.upcase));
    drop(registry);
    drop(root);
    let value = child.value(&units("RAW")).unwrap();
    assert_eq!((value.kind, value.data.as_slice()), (3, &[0, 255, 17][..]));
    assert!(matches!(child.relative(&units("absent")), Lookup::Missing));
}

#[test]
fn native_error_message_instrument_registry_selection_rejects_duplicates_and_excess_values() {
    assert!(
        Registry::default()
            .with_tree(TreeRoot::ErrorMessageInstrument, None)
            .is_err()
    );
    let selected = registry()
        .with_tree(TreeRoot::ErrorMessageInstrument, None)
        .unwrap();
    assert!(
        selected
            .with_tree(TreeRoot::ErrorMessageInstrument, Some(KeyTree::default()))
            .is_err()
    );
    let full = registry()
        .with_ifeo(KeyTree {
            values: (0..MAX_VALUES)
                .map(|i| Value {
                    name: units(&i.to_string()),
                    kind: 3,
                    data: vec![],
                })
                .collect(),
            children: vec![],
        })
        .unwrap();
    assert!(
        full.with_tree(
            TreeRoot::ErrorMessageInstrument,
            Some(KeyTree {
                values: vec![Value {
                    name: units("extra"),
                    kind: 3,
                    data: vec![1]
                }],
                children: vec![],
            })
        )
        .is_err()
    );
}
