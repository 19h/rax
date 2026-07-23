//! Strict-lift regressions for AMD SSE4A EXTRQ/INSERTQ.

use super::*;

#[test]
fn strict_lifter_accepts_all_sse4a_bitfield_forms_and_exact_lengths() {
    for (name, bytes) in [
        ("EXTRQ immediate", &[0x66, 0x0F, 0x78, 0xC1, 0x08, 0x04][..]),
        ("EXTRQ register", &[0x66, 0x0F, 0x79, 0xCA][..]),
        (
            "INSERTQ immediate",
            &[0xF2, 0x0F, 0x78, 0xCA, 0x08, 0x10][..],
        ),
        ("INSERTQ register", &[0xF2, 0x0F, 0x79, 0xCA][..]),
        ("extended XMM", &[0x66, 0x45, 0x0F, 0x79, 0xCA][..]),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(!result.ops.is_empty(), "{name}");
    }
}

#[test]
fn strict_lifter_terminalizes_reserved_sse4a_shapes_without_fallback() {
    for (name, bytes, expected_len) in [
        ("EXTRQ memory", &[0x66, 0x0F, 0x79, 0x00][..], 4),
        ("INSERTQ memory", &[0xF2, 0x0F, 0x79, 0x00][..], 4),
        (
            "EXTRQ immediate nonzero group",
            &[0x66, 0x0F, 0x78, 0xC9, 1, 2][..],
            4,
        ),
        ("prefix-free", &[0x0F, 0x79, 0xC1][..], 2),
        ("REP", &[0xF3, 0x0F, 0x79, 0xC1][..], 3),
        ("LOCK", &[0xF0, 0x66, 0x0F, 0x79, 0xC1][..], 4),
        ("REX2", &[0x66, 0xD5, 0x00, 0x0F, 0x79, 0xC1][..], 4),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, expected_len);
    }
}

#[test]
fn strict_lifter_reports_both_missing_immediate_bytes_precisely() {
    for (bytes, have, need) in [
        (&[0x66, 0x0F, 0x78, 0xC0][..], 4, 5),
        (&[0x66, 0x0F, 0x78, 0xC0, 0x08][..], 5, 6),
        (&[0xF2, 0x0F, 0x78, 0xC1][..], 4, 5),
        (&[0xF2, 0x0F, 0x78, 0xC1, 0x08][..], 5, 6),
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::Incomplete {
                    have: got_have,
                    need: got_need,
                    ..
                }) if got_have == have && got_need == need
            ),
            "bytes={bytes:02X?}"
        );
    }
}
