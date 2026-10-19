//! Independent native byte fixtures and bounded pointer-graph rejection.
use super::{Attribute, Attributes, MAX_CAPTURE, Values};

pub(crate) fn fixture(kind: u16) -> Attribute {
    let values = match kind {
        1 => Values::Int64(vec![-1, i64::MIN, i64::MAX]),
        2 => Values::Uint64(vec![0, 0x0123_4567_89AB_CDEF, u64::MAX]),
        3 => Values::String(vec![vec![0x61], vec![0x3A9, 0x20AC]]),
        4 => Values::Fqbn(vec![(
            0x0123_4567_89AB_CDEF,
            "fixture".encode_utf16().collect(),
        )]),
        5 => Values::Sid(vec![vec![1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0]]),
        6 => Values::Boolean(vec![0, 1]),
        16 => Values::Octet(vec![vec![0, 1, 0x7F, 0x80, 0xFF]]),
        _ => panic!("unknown independent fixture type"),
    };
    Attribute {
        name: "Assist.Fixture".encode_utf16().collect(),
        flags: if matches!(kind, 3 | 4) { 2 } else { 0 },
        values,
    }
}

#[test]
fn native_token_security_attributes_decodes_independent_all_type_native_captures() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../docs/specifications/windows/native-token-security-attributes/cases.json"
    ))
    .unwrap();
    let mut decoded = 0;
    for group in corpus["groups"].as_array().unwrap() {
        if group["version"] != "v4b" {
            continue;
        }
        for row in group["cases"].as_array().unwrap() {
            if row["status"] != "00000000" {
                continue;
            }
            let width = row["width"].as_str().unwrap().parse().unwrap();
            let base = u64::from_str_radix(row["output"].as_str().unwrap(), 16).unwrap();
            let count = usize::from_str_radix(row["required"].as_str().unwrap(), 16).unwrap();
            let bytes = crate::user::windows::wnf::tests::hex(row["bytes"].as_str().unwrap());
            let kind = row["type"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                Attributes::from_native(&bytes[..count], base, width),
                Attributes::new(vec![fixture(kind)]),
                "{row}"
            );
            decoded += 1;
        }
    }
    assert_eq!(decoded, 116);
}

#[test]
fn native_token_security_attributes_owned_parser_rejects_escaping_and_oversized_graphs() {
    let attributes = Attributes::new(vec![fixture(4)]).unwrap();
    for width in [4, 8] {
        let packet = attributes
            .pack(&attributes.all(), width, 0x10000, MAX_CAPTURE)
            .unwrap();
        assert_eq!(
            Attributes::from_native(&packet.bytes, 0x10000, width),
            Some(attributes.clone())
        );
        for length in 0..packet.bytes.len() {
            assert!(Attributes::from_native(&packet.bytes[..length], 0x10000, width).is_none());
        }
        for (at, replacement) in [(0, 2u16), (2, 1), (if width == 8 { 34 } else { 22 }, 1)] {
            let mut bytes = packet.bytes.clone();
            bytes[at..at + 2].copy_from_slice(&replacement.to_le_bytes());
            assert!(Attributes::from_native(&bytes, 0x10000, width).is_none());
        }
        let mut bytes = packet.bytes.clone();
        bytes[8..8 + width].fill(0xFF);
        assert!(Attributes::from_native(&bytes, 0x10000, width).is_none());
        let mut bytes = packet.bytes.clone();
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Attributes::from_native(&bytes, 0x10000, width).is_none());
    }
    let mut empty_values = fixture(2);
    empty_values.values = Values::Uint64(vec![]);
    assert!(Attributes::new(vec![empty_values]).is_none());
    let mut invalid = fixture(5);
    invalid.values = Values::Sid(vec![vec![1, 15, 0, 0, 0, 0, 0, 5]]);
    assert!(Attributes::new(vec![invalid]).is_none());
}

#[test]
fn native_token_security_attributes_retains_opaque_flags_and_exact_name_grammar() {
    let mut attribute = fixture(2);
    attribute.flags = 0xFFFF_0041;
    attribute.name = "TSA://ProcUnique".encode_utf16().collect();
    let attributes = Attributes::new(vec![attribute]).unwrap();
    let mut upcase: Vec<_> = (0..=u16::MAX).collect();
    for unit in b'a'..=b'z' {
        upcase[usize::from(unit)] = u16::from(unit - 32);
    }
    assert_eq!(
        attributes
            .find(
                &"tsa://procunique".encode_utf16().collect::<Vec<_>>(),
                &upcase
            )
            .unwrap()
            .flags,
        0xFFFF_0041
    );
    assert!(
        attributes
            .find(
                &"TSA:\\ProcUnique".encode_utf16().collect::<Vec<_>>(),
                &upcase
            )
            .is_none()
    );
    assert!(attributes.find(&[], &upcase).is_none());
    assert!(
        attributes
            .find(&"TSA://ProcUnique".encode_utf16().collect::<Vec<_>>(), &[])
            .is_none()
    );
    assert!(
        attributes
            .pack(&attributes.all(), 4, u64::from(u32::MAX), MAX_CAPTURE)
            .is_none()
    );
    assert!(
        attributes
            .pack(&attributes.all(), 8, u64::MAX, MAX_CAPTURE)
            .is_none()
    );
}
