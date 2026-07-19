//! Fail-closed admission tests for x86 square-root semantics.

use super::*;

#[test]
fn x86_sqrt_er_is_interpreter_only_until_native_state_semantics_exist() {
    let op = OpKind::X86Sqrt {
        dst: x86(X86Reg::Zmm(0)),
        src: x86(X86Reg::Zmm(1)),
        elem: VecElementType::F32,
        lanes: 16,
        round: FpRoundMode::RoundUp,
        suppress_exceptions: true,
    };
    assert!(!op.is_jit_safe());
    assert!(!op.has_side_effects());
    assert_eq!(op.dests(), vec![x86(X86Reg::Zmm(0))]);
    assert_eq!(op.source_vregs(), vec![x86(X86Reg::Zmm(1))]);
    assert!(!x86_gate(op.clone()));
    assert!(!aarch64_gate(vec![op], false));

    let dynamic = OpKind::X86Sqrt {
        dst: x86(X86Reg::Xmm(0)),
        src: x86(X86Reg::Xmm(1)),
        elem: VecElementType::F64,
        lanes: 1,
        round: FpRoundMode::Dynamic,
        suppress_exceptions: false,
    };
    assert!(
        dynamic.has_side_effects(),
        "MXCSR/#XM makes dynamic SQRT observable"
    );
    assert!(!x86_gate(dynamic.clone()));
    assert!(!aarch64_gate(vec![dynamic], false));
}
