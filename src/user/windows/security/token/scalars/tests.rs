use super::*;

#[test]
fn native_token_scalar_capture_separates_missing_invalid_class_and_checked_values() {
    assert_eq!(Scalars::default().get(42), None);
    for class in CLASSES {
        assert_eq!(
            width(class),
            match class {
                21 => Some(1),
                45 | 46 => None,
                _ => Some(4),
            }
        );
        let values = Scalars::selected([(class, CapturedScalar::InvalidClass)]).unwrap();
        assert_eq!(values.get(class), Some(CapturedScalar::InvalidClass));
        assert!(
            Scalars::selected([
                (class, CapturedScalar::InvalidClass),
                (class, CapturedScalar::InvalidClass)
            ])
            .is_none()
        );
        let valid: &[u32] = match class {
            21 => &[0, 1, 7, 255],
            45 | 46 => &[],
            _ => &[0, 1, 7, 0x1234_5678, 0x8000_0000, u32::MAX],
        };
        for &value in valid {
            let values = Scalars::selected([(class, CapturedScalar::Value(value))]).unwrap();
            assert_eq!(values.get(class), Some(CapturedScalar::Value(value)));
        }
        for value in [0, 1, 2, 3, 4, 7, u32::MAX] {
            assert_eq!(
                Scalars::selected([(class, CapturedScalar::Value(value))]).is_some(),
                match class {
                    21 => value <= 255,
                    45 | 46 => false,
                    _ => true,
                },
                "{class}/{value}"
            );
        }
    }
    for class in [0, 12, 29, 43, u32::MAX] {
        assert!(!is_class(class));
        assert_eq!(width(class), None);
        assert!(Scalars::selected([(class, CapturedScalar::InvalidClass)]).is_none());
    }
}
