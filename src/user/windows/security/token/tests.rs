use super::*;

#[test]
fn native_process_token_profile_rejects_unknown_capture_shapes() {
    for maximum in [0, 0x200, 0x0010_0000, 0x0100_0000, u32::MAX] {
        assert_eq!(Access::new(maximum, STATUS_SUCCESS), None);
    }
    assert_eq!(Access::new(8, 0xC000_0008), None);
    for status in [
        STATUS_SUCCESS,
        STATUS_ACCESS_DENIED,
        STATUS_PRIVILEGE_NOT_HELD,
    ] {
        assert!(Access::new(8, status).is_some());
    }
}

#[test]
fn native_process_token_profile_preserves_measured_generic_mappings() {
    let access = Access::new(ALL_ACCESS, STATUS_PRIVILEGE_NOT_HELD).unwrap();
    for (desired, granted) in [
        (0x8000_0000, 0x0002_001A),
        (0x4000_0000, 0x0002_01E0),
        (0x2000_0000, 0x0002_0005),
        (0x1000_0000, ALL_ACCESS),
        (MAXIMUM_ALLOWED, ALL_ACCESS),
        (MAXIMUM_ALLOWED | 8, ALL_ACCESS),
    ] {
        assert_eq!(access.grant(desired), Ok(granted));
    }
    assert_eq!(access.grant(0), Err(STATUS_ACCESS_DENIED));
    assert_eq!(access.grant(0x0010_0000), Err(STATUS_ACCESS_DENIED));
}

#[test]
fn native_process_token_profile_orders_illegal_bits_privilege_and_grants() {
    let access = Access::new(8, STATUS_PRIVILEGE_NOT_HELD).unwrap();
    assert_eq!(access.grant(0x0500_0008), Err(STATUS_ACCESS_DENIED));
    assert_eq!(access.grant(0x0100_0200), Err(STATUS_PRIVILEGE_NOT_HELD));
    assert_eq!(access.grant(0x0000_0200), Err(STATUS_ACCESS_DENIED));
    assert_eq!(access.grant(0x8000_0000), Err(STATUS_ACCESS_DENIED));
    assert_eq!(access.grant(MAXIMUM_ALLOWED), Ok(8));
    let access = Access::new(8, STATUS_SUCCESS).unwrap();
    assert_eq!(
        access.grant(SYSTEM_SECURITY | MAXIMUM_ALLOWED),
        Ok(SYSTEM_SECURITY | 8)
    );
    assert_eq!(
        access.grant(SYSTEM_SECURITY | 0x200),
        Err(STATUS_ACCESS_DENIED)
    );
    let access = Access::new(8, STATUS_ACCESS_DENIED).unwrap();
    assert_eq!(access.grant(SYSTEM_SECURITY | 8), Err(STATUS_ACCESS_DENIED));
}
