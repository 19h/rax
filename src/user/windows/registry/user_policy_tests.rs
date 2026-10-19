use super::*;

fn units(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
fn sid() -> Vec<u8> {
    vec![1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0]
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
fn user_policy_sid_paths_match_all_512_native_format_observations_and_reject_invalid_sids() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/specifications/windows/native-user-policy/sid-text-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["observations"], 512);
    for row in corpus["cases"].as_array().unwrap() {
        let sid = crate::user::windows::wnf::tests::hex(row["sid"].as_str().unwrap());
        let expected = format!(
            "\\Registry\\User\\{}\\Software\\Policies\\Microsoft\\Windows\\Safer\\CodeIdentifiers",
            row["text"].as_str().unwrap()
        );
        assert_eq!(user_policy_path(&sid).unwrap(), units(&expected));
    }
    for invalid in [
        vec![],
        vec![1; 7],
        vec![2, 0, 0, 0, 0, 0, 0, 5],
        vec![1, 16, 0, 0, 0, 0, 0, 5],
        {
            let mut value = sid();
            value.push(0);
            value
        },
    ] {
        assert!(user_policy_path(&invalid).is_err());
        assert!(registry().with_user_policy(&invalid, None).is_err());
    }
}

#[test]
fn user_policy_absence_is_bound_to_one_primary_sid_and_fixed_suffix() {
    let path = user_policy_path(&sid()).unwrap();
    let old = registry();
    assert!(matches!(old.lookup(&path), Lookup::Unselected));
    let selected = old.with_user_policy(&sid(), None).unwrap();
    for tail in ["", "\\", "\\missing\\nested", "\\.", "\\.."] {
        let name: Vec<_> = path.iter().copied().chain(tail.encode_utf16()).collect();
        assert!(matches!(selected.lookup(&name), Lookup::Missing));
    }
    let lower: Vec<_> = path
        .iter()
        .map(|&u| if (65..=90).contains(&u) { u + 32 } else { u })
        .collect();
    assert!(matches!(selected.lookup(&lower), Lookup::Missing));
    for outside in [
        "\\Registry\\User\\S-1-5-19\\Software\\Policies\\Microsoft\\Windows\\Safer\\CodeIdentifiers",
        "\\Registry\\User\\S-1-5-18\\Software\\Policies\\Microsoft\\Windows\\Safer",
        "\\Registry\\User\\S-1-5-18\\Software\\Policies\\Microsoft\\Windows\\Safer\\CodeIdentifiersSuffix",
    ] {
        assert!(matches!(
            selected.lookup(&units(outside)),
            Lookup::Unselected
        ));
    }
    assert!(selected.clone().with_user_policy(&sid(), None).is_err());
    assert!(Registry::default().with_user_policy(&sid(), None).is_err());
}

#[test]
fn user_policy_present_tree_retains_values_lifetime_and_shared_six_root_budget() {
    let selected = registry()
        .with_user_policy(
            &sid(),
            Some(KeyTree {
                values: vec![value("Root")],
                children: vec![(
                    units("child"),
                    KeyTree {
                        values: vec![value("Raw")],
                        children: vec![],
                    },
                )],
            }),
        )
        .unwrap();
    let root = selected.key(&user_policy_path(&sid()).unwrap()).unwrap();
    assert_eq!(root.value(&units("root")).unwrap().data, [0, 255, 17]);
    let Lookup::Present(child) = root.relative(&units("CHILD")) else {
        panic!("selected child")
    };
    drop(selected);
    drop(root);
    assert_eq!(child.value(&units("raw")).unwrap().data, [0, 255, 17]);
    let mut full = registry()
        .with_ifeo(KeyTree {
            values: (0..MAX_VALUES - 5)
                .map(|i| value(&format!("v{i}")))
                .collect(),
            children: vec![],
        })
        .unwrap();
    for root in [
        TreeRoot::RedirectionKeys,
        TreeRoot::SafeBootOption,
        TreeRoot::SrpDll,
        TreeRoot::SaferCodeIdentifiers,
    ] {
        full = full
            .with_tree(
                root,
                Some(KeyTree {
                    values: vec![value("other")],
                    children: vec![],
                }),
            )
            .unwrap();
    }
    let one = KeyTree {
        values: vec![value("last")],
        children: vec![],
    };
    assert_eq!(
        full.clone()
            .with_user_policy(&sid(), Some(one))
            .unwrap()
            .value_budget()
            .unwrap()
            .count,
        MAX_VALUES
    );
    assert!(
        full.with_user_policy(
            &sid(),
            Some(KeyTree {
                values: vec![value("a"), value("b")],
                children: vec![]
            })
        )
        .is_err()
    );
}
