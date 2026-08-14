//! Optimizer preservation tests for state-backed x87 no-wait controls.

use super::*;
use crate::smir::ir::ops::X86X87ControlKind;
use crate::smir::ir::types::SourceArch;
use crate::smir::ir::{FunctionBuilder, SmirFunction};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::tests::*;

const FORMS: [([u8; 2], X86X87ControlKind); 3] = [
    ([0xDB, 0xE2], X86X87ControlKind::ClearExceptions),
    ([0xDB, 0xE3], X86X87ControlKind::Init),
    ([0xDF, 0xE0], X86X87ControlKind::StoreStatusAx),
];

fn lifted(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
    let lifted = lifter.lift_insn(0x1000, bytes, &mut lift_ctx).unwrap();
    assert_eq!(lifted.bytes_consumed, bytes.len());
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops = lifted.ops;
    function
}

fn assert_form(function: &SmirFunction, expected: X86X87ControlKind, rex2: bool) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), 1 + usize::from(rex2), "{ops:?}");
    if rex2 {
        assert!(matches!(ops[0].kind, OpKind::X86RequireApx), "{ops:?}");
    }
    assert!(
        matches!(
            ops.last().unwrap().kind,
            OpKind::X86X87Control { kind, addr: None } if kind == expected
        ),
        "{ops:?}"
    );
    for (index, op) in ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16), "{ops:?}");
        assert_eq!(op.guest_pc, 0x1000, "{ops:?}");
    }
}

#[test]
fn every_optimizer_level_preserves_each_x87_no_wait_control_side_effect() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (bytes, expected) in FORMS {
            for prefix in [None, Some(0x66), Some(0xF2), Some(0xF3)] {
                let encoded = prefix.into_iter().chain(bytes).collect::<Vec<_>>();
                let mut function = lifted(&encoded);
                optimize_function(&mut function, level);
                assert_form(&function, expected, false);
            }
        }
    }
}

#[test]
fn o2_preserves_every_rex2_apx_guard_before_each_x87_no_wait_control() {
    for payload in 0x00..=0x7F {
        for (bytes, expected) in FORMS {
            let mut encoded = vec![0xD5, payload];
            encoded.extend_from_slice(&bytes);
            let mut function = lifted(&encoded);
            optimize_function(&mut function, OptLevel::O2);
            assert_form(&function, expected, true);
        }
    }
}
