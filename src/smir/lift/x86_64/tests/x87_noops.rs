//! Strict-lifting coverage for obsolete x87 no-operation encodings.

use super::*;

#[test]
fn obsolete_x87_control_encodings_lift_as_exact_no_operations() {
    for (bytes, name) in [
        (&[0xDB, 0xE0][..], "FENI8087_NOP"),
        (&[0xDB, 0xE1][..], "FDISI8087_NOP"),
        (&[0xDB, 0xE4][..], "FSETPM287_NOP"),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| {
            panic!("{name} must lift without an interpreter frontier: {error}")
        });
        assert_eq!(result.bytes_consumed, 2, "{name}");
        assert!(result.ops.is_empty(), "{name}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(result.branch_targets.is_empty(), "{name}");
    }
}

#[test]
fn obsolete_x87_no_operations_do_not_hide_invalid_or_unsupported_neighbors() {
    for bytes in [
        &[0xF0, 0xDB, 0xE0][..],
        &[0xF0, 0xDB, 0xE1][..],
        &[0xF0, 0xDB, 0xE4][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "LOCK-prefixed x87 no-operation must remain invalid: {bytes:02X?}",
        );
    }

    match lift_single(&[0xDB, 0xE5]) {
        Err(LiftError::Unsupported { addr, mnemonic }) => {
            assert_eq!(addr, 0x1000);
            assert_eq!(mnemonic, "x87 DB E5");
        }
        result => panic!("reserved neighbor DB E5 must remain unsupported, got {result:?}"),
    }
}

#[test]
fn obsolete_x87_no_operations_do_not_split_a_strict_lifted_block() {
    let ops = lift_one(&[
        0xDB, 0xE0, // FENI8087_NOP
        0xDB, 0xE1, // FDISI8087_NOP
        0xDB, 0xE4, // FSETPM287_NOP
        0x90, // NOP
    ])
    .expect("obsolete x87 no-operations must not force interpreter fallback");
    assert!(ops.is_empty());
}
