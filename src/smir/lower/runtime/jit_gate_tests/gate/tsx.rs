//! RTM deterministic fallback native-admission tests.

use super::*;
use crate::smir::lower::runtime::jit_gate_tests::*;
use crate::smir::lower::runtime::*;

#[test]
fn xtest_is_admitted_only_by_the_x86_specific_gate() {
    assert!(
        !OpKind::X86XTest.is_jit_safe(),
        "generic cross-host admission must remain fail-closed"
    );
    assert!(x86_gate(OpKind::X86XTest));
    assert!(!aarch64_gate(vec![OpKind::X86XTest], false));
    assert_eq!(x86_flag_uses(&OpKind::X86XTest), FlagSet::EMPTY);
    assert_eq!(x86_flag_defs(&OpKind::X86XTest), FlagSet::ALL_X86);
}
