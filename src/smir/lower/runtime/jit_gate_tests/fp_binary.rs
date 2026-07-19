//! Fail-closed admission tests for exact x86 binary FP semantics.

use super::*;
use crate::smir::ir::types::X86FpBinaryOp;

#[test]
fn x86_fp_binary_is_interpreter_only_until_native_mxcsr_semantics_exist() {
    let dst = x86(X86Reg::Xmm(0));
    let src1 = x86(X86Reg::Xmm(1));
    let src2 = x86(X86Reg::Xmm(2));
    let mask = x86(X86Reg::K(1));
    let dynamic = OpKind::X86FpBinary {
        dst,
        src1,
        src2,
        mask: Some(mask),
        elem: VecElementType::F32,
        lanes: 1,
        op: X86FpBinaryOp::Div,
        round: FpRoundMode::Dynamic,
        suppress_exceptions: false,
    };
    assert!(!dynamic.is_jit_safe());
    assert!(dynamic.has_side_effects(), "MXCSR/#XM is observable");
    assert_eq!(dynamic.dests(), vec![dst]);
    assert_eq!(dynamic.source_vregs(), vec![src1, src2, mask]);
    assert!(!x86_gate(dynamic.clone()));
    assert!(!aarch64_gate(vec![dynamic], false));

    let embedded = OpKind::X86FpBinary {
        dst,
        src1,
        src2,
        mask: None,
        elem: VecElementType::F64,
        lanes: 1,
        op: X86FpBinaryOp::Add,
        round: FpRoundMode::RoundUp,
        suppress_exceptions: true,
    };
    assert!(!embedded.is_jit_safe());
    assert!(!embedded.has_side_effects());
    assert_eq!(embedded.source_vregs(), vec![src1, src2]);
    assert!(!x86_gate(embedded.clone()));
    assert!(!aarch64_gate(vec![embedded], false));
}
