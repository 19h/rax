//! Fail-closed native admission and helper ABI for far RET (`CA`/`CB`).

use super::*;
use crate::smir::ir::ops::X86FarReturnOp;
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::{x86_far_return_shape_valid, x86_far_return_terminal_shape_valid};
use crate::smir::lower::{X86_GUEST_FAR_CALL_FN_OFFSET, X86_GUEST_FAR_RETURN_FN_OFFSET};

fn far_return(target: VReg, width: OpWidth, next_pc: u64) -> OpKind {
    OpKind::X86FarReturn(X86FarReturnOp {
        target,
        offset_width: width,
        pop_bytes: 0x1234,
        requires_apx: false,
        next_pc,
    })
}

fn function(kind: OpKind, terminal_target: VReg) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::IndirectBranch {
        target: terminal_target,
        possible_targets: vec![],
    });
    builder.finish()
}

#[test]
fn far_return_helper_offset_is_append_only_and_matches_guest_layout() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, far_return_fn),
        X86_GUEST_FAR_RETURN_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_FAR_RETURN_FN_OFFSET,
        X86_GUEST_FAR_CALL_FN_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().far_return_fn, 0);
}

#[test]
fn far_return_gate_accepts_only_exact_terminal_memory_helper_shape() {
    let exact = far_return(x86(X86Reg::Rip), OpWidth::W64, 0x1003);
    let exact_function = function(exact.clone(), x86(X86Reg::Rip));
    let op = &exact_function.blocks[0].ops[0];
    assert!(op.is_jit_safe());
    assert!(x86_far_return_shape_valid(op));
    assert!(x86_far_return_terminal_shape_valid(
        &exact_function.blocks[0]
    ));
    assert!(x86_jit_op_uses_mem_helper(&op.kind));
    assert!(!is_native_clobber_safe_excluding(
        &exact_function,
        &std::collections::HashMap::new(),
        false,
    ));
    assert!(is_native_clobber_safe_excluding(
        &exact_function,
        &std::collections::HashMap::new(),
        true,
    ));

    for malformed in [
        far_return(x86(X86Reg::Rbx), OpWidth::W64, 0x1003),
        far_return(x86(X86Reg::Rip), OpWidth::W8, 0x1003),
        far_return(x86(X86Reg::Rip), OpWidth::W64, 0x1000),
        far_return(x86(X86Reg::Rip), OpWidth::W64, 0x1010),
        OpKind::X86FarReturn(X86FarReturnOp {
            target: x86(X86Reg::Rip),
            offset_width: OpWidth::W64,
            pop_bytes: 1,
            requires_apx: false,
            next_pc: 0x1001,
        }),
    ] {
        let malformed = function(malformed, x86(X86Reg::Rip));
        assert!(!x86_far_return_terminal_shape_valid(&malformed.blocks[0]));
        assert!(!is_native_clobber_safe_excluding(
            &malformed,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    let mut hinted = exact_function.clone();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &exact_function,
        &std::collections::HashMap::new(),
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
}
