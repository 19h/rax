use super::*;
use crate::user::windows::registry::Value;
use std::sync::{Arc, OnceLock};

pub(crate) const NAME: u64 = 0x0280_032E_A3BC_0875;
pub(crate) fn cases() -> &'static serde_json::Value {
    static DATA: OnceLock<serde_json::Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../docs/specifications/windows/native-wnf-system-subscribe/cases.json"
        ))
        .unwrap()
    })
}
pub(crate) fn value(index: usize) -> Value {
    let mut data = super::tests::hex(cases()["descriptors"][index].as_str().unwrap());
    data.extend_from_slice(&64u32.to_le_bytes());
    Value {
        name: format!("{NAME:016X}").encode_utf16().collect(),
        kind: 3,
        data,
    }
}
pub(crate) fn select(index: usize, record: Record) -> Catalog {
    let value = value(index);
    let definition = Definition::parse(&value).unwrap();
    let subject = (*super::tests::catalog().primary_subject().unwrap()).clone();
    Catalog::selected(vec![value], subject)
        .unwrap()
        .with_queries(Queries::selected(vec![(definition, record)], 0).unwrap())
}
pub(crate) fn data(index: usize, stamp: u32) -> Catalog {
    select(
        index,
        Record::Data(Data {
            stamp,
            bytes: if stamp == 0 {
                Arc::from([])
            } else {
                Arc::from([1; 8])
            },
        }),
    )
}

#[test]
fn native_system_wnf_permissions_match_3456_recorded_calls() {
    let catalogs: Vec<_> = (0..cases()["descriptors"].as_array().unwrap().len())
        .map(|i| data(i, 1))
        .collect();
    let mut count = 0;
    for (profile, recorded) in cases()["permissions"].as_object().unwrap() {
        let rows = recorded.as_array().unwrap();
        assert_eq!(rows.len(), if profile == "arm64-deny" { 384 } else { 768 });
        for row in rows {
            let index: usize = row["descriptor"].as_str().unwrap().parse().unwrap();
            let mask = u32::from_str_radix(row["mask"].as_str().unwrap(), 16).unwrap();
            let expected = match row["status"].as_str().unwrap() {
                "00000000" => Permission::Allowed,
                "C0000022" => Permission::Denied,
                status => panic!("unmodeled native status {status}"),
            };
            assert_eq!(
                catalogs[index].permission(NAME, mask),
                expected,
                "{profile}: {row}"
            );
            count += 1;
        }
    }
    assert_eq!(count, 3456);
}

#[test]
fn native_system_wnf_actual_typed_descriptor_requires_selected_queries() {
    let value = super::tests::values()
        .into_iter()
        .find(|v| v.name == format!("{NAME:016X}").encode_utf16().collect::<Vec<_>>())
        .unwrap();
    let definition = Definition::parse(&value).unwrap();
    let subject = (*super::tests::catalog().primary_subject().unwrap()).clone();
    let catalog = Catalog::selected(vec![value], subject).unwrap();
    assert_eq!(catalog.permission(NAME, 0x11), Permission::Unmodeled);
    let catalog = catalog.with_queries(
        Queries::selected(
            vec![(
                definition,
                Record::Data(Data {
                    stamp: 1,
                    bytes: Arc::from([1, 0, 0, 0, 0, 0, 0, 0]),
                }),
            )],
            0,
        )
        .unwrap(),
    );
    assert_eq!(catalog.permission(NAME, 0x11), Permission::Allowed);
    assert_eq!(catalog.permission(NAME, 2), Permission::Denied);
    assert_eq!(catalog.subscription_stamp(NAME), Ok(Some(1)));
}

#[test]
fn native_system_wnf_unknown_descriptors_and_subjects_remain_unavailable() {
    let original = value(7);
    let subject = (*super::tests::catalog().primary_subject().unwrap()).clone();
    for offset in [0, 1, 2, 4, 8, 12, 16] {
        let mut changed = original.clone();
        changed.data[offset] ^= 0x80;
        let selected = Catalog::selected(vec![changed], subject.clone()).unwrap();
        let definition = Definition::parse(&original).unwrap();
        let selected = selected.with_queries(
            Queries::selected(
                vec![(
                    definition,
                    Record::Data(Data {
                        stamp: 0,
                        bytes: Arc::from([]),
                    }),
                )],
                0,
            )
            .unwrap(),
        );
        assert_eq!(
            selected.permission(NAME, 1),
            Permission::Unmodeled,
            "offset {offset}"
        );
    }
    for restricted in [false, true] {
        let mut changed = subject.clone();
        if restricted {
            changed.restricted.push(changed.groups[0].clone());
        } else {
            changed.app_container = true;
        }
        let selected = Catalog::selected(vec![original.clone()], changed)
            .unwrap()
            .with_queries(
                Queries::selected(
                    vec![(
                        Definition::parse(&original).unwrap(),
                        Record::Data(Data {
                            stamp: 0,
                            bytes: Arc::from([]),
                        }),
                    )],
                    0,
                )
                .unwrap(),
            );
        assert_eq!(selected.permission(NAME, 0), Permission::Unmodeled);
    }
    let mut owner_rights = value(4);
    let dacl = u32::from_le_bytes(owner_rights.data[16..20].try_into().unwrap()) as usize;
    owner_rights.data[dacl + 16..dacl + 28].copy_from_slice(&[1, 1, 0, 0, 0, 0, 0, 3, 4, 0, 0, 0]);
    assert_eq!(
        super::system::grants(&owner_rights.data[..owner_rights.data.len() - 4], &subject),
        None
    );
    let mut execute = value(4);
    let dacl = u32::from_le_bytes(execute.data[16..20].try_into().unwrap()) as usize;
    execute.data[dacl + 12..dacl + 16].copy_from_slice(&0x2000_0000u32.to_le_bytes());
    assert_eq!(
        super::system::grants(&execute.data[..execute.data.len() - 4], &subject),
        None
    );
}

#[test]
fn native_system_wnf_query_admission_is_not_replaced_by_empty_data() {
    use crate::user::windows::nt::status::*;
    assert_eq!(
        select(
            6,
            Record::Data(Data {
                stamp: 1,
                bytes: Arc::from([])
            })
        )
        .subscription_stamp(NAME),
        Ok(Some(1))
    );
    assert_eq!(
        select(4, Record::Status(STATUS_OBJECT_NAME_NOT_FOUND)).subscription_stamp(NAME),
        Ok(None)
    );
    assert!(
        select(4, Record::Status(STATUS_ACCESS_DENIED))
            .subscription_stamp(NAME)
            .is_err()
    );
    assert!(
        select(4, Record::Unavailable("unstable selection"))
            .subscription_stamp(NAME)
            .is_err()
    );
    assert!(
        select(
            4,
            Record::Data(Data {
                stamp: 0,
                bytes: Arc::from([1])
            })
        )
        .subscription_stamp(NAME)
        .is_err()
    );
}
