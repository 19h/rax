use super::*;

#[test]
fn bno_model_preserves_disabled_empty_unicode_and_raw_utf16() {
    let disabled = Isolation::new(None).unwrap();
    assert_eq!(disabled.prefix(), None);
    assert_eq!(disabled.required_bytes(4), Some(8));
    assert_eq!(disabled.required_bytes(8), Some(16));
    assert_eq!(disabled.required_bytes(16), None);
    for prefix in [
        vec![],
        vec![0x41, 0x03a9, 0x4e2d],
        vec![0xd800, 0xdc00, 0xdfff],
    ] {
        let value = Isolation::new(Some(prefix.clone())).unwrap();
        assert_eq!(value.prefix(), Some(prefix.as_slice()));
        assert_eq!(
            value.required_bytes(4),
            Some(8 + 2 * (prefix.len() as u32 + 1))
        );
        assert_eq!(
            value.required_bytes(8),
            Some(16 + 2 * (prefix.len() as u32 + 1))
        );
    }
    assert!(Isolation::new(Some(vec![65, 0, 66])).is_none());
    assert!(Isolation::new(Some(vec![65; MAX_PREFIX_UNITS])).is_some());
    assert!(Isolation::new(Some(vec![65; MAX_PREFIX_UNITS + 1])).is_none());
}

fn native(width: usize, prefix: Option<&[u16]>) -> Vec<u8> {
    let header = width * 2;
    let mut bytes = vec![0xa5; header];
    let pointer = if prefix.is_some() {
        0x1000u64 + header as u64
    } else {
        0
    };
    bytes[..width].copy_from_slice(&pointer.to_le_bytes()[..width]);
    bytes[width] = u8::from(prefix.is_some());
    if let Some(prefix) = prefix {
        for unit in prefix.iter().copied().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
    }
    bytes
}

#[test]
fn bno_native_capture_validates_layout_and_copies_owned_prefix() {
    for width in [4, 8] {
        for prefix in [None, Some(&[][..]), Some(&[65, 0x03a9, 0x4e2d][..])] {
            let mut bytes = native(width, prefix);
            let captured = Isolation::from_native(&bytes, 0x1000, width).unwrap();
            bytes.fill(0);
            assert_eq!(captured.prefix(), prefix);
        }
        let good = native(width, Some(&[65, 66]));
        for length in 0..good.len() {
            assert!(Isolation::from_native(&good[..length], 0x1000, width).is_none());
        }
        for pointer in [
            0u64,
            1,
            0x1000,
            0x1001,
            0x1000 + good.len() as u64,
            u64::MAX,
        ] {
            let mut bad = good.clone();
            bad[..width].copy_from_slice(&pointer.to_le_bytes()[..width]);
            assert!(Isolation::from_native(&bad, 0x1000, width).is_none());
        }
        let mut bad = good.clone();
        bad[width] = 2;
        assert!(Isolation::from_native(&bad, 0x1000, width).is_none());
        let mut bad = good.clone();
        let n = bad.len();
        bad[n - 2..].copy_from_slice(&65u16.to_le_bytes());
        assert!(Isolation::from_native(&bad, 0x1000, width).is_none());
        let mut bad = native(width, None);
        bad[width] = 1;
        assert!(Isolation::from_native(&bad, 0x1000, width).is_none());
        let mut bad = good.clone();
        bad[width] = 0;
        assert!(Isolation::from_native(&bad, 0x1000, width).is_none());
        let mut bad = good.clone();
        bad.push(0);
        assert!(Isolation::from_native(&bad, 0x1000, width).is_none());
        assert!(Isolation::from_native(&good, u64::MAX - 4, width).is_none());
    }
}
