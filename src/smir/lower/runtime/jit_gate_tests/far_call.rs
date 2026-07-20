//! Fail-closed native admission and helper ABI for indirect far CALL.

use super::*;
use crate::smir::ir::ops::X86FarCallOp;
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::{x86_far_call_shape_valid, x86_far_call_terminal_shape_valid};
use crate::smir::lower::{X86_GUEST_FAR_CALL_FN_OFFSET, X86_GUEST_FAR_JUMP_FN_OFFSET};

fn far_call(addr: Address, target: VReg, width: OpWidth, requires_apx: bool) -> OpKind {
    OpKind::X86FarCall(X86FarCallOp {
        addr,
        target,
        offset_width: width,
        requires_apx,
        stack_segment: false,
        next_pc: 0x1003,
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
fn far_call_helper_offset_is_append_only_and_matches_guest_layout() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, far_call_fn),
        X86_GUEST_FAR_CALL_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_FAR_CALL_FN_OFFSET,
        X86_GUEST_FAR_JUMP_FN_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().far_call_fn, 0);
}

#[test]
fn far_call_gate_accepts_only_exact_terminal_memory_helper_shape() {
    let exact = far_call(
        Address::Direct(x86(X86Reg::Rsp)),
        x86(X86Reg::Rip),
        OpWidth::W64,
        false,
    );
    let exact_function = function(exact.clone(), x86(X86Reg::Rip));
    let op = &exact_function.blocks[0].ops[0];
    assert!(op.is_jit_safe());
    assert!(x86_far_call_shape_valid(op));
    assert!(x86_far_call_terminal_shape_valid(&exact_function.blocks[0]));
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
        far_call(
            Address::Direct(VReg::virt(0)),
            x86(X86Reg::Rip),
            OpWidth::W64,
            false,
        ),
        far_call(
            Address::Direct(x86(X86Reg::R31)),
            x86(X86Reg::Rip),
            OpWidth::W64,
            false,
        ),
        far_call(
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rbx),
            OpWidth::W64,
            false,
        ),
        far_call(
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rip),
            OpWidth::W8,
            false,
        ),
    ] {
        let malformed = function(malformed, x86(X86Reg::Rip));
        assert!(!x86_far_call_terminal_shape_valid(&malformed.blocks[0]));
        assert!(!is_native_clobber_safe_excluding(
            &malformed,
            &std::collections::HashMap::new(),
            true,
        ));
    }
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &exact_function,
        &std::collections::HashMap::new(),
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
}
