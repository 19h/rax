//! Fail-closed native-admission coverage for the x86 APX feature guard.

use super::*;
use crate::smir::lower::x86_64::x86_require_apx_shape_valid;

fn op() -> crate::smir::ir::ops::SmirOp {
    crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::X86RequireApx,
    )
}

#[test]
fn x86_gates_admit_only_the_exact_apx_guard_and_aarch64_guest_rejects_it() {
    let exact = op();
    assert!(exact.kind.is_jit_safe());
    assert!(exact.is_jit_safe());
    assert!(x86_require_apx_shape_valid(&exact));
    assert!(x86_gate(OpKind::X86RequireApx));

    assert!(!aarch64_gate(vec![OpKind::X86RequireApx], false));
    assert!(x86_aarch64_gate(vec![OpKind::X86RequireApx]));
    assert!(x86_aarch64_scalar_shape_valid(&OpKind::X86RequireApx));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::X86RequireApx);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_require_apx_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&hinted));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
    ));
}

#[test]
fn apx_guard_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::X86RequireApx);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(matches!(
        function.entry_block().unwrap().ops.as_slice(),
        [crate::smir::ir::ops::SmirOp {
            kind: OpKind::X86RequireApx,
            ..
        }]
    ));
    assert!(is_native_clobber_safe(&function));
}
