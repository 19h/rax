//! Fail-closed admission tests for architectural trap terminators.

use super::*;
use crate::smir::ir::{TrapKind, X86Segment, X86StringIoKind};
use crate::smir::lower::aarch64::Aarch64Lowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};

fn assert_trap_is_interpreter_only(kind: TrapKind) {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap { kind });
    let function = builder.finish();

    assert!(!is_native_clobber_safe(&function));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
    assert!(!is_aarch64_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        false,
    ));

    let excluded = std::collections::HashMap::from([(function.entry, 0x1000)]);
    assert!(is_native_clobber_safe_excluding(
        &function, &excluded, false,
    ));
    assert!(is_x86_aarch64_native_clobber_safe_excluding(
        &function, &excluded,
    ));
    assert!(is_aarch64_native_clobber_safe_excluding(
        &function, &excluded, false,
    ));

    let x86_error = X86_64Lowerer::new().lower_function(&function).unwrap_err();
    assert!(matches!(x86_error, LowerError::UnsupportedOp { .. }));
    let aarch64_error = Aarch64Lowerer::new().lower_function(&function).unwrap_err();
    assert!(matches!(aarch64_error, LowerError::UnsupportedOp { .. }));
}

#[test]
fn general_protection_trap_is_interpreter_only() {
    assert_trap_is_interpreter_only(TrapKind::GeneralProtection);
}

#[test]
fn x86_debug_trap_is_interpreter_only() {
    assert_trap_is_interpreter_only(TrapKind::X86Debug {
        fault_pc: 0x1000,
        return_pc: 0x1001,
        requires_apx: false,
    });
}

#[test]
fn x86_breakpoint_trap_is_interpreter_only() {
    assert_trap_is_interpreter_only(TrapKind::X86Breakpoint {
        fault_pc: 0x1000,
        return_pc: 0x1001,
        requires_apx: false,
    });
}

#[test]
fn x86_software_interrupt_trap_is_interpreter_only() {
    assert_trap_is_interpreter_only(TrapKind::X86SoftwareInterrupt {
        vector: 0x80,
        fault_pc: 0x1000,
        return_pc: 0x1002,
        requires_apx: false,
    });
}

#[test]
fn x86_interrupt_return_trap_is_interpreter_only() {
    assert_trap_is_interpreter_only(TrapKind::X86InterruptReturn {
        width: OpWidth::W64,
        fault_pc: 0x1000,
        requires_apx: false,
    });
}

#[test]
fn x86_string_io_trap_is_interpreter_only() {
    assert_trap_is_interpreter_only(TrapKind::X86StringIo {
        kind: X86StringIoKind::Outs,
        width: MemWidth::B2,
        address_width: OpWidth::W32,
        repeated: true,
        memory_segment: X86Segment::Fs,
        fault_pc: 0x1000,
        return_pc: 0x1005,
        requires_apx: false,
    });
}
