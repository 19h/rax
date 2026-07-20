//! Fail-closed native admission for x86 CLTS.

use super::*;
use crate::smir::lower::x86_64::x86_clts_shape_valid;

#[test]
fn x86_clts_gate_admits_the_exact_operand_free_operation() {
    let op = OpKind::X86Clts;
    assert!(op.is_jit_safe());
    assert!(x86_clts_shape_valid(&op));
    assert!(x86_gate(op));
}

#[test]
fn x86_clts_gate_rejects_cross_host_execution() {
    let op = OpKind::X86Clts;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, op.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ),
        "CLTS has no AArch64-host CR0 ABI or native lowering"
    );
    assert!(!x86_aarch64_scalar_shape_valid(&op));
    assert!(!aarch64_gate(vec![op], false));
}

#[test]
fn x86_clts_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::X86Clts);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86Clts))
    );
    assert!(is_native_clobber_safe(&function));
}
