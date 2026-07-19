//! Fail-closed admission tests for architectural trap terminators.

use super::*;
use crate::smir::ir::TrapKind;
use crate::smir::lower::aarch64::Aarch64Lowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};

#[test]
fn general_protection_trap_is_interpreter_only() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::GeneralProtection,
    });
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
