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
fn obsolete_x87_no_operations_do_not_hide_invalid_neighbors() {
    for bytes in [
        &[0xF0, 0xDB, 0xE0][..],
        &[0xF0, 0xDB, 0xE1][..],
        &[0xF0, 0xDB, 0xE4][..],
    ] {
        let result =
            lift_single(bytes).expect("LOCK-prefixed x87 no-operation must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, bytes.len());
    }

    let reserved =
        lift_single(&[0xDB, 0xE5]).expect("reserved neighbor DB E5 must strictly lift to #UD");
    assert_invalid_opcode_trap(&reserved, 2);
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
