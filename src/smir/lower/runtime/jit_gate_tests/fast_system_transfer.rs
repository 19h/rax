//! Fail-closed native admission and append-only helper ABI for Intel
//! SYSENTER/SYSEXIT.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86FastSystemTransferKind, X86FastSystemTransferOp};
use crate::smir::ir::types::OpId;
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::{
    x86_fast_system_transfer_shape_valid, x86_fast_system_transfer_terminal_shape_valid,
};
use crate::smir::lower::{X86_GUEST_FAST_SYSTEM_TRANSFER_FN_OFFSET, X86_GUEST_INVLPG_FN_OFFSET};

fn transfer(kind: X86FastSystemTransferKind, operand64: bool, next_pc: u64) -> OpKind {
    OpKind::X86FastSystemTransfer(X86FastSystemTransferOp {
        kind,
        target: x86(X86Reg::Rip),
        stack_pointer: x86(X86Reg::Rsp),
        return_target: x86(X86Reg::Rdx),
        return_stack_pointer: x86(X86Reg::Rcx),
        operand64,
        next_pc,
    })
}

fn function(kind: OpKind, target: VReg) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::IndirectBranch {
        target,
        possible_targets: vec![],
    });
    builder.finish()
}

fn gate(function: &crate::smir::ir::SmirFunction, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(function, &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn fast_system_transfer_helper_offset_is_append_only_exact_and_zero_initialized() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, fast_system_transfer_fn),
        X86_GUEST_FAST_SYSTEM_TRANSFER_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_FAST_SYSTEM_TRANSFER_FN_OFFSET,
        X86_GUEST_INVLPG_FN_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().fast_system_transfer_fn, 0);
}

#[test]
fn fast_system_transfer_gate_admits_exact_terminal_forms_without_memory_helpers() {
    for (kind, operand64, next_pc) in [
        (X86FastSystemTransferKind::Sysenter, false, 0x1002),
        (X86FastSystemTransferKind::Sysexit, false, 0x1002),
        (X86FastSystemTransferKind::Sysexit, true, 0x1003),
    ] {
        let function = function(transfer(kind, operand64, next_pc), x86(X86Reg::Rip));
        let op = &function.blocks[0].ops[0];
        assert!(op.is_jit_safe());
        assert!(x86_fast_system_transfer_shape_valid(op));
        assert!(x86_fast_system_transfer_terminal_shape_valid(
            &function.blocks[0]
        ));
        assert!(!x86_jit_op_uses_mem_helper(&op.kind));
        assert!(gate(&function, false));
        assert!(gate(&function, true));
    }
}

#[test]
fn fast_system_transfer_gate_rejects_malformed_terminal_and_cross_host_shapes() {
    let mut malformed = vec![
        transfer(X86FastSystemTransferKind::Sysenter, true, 0x1002),
        transfer(X86FastSystemTransferKind::Sysenter, false, 0x1001),
        transfer(X86FastSystemTransferKind::Sysenter, false, 0x1010),
    ];
    for (field, value) in [
        (0, x86(X86Reg::Rbx)),
        (1, x86(X86Reg::Rbp)),
        (2, x86(X86Reg::Rax)),
        (3, arm_x(0)),
    ] {
        let mut kind = transfer(X86FastSystemTransferKind::Sysexit, true, 0x1003);
        let OpKind::X86FastSystemTransfer(op) = &mut kind else {
            unreachable!()
        };
        match field {
            0 => op.target = value,
            1 => op.stack_pointer = value,
            2 => op.return_target = value,
            3 => op.return_stack_pointer = value,
            _ => unreachable!(),
        }
        malformed.push(kind);
    }
    for kind in malformed {
        let function = function(kind, x86(X86Reg::Rip));
        assert!(!x86_fast_system_transfer_shape_valid(
            &function.blocks[0].ops[0]
        ));
        assert!(!x86_fast_system_transfer_terminal_shape_valid(
            &function.blocks[0]
        ));
        assert!(!gate(&function, true));
    }

    let exact = transfer(X86FastSystemTransferKind::Sysenter, false, 0x1002);
    let wrong_target = function(exact.clone(), x86(X86Reg::Rbx));
    assert!(!x86_fast_system_transfer_terminal_shape_valid(
        &wrong_target.blocks[0]
    ));
    assert!(!gate(&wrong_target, false));

    let mut annotated = function(exact.clone(), x86(X86Reg::Rip));
    let Terminator::IndirectBranch {
        possible_targets, ..
    } = &mut annotated.blocks[0].terminator
    else {
        unreachable!()
    };
    possible_targets.push(BlockId(1));
    assert!(!gate(&annotated, false));

    let mut nonterminal = function(exact.clone(), x86(X86Reg::Rip));
    nonterminal.blocks[0]
        .ops
        .push(SmirOp::new(OpId(1), 0x1002, OpKind::Nop));
    assert!(!gate(&nonterminal, false));

    let mut duplicate = function(exact.clone(), x86(X86Reg::Rip));
    duplicate.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(1), 0x1000, exact.clone()));
    assert!(!gate(&duplicate, false));

    let mut hinted = function(exact.clone(), x86(X86Reg::Rip));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!gate(&hinted, false));

    let cross = function(exact.clone(), x86(X86Reg::Rip));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &cross,
        &std::collections::HashMap::new()
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
}

#[test]
fn fast_system_transfer_survives_o2_with_terminal_ownership_and_admission() {
    let mut function = function(
        transfer(X86FastSystemTransferKind::Sysexit, true, 0x1003),
        x86(X86Reg::Rip),
    );
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert!(matches!(
        function.entry_block().unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FastSystemTransfer(_),
            ..
        }]
    ));
    assert!(x86_fast_system_transfer_terminal_shape_valid(
        function.entry_block().unwrap()
    ));
    assert!(gate(&function, false));
}
