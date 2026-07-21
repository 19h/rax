//! Fail-closed admission tests for x87 transcendental state semantics.

use super::*;
use crate::smir::ir::ops::{X86X87DataKind, X86X87TranscendentalKind};

#[test]
fn every_x87_transcendental_remains_interpreter_only_without_native_x87_state_marshalling() {
    for (fop, kind) in [
        (0x01F0, X86X87TranscendentalKind::Exp2MinusOne),
        (0x01F1, X86X87TranscendentalKind::YLog2X),
        (0x01F2, X86X87TranscendentalKind::Tangent),
        (0x01F3, X86X87TranscendentalKind::Arctangent),
        (0x01F9, X86X87TranscendentalKind::YLog2Xp1),
        (0x01FB, X86X87TranscendentalKind::SineCosine),
        (0x01FE, X86X87TranscendentalKind::Sine),
        (0x01FF, X86X87TranscendentalKind::Cosine),
    ] {
        let op = OpKind::X86X87Data {
            kind: X86X87DataKind::Transcendental(kind),
            addr: None,
            st: (fop & 7) as u8,
            fop,
        };
        assert!(op.has_side_effects(), "{kind:?}");
        assert!(!op.reads_memory(), "{kind:?}");
        assert!(!op.writes_memory(), "{kind:?}");
        assert!(op.dests().is_empty(), "{kind:?}");
        assert!(op.source_vregs().is_empty(), "{kind:?}");
        assert!(!op.is_jit_safe(), "{kind:?}");
        assert!(!x86_gate(op.clone()), "x86-64 gate admitted {kind:?}");
        assert!(
            !aarch64_gate(vec![op], false),
            "AArch64 gate admitted {kind:?}"
        );
    }
}
