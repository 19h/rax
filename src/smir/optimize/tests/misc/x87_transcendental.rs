//! Optimizer preservation tests for side-effecting x87 transcendental ops.

use super::*;
use crate::smir::ir::ops::X86X87TranscendentalKind;
use crate::smir::ir::types::SourceArch;
use crate::smir::ir::{FunctionBuilder, SmirFunction};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::tests::*;

const FORMS: [(u8, X86X87TranscendentalKind); 8] = [
    (0xF0, X86X87TranscendentalKind::Exp2MinusOne),
    (0xF1, X86X87TranscendentalKind::YLog2X),
    (0xF2, X86X87TranscendentalKind::Tangent),
    (0xF3, X86X87TranscendentalKind::Arctangent),
    (0xF9, X86X87TranscendentalKind::YLog2Xp1),
    (0xFB, X86X87TranscendentalKind::SineCosine),
    (0xFE, X86X87TranscendentalKind::Sine),
    (0xFF, X86X87TranscendentalKind::Cosine),
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

fn assert_form(function: &SmirFunction, expected: X86X87TranscendentalKind, rex2: bool) {
    let ops = &function.blocks[0].ops;
    assert_eq!(ops.len(), 1 + usize::from(rex2), "{ops:?}");
    if rex2 {
        assert!(matches!(ops[0].kind, OpKind::X86RequireApx), "{ops:?}");
    }
    assert!(
        matches!(
            ops.last().unwrap().kind,
            OpKind::X86X87Data {
                kind: X86X87DataKind::Transcendental(kind),
                addr: None,
                ..
            } if kind == expected
        ),
        "{ops:?}"
    );
    for (index, op) in ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16), "{ops:?}");
        assert_eq!(op.guest_pc, 0x1000, "{ops:?}");
    }
}

#[test]
fn every_optimizer_level_preserves_each_x87_transcendental_side_effect() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (modrm, expected) in FORMS {
            let mut function = lifted(&[0xD9, modrm]);
            optimize_function(&mut function, level);
            assert_form(&function, expected, false);
        }
    }
}

#[test]
fn o2_preserves_every_rex2_apx_guard_before_each_x87_transcendental() {
    for payload in 0x00..=0x7F {
        for (modrm, expected) in FORMS {
            let mut function = lifted(&[0xD5, payload, 0xD9, modrm]);
            optimize_function(&mut function, OptLevel::O2);
            assert_form(&function, expected, true);
        }
    }
}
