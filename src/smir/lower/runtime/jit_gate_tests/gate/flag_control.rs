//! Fail-closed native-admission tests for operand-free x86 flag controls.

use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::OpId;
use crate::smir::lower::x86_64::x86_flag_control_shape_valid;

fn function_with_op(op: SmirOp) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(op.guest_pc, op.kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = op.x86_hint;
    function
}

#[test]
fn flag_control_gate_is_target_specific_exact_and_unhinted() {
    for kind in [
        OpKind::SetCF { value: false },
        OpKind::SetCF { value: true },
        OpKind::CmcCF,
        OpKind::SetDF { value: false },
        OpKind::SetDF { value: true },
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(x86_flag_control_shape_valid(&op), "{kind:?}");
        assert!(
            !op.is_jit_safe(),
            "generic cross-target admission must remain closed for {kind:?}"
        );
        assert!(is_native_clobber_safe(&function_with_op(op)), "{kind:?}");

        let mut hinted = SmirOp::new(OpId(0), 0x1000, kind.clone());
        hinted.x86_hint = Some(X86OpHint::Mulx);
        assert!(!x86_flag_control_shape_valid(&hinted), "{kind:?}");
        assert!(
            !is_native_clobber_safe(&function_with_op(hinted)),
            "encoding metadata must fail closed for {kind:?}"
        );
    }

    for kind in [
        OpKind::ReadFlags {
            dst: x86(X86Reg::Rax),
        },
        OpKind::WriteFlags {
            src: x86(X86Reg::Rax),
        },
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(!x86_flag_control_shape_valid(&op));
        assert!(!is_native_clobber_safe(&function_with_op(op)), "{kind:?}");
    }
}
