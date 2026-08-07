//! Exhaustive strict-lift and metadata coverage for PUSHF/POPF.

use super::*;
use crate::smir::ir::ops::{X86StackFlagsKind, X86StackFlagsOp};

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

fn exact(result: &LiftResult) -> &X86StackFlagsOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86StackFlags(op) => op,
        other => panic!("expected one X86StackFlags op, got {other:?}"),
    }
}

#[test]
fn all_scanner_stack_flags_images_lift_without_fallback() {
    let mut images = 0usize;
    for prefix in SCANNER_PREFIXES {
        for (opcode, kind) in [
            (0x9C, X86StackFlagsKind::Push),
            (0x9D, X86StackFlagsKind::Pop),
        ] {
            let mut bytes = prefix.to_vec();
            bytes.push(opcode);
            let result = lift_single(&bytes)
                .unwrap_or_else(|error| panic!("{bytes:02X?}: strict fallback: {error:?}"));
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
            let op = exact(&result);
            assert_eq!(op.kind, kind, "{bytes:02X?}");
            assert_eq!(
                op.width,
                if prefix.contains(&0x66) && !prefix.iter().any(|byte| byte & 0xF8 == 0x48) {
                    OpWidth::W16
                } else {
                    OpWidth::W64
                },
                "{bytes:02X?}"
            );
            assert!(!op.requires_apx, "{bytes:02X?}");
            assert_eq!(op.next_pc, 0x1000 + bytes.len() as u64, "{bytes:02X?}");
            images += 1;
        }
    }
    assert_eq!(images, 28);
}

#[test]
fn rex2_payload_space_retains_apx_and_w_precedence() {
    let mut images = 0usize;
    for payload in 0_u8..=0x7F {
        for opcode in [0x9C, 0x9D] {
            let bytes = [0x66, 0xD5, payload, opcode];
            let result =
                lift_single(&bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
            let op = exact(&result);
            assert!(op.requires_apx, "{bytes:02X?}");
            assert_eq!(
                op.width,
                if payload & 0x08 != 0 {
                    OpWidth::W64
                } else {
                    OpWidth::W16
                },
                "{bytes:02X?}"
            );
            images += 1;
        }
    }
    assert_eq!(images, 256);
}

#[test]
fn stack_flags_metadata_is_stateful_faulting_and_flag_exact() {
    for (opcode, kind) in [
        (0x9C, X86StackFlagsKind::Push),
        (0x9D, X86StackFlagsKind::Pop),
    ] {
        let op = lift_single(&[opcode]).unwrap().ops.remove(0);
        assert_eq!(op.kind.source_vregs(), vec![x86_gpr(4)]);
        assert_eq!(op.kind.dests(), vec![x86_gpr(4)]);
        assert_eq!(op.kind.flags_read(), FlagSet::ALL_X86);
        assert_eq!(
            op.kind.flags_written(),
            if kind == X86StackFlagsKind::Pop {
                FlagSet::ALL_X86
            } else {
                FlagSet::EMPTY
            }
        );
        assert!(op.kind.has_side_effects());
        assert_eq!(op.kind.reads_memory(), kind == X86StackFlagsKind::Pop);
        assert_eq!(op.kind.writes_memory(), kind == X86StackFlagsKind::Push);
        assert!(!op.kind.is_jit_safe());
        assert!(!op.is_jit_safe());
    }
}

#[test]
fn lock_is_rejected_before_stack_flags_lifting() {
    for opcode in [0x9C, 0x9D] {
        assert!(matches!(
            lift_single(&[0xF0, opcode]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}
