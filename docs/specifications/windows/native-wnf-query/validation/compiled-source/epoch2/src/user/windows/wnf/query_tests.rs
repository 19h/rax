use super::query::*;
use super::tests::hex;
use crate::user::windows::nt::status::*;
use crate::user::windows::registry::Value;
use serde_json::Value as Json;
use std::{io, sync::Arc};

fn values() -> Vec<Value> {
    let raw: Json = serde_json::from_str(include_str!("../../../../docs/specifications/windows/native-wnf-subscribe/native-wnf-subscribe-registry.json")).unwrap();
    raw.as_array()
        .unwrap()
        .iter()
        .map(|row| Value {
            name: row["name"].as_str().unwrap().encode_utf16().collect(),
            kind: 3,
            data: hex(row["data"].as_str().unwrap()),
        })
        .collect()
}
fn definition(name: u64, maximum: u32) -> Definition {
    Definition {
        name,
        maximum,
        type_id: None,
    }
}
const SYSTEM: u64 = 0x0280_032E_A3BC_0875;
const PROCESS: u64 = 0x4191_1A28_A3BC_10B5;

#[test]
fn query_provisioning_parses_every_native_record_and_the_single_typed_tail() {
    let all = values();
    assert_eq!(all.len(), 1613);
    let definitions = all
        .iter()
        .map(Definition::parse)
        .collect::<io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        definitions.iter().filter(|d| d.type_id.is_some()).count(),
        1
    );
    let typed = definitions.iter().find(|d| d.name == SYSTEM).unwrap();
    assert_eq!(typed.maximum, 8);
    assert_eq!(
        typed.type_id,
        Some(hex("6364f948d652854e9c02f2c14e17264e").try_into().unwrap())
    );
    let process = definitions.iter().find(|d| d.name == PROCESS).unwrap();
    assert_eq!(process.maximum, 0);
    assert_eq!(process.type_id, None);
}

#[test]
fn query_provisioning_refuses_malformed_offsets_sid_extents_and_unknown_trailers() {
    let value = values()
        .into_iter()
        .find(|v| String::from_utf16(&v.name).unwrap() == "0280032EA3BC0875")
        .unwrap();
    for length in 0..value.data.len() {
        let mut short = value.clone();
        short.data.truncate(length);
        // Removing exactly the optional GUID leaves the other admitted format.
        if length == value.data.len() - 16 {
            assert!(Definition::parse(&short).is_ok());
        } else {
            assert!(Definition::parse(&short).is_err(), "length {length}");
        }
    }
    for offset in [1u32, 16, u32::MAX, 0xFFFF_FFFC] {
        let mut broken = value.clone();
        broken.data[4..8].copy_from_slice(&offset.to_le_bytes());
        assert!(Definition::parse(&broken).is_err(), "offset {offset}");
    }
    let mut oversized = value.clone();
    oversized.data.resize(MAX_STATE_BYTES + 1, 0);
    assert!(Definition::parse(&oversized).is_err());
    let mut kind = value.clone();
    kind.kind = 1;
    assert!(Definition::parse(&kind).is_err());
    let mut name = value;
    name.name = "0000000000000000".encode_utf16().collect();
    assert!(Definition::parse(&name).is_err());
}

#[test]
fn query_capture_bounds_retries_and_never_imports_host_process_payload() {
    let mut calls = 0;
    let stable = capture(vec![definition(SYSTEM, 8)], 0, |_, bytes| {
        calls += 1;
        bytes.copy_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        (STATUS_SUCCESS, 9, 8)
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(
        stable.get(SYSTEM).unwrap().1,
        Record::Data(Data {
            stamp: 9,
            bytes: Arc::from([1, 0, 0, 0, 0, 0, 0, 0])
        })
    );
    let mut calls = 0;
    let changing = capture(vec![definition(SYSTEM, 8)], 0, |_, _| {
        calls += 1;
        (STATUS_SUCCESS, calls, 0)
    })
    .unwrap();
    assert_eq!(calls, 6);
    assert!(matches!(
        changing.get(SYSTEM).unwrap().1,
        Record::Unavailable(_)
    ));
    let private = capture(vec![definition(PROCESS, 32)], 0, |_, bytes| {
        assert!(bytes.is_empty());
        (STATUS_BUFFER_TOO_SMALL, 99, 16)
    })
    .unwrap();
    assert_eq!(
        private.get(PROCESS).unwrap().1,
        Record::Data(Data {
            stamp: 0,
            bytes: Arc::from([])
        })
    );
    for status in [STATUS_ACCESS_DENIED, STATUS_OBJECT_NAME_NOT_FOUND] {
        let q = capture(vec![definition(SYSTEM, 8)], 0, |_, _| (status, 0, 8)).unwrap();
        assert_eq!(q.get(SYSTEM).unwrap().1, Record::Status(status));
    }
    let unavailable = capture(vec![definition(SYSTEM, u32::MAX)], 0, |_, _| {
        panic!("oversized capture must not call host")
    })
    .unwrap();
    assert!(matches!(
        unavailable.get(SYSTEM).unwrap().1,
        Record::Unavailable(_)
    ));
}

#[test]
fn query_catalog_rejects_duplicate_invalid_and_over_budget_data() {
    let row = (
        definition(SYSTEM, 8),
        Record::Data(Data {
            stamp: 1,
            bytes: Arc::from([0u8; 8]),
        }),
    );
    assert!(Queries::selected(vec![row.clone(), row], 0).is_err());
    assert!(
        Queries::selected(
            vec![(definition(0, 0), Record::Status(STATUS_ACCESS_DENIED))],
            0
        )
        .is_err()
    );
    assert!(
        Queries::selected(
            vec![(
                definition(SYSTEM, 0),
                Record::Data(Data {
                    stamp: 1,
                    bytes: Arc::from([1])
                })
            )],
            0
        )
        .is_err()
    );
    assert!(
        Queries::selected(
            vec![(
                definition(SYSTEM, 0),
                Record::Status(STATUS_INVALID_PARAMETER)
            )],
            0
        )
        .is_err()
    );
    let mut definitions = Vec::new();
    for i in 0..257 {
        definitions.push(definition(
            SYSTEM ^ ((i as u64) << 11),
            MAX_STATE_BYTES as u32,
        ));
    }
    let mut calls = 0;
    assert!(
        capture(definitions, 0, |_, bytes| {
            calls += 1;
            (STATUS_SUCCESS, 0, bytes.len())
        })
        .is_err()
    );
    assert_eq!(calls, 514);
}

#[cfg(windows)]
#[test]
fn native_query_snapshot_captures_typed_state_and_initializes_private_process_state() {
    let selected = super::snapshot().unwrap();
    let q = selected.queries().unwrap();
    let (definition, record) = q.get(SYSTEM).unwrap();
    assert_eq!(definition.maximum, 8);
    assert!(definition.type_id.is_some());
    assert_eq!(
        *record,
        Record::Data(Data {
            stamp: 1,
            bytes: Arc::from([1, 0, 0, 0, 0, 0, 0, 0])
        })
    );
    assert_eq!(
        q.get(PROCESS).unwrap().1,
        Record::Data(Data {
            stamp: 0,
            bytes: Arc::from([])
        })
    );
    assert!(selected.primary_subject().is_some());
}

#[cfg(windows)]
#[test]
fn native_query_snapshot_refuses_an_owned_impersonation_scope_and_reverts() {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn ImpersonateSelf(level: u32) -> i32;
        fn RevertToSelf() -> i32;
    }
    assert!(super::installed_query::ensure_primary_thread().is_ok());
    struct Revert(bool);
    impl Drop for Revert {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    RevertToSelf();
                }
            }
        }
    }
    // The test owns this test-runner thread; never alter another thread token.
    assert_ne!(unsafe { ImpersonateSelf(2) }, 0);
    let mut guard = Revert(true);
    let error = super::snapshot().unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(error.to_string().contains("impersonation"));
    assert_ne!(unsafe { RevertToSelf() }, 0);
    guard.0 = false;
    assert!(super::installed_query::ensure_primary_thread().is_ok());
}
