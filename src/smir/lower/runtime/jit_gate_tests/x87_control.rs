//! Fail-closed admission and ABI tests for state-backed x87 controls.

use super::*;
use crate::smir::lower::runtime::{
    GuestRegs, uses_x86_x87_environment_state_excluding, uses_x86_x87_tag_state_excluding,
};
use crate::smir::lower::{
    X86_GUEST_STACK_FLAGS_RFLAGS_VALID_OFFSET, X86_GUEST_X87_CONTROL_WORD_OFFSET,
    X86_GUEST_X87_DATA_PTR_OFFSET, X86_GUEST_X87_INSTR_PTR_OFFSET,
    X86_GUEST_X87_LAST_OPCODE_OFFSET, X86_GUEST_X87_STATE_ACTIVE_OFFSET,
    X86_GUEST_X87_STATUS_WORD_OFFSET,
};

fn control(kind: X86X87ControlKind) -> OpKind {
    OpKind::X86X87Control { kind, addr: None }
}

#[test]
fn x87_no_wait_environment_controls_are_narrowly_x86_native_safe() {
    for kind in [
        X86X87ControlKind::Init,
        X86X87ControlKind::ClearExceptions,
        X86X87ControlKind::StoreStatusAx,
    ] {
        let op = control(kind);
        assert!(op.is_jit_safe(), "{kind:?}");
        assert!(x86_gate(op.clone()), "x86-64 gate rejected {kind:?}");
        assert!(
            !aarch64_gate(vec![op], false),
            "AArch64 guest-state ABI admitted x87 {kind:?}"
        );
    }

    for kind in [
        X86X87ControlKind::LoadControlWord,
        X86X87ControlKind::StoreControlWord,
        X86X87ControlKind::StoreStatusWord,
        X86X87ControlKind::LoadEnvironment(crate::smir::ir::ops::X86X87EnvWidth::W32),
        X86X87ControlKind::StoreEnvironment(crate::smir::ir::ops::X86X87EnvWidth::W32),
        X86X87ControlKind::RestoreState(crate::smir::ir::ops::X86X87EnvWidth::W32),
        X86X87ControlKind::SaveState(crate::smir::ir::ops::X86X87EnvWidth::W32),
    ] {
        let op = control(kind);
        assert!(!op.is_jit_safe(), "payload/memory x87 {kind:?}");
        assert!(!x86_gate(op), "x86-64 gate admitted {kind:?}");
    }
}

#[test]
fn x87_environment_detector_honors_native_exit_exclusion() {
    for (index, kind) in [
        X86X87ControlKind::Init,
        X86X87ControlKind::ClearExceptions,
        X86X87ControlKind::StoreStatusAx,
    ]
    .into_iter()
    .enumerate()
    {
        let mut builder = FunctionBuilder::new(FunctionId(index as u32), 0x1000);
        builder.push_op(0x1000, control(kind));
        builder.set_terminator(Terminator::Return { values: vec![] });
        let function = builder.finish();
        assert!(uses_x86_x87_environment_state_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
        assert!(!uses_x86_x87_tag_state_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
        assert!(!uses_x86_x87_environment_state_excluding(
            &function,
            &std::collections::HashMap::from([(function.entry, 0x1002)])
        ));
    }
}

#[test]
fn x87_environment_abi_is_append_only_and_exact() {
    for (actual, expected) in [
        (
            std::mem::offset_of!(GuestRegs, x87_control_word),
            X86_GUEST_X87_CONTROL_WORD_OFFSET as usize,
        ),
        (
            std::mem::offset_of!(GuestRegs, x87_status_word),
            X86_GUEST_X87_STATUS_WORD_OFFSET as usize,
        ),
        (
            std::mem::offset_of!(GuestRegs, x87_data_ptr),
            X86_GUEST_X87_DATA_PTR_OFFSET as usize,
        ),
        (
            std::mem::offset_of!(GuestRegs, x87_instr_ptr),
            X86_GUEST_X87_INSTR_PTR_OFFSET as usize,
        ),
        (
            std::mem::offset_of!(GuestRegs, x87_last_opcode),
            X86_GUEST_X87_LAST_OPCODE_OFFSET as usize,
        ),
        (
            std::mem::offset_of!(GuestRegs, x87_state_active),
            X86_GUEST_X87_STATE_ACTIVE_OFFSET as usize,
        ),
    ] {
        assert_eq!(actual, expected);
    }
    assert_eq!(
        X86_GUEST_X87_CONTROL_WORD_OFFSET,
        X86_GUEST_STACK_FLAGS_RFLAGS_VALID_OFFSET + 8
    );
    assert_eq!(
        X86_GUEST_X87_STATE_ACTIVE_OFFSET,
        X86_GUEST_X87_CONTROL_WORD_OFFSET + 5 * 8
    );

    let defaults = GuestRegs::default();
    assert_eq!(defaults.x87_control_word, 0x037F);
    assert_eq!(defaults.x87_status_word, 0);
    assert_eq!(defaults.x87_tag_word, 0xFFFF);
    assert_eq!(defaults.x87_data_ptr, 0);
    assert_eq!(defaults.x87_instr_ptr, 0);
    assert_eq!(defaults.x87_last_opcode, 0);
    assert_eq!(defaults.x87_state_active, 0);
}
