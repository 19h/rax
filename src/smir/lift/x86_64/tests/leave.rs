//! Exhaustive strict-lift and metadata coverage for long-mode x86 `LEAVE`.

use super::*;
use crate::smir::ir::ops::{X86LeaveOp, X86LeaveWidth};

const SCANNER_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn exact(result: &LiftResult) -> &X86LeaveOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86Leave(op) => op,
        other => panic!("expected one X86Leave op, got {other:?}"),
    }
}

#[test]
fn all_scanner_leave_images_lift_without_fallback() {
    for prefix in SCANNER_PREFIXES {
        let mut bytes = prefix.to_vec();
        bytes.push(0xC9);
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("{bytes:02X?}: strict fallback: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        let op = exact(&result);
        assert_eq!(
            op.width,
            if prefix.contains(&0x66) && !prefix.iter().any(|byte| byte & 0xF8 == 0x48) {
                X86LeaveWidth::W16
            } else {
                X86LeaveWidth::W64
            },
            "{bytes:02X?}"
        );
        assert!(!op.requires_apx, "{bytes:02X?}");
        assert_eq!(op.next_pc, 0x1000 + bytes.len() as u64, "{bytes:02X?}");
    }
}

#[test]
fn leave_obeys_effective_rex_order_and_rex2_w_precedence() {
    for (bytes, expected) in [
        (&[0x66, 0xC9][..], X86LeaveWidth::W16),
        (&[0x66, 0x48, 0xC9], X86LeaveWidth::W64),
        (&[0x48, 0x66, 0xC9], X86LeaveWidth::W16),
        (&[0x66, 0x40, 0xC9], X86LeaveWidth::W16),
    ] {
        assert_eq!(
            exact(&lift_single(bytes).unwrap()).width,
            expected,
            "{bytes:02X?}"
        );
    }

    for payload in 0_u8..=0x7F {
        let bytes = [0x66, 0xD5, payload, 0xC9];
        let result = lift_single(&bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        let op = exact(&result);
        assert!(op.requires_apx, "{bytes:02X?}");
        assert_eq!(
            op.width,
            if payload & 0x08 != 0 {
                X86LeaveWidth::W64
            } else {
                X86LeaveWidth::W16
            },
            "{bytes:02X?}"
        );
        assert_eq!(op.next_pc, 0x1004);
    }
}

#[test]
fn leave_metadata_is_faulting_stateful_and_flag_neutral() {
    let op = lift_single(&[0xC9]).unwrap().ops.remove(0);
    assert_eq!(op.kind.source_vregs(), vec![x86_gpr(5)]);
    assert_eq!(op.kind.dests(), vec![x86_gpr(4), x86_gpr(5)]);
    assert_eq!(op.kind.flags_read(), FlagSet::EMPTY);
    assert_eq!(op.kind.flags_written(), FlagSet::EMPTY);
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(!op.kind.is_jit_safe());
    assert!(!op.is_jit_safe());
}

#[test]
fn lock_leave_is_rejected_and_instruction_length_is_bounded() {
    assert!(matches!(
        lift_single(&[0xF0, 0xC9]),
        Err(LiftError::InvalidEncoding { .. })
    ));

    let mut maximum = vec![0x66; 14];
    maximum.push(0xC9);
    let result = lift_single(&maximum).expect("15-byte LEAVE must remain encodable");
    assert_eq!(result.bytes_consumed, 15);
    assert_eq!(exact(&result).next_pc, 0x100F);

    let mut too_long = vec![0x66; 15];
    too_long.push(0xC9);
    assert!(lift_single(&too_long).is_err());
}
