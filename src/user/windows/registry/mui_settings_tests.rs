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
fn native_mui_settings_absence_is_confined_to_the_selected_root() {
    let original = registry();
    let selected = original
        .clone()
        .with_tree(TreeRoot::MuiSettings, None)
        .unwrap();
    for suffix in ["", "\\", "\\\\Control Panel\\Desktop\\"] {
        let path = format!("{MUI_SETTINGS_KEY}{suffix}");
        assert!(matches!(original.lookup(&units(&path)), Lookup::Unselected));
        assert!(matches!(selected.lookup(&units(&path)), Lookup::Missing));
        assert!(matches!(
            selected.lookup(&units(&path.to_ascii_lowercase())),
            Lookup::Missing
        ));
    }
    for suffix in ["Suffix", "/child", "\0\\child"] {
        assert!(matches!(
            selected.lookup(&units(&format!("{MUI_SETTINGS_KEY}{suffix}"))),
            Lookup::Unselected
        ));
    }
    assert!(matches!(
        selected.lookup(&units(
            "\\Registry\\Machine\\Software\\Policies\\Microsoft\\MUI"
        )),
        Lookup::Unselected
    ));
    assert!(selected.with_tree(TreeRoot::MuiSettings, None).is_err());
}

#[test]
fn native_mui_settings_present_tree_retains_raw_values_and_handle_lifetime() {
    let selected = registry()
        .with_tree(
            TreeRoot::MuiSettings,
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
    let child = selected
        .key(&units(&format!("{MUI_SETTINGS_KEY}\\LITERAL.CHILD\\")))
        .unwrap();
    drop(selected);
    let value = child.value(&units("raw")).unwrap();
    assert_eq!((value.kind, value.data.as_slice()), (3, &[0, 255, 17][..]));
    assert!(matches!(child.relative(&units("absent")), Lookup::Missing));
}
