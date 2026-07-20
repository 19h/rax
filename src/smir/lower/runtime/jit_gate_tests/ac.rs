//! Native admission and ABI-layout coverage for guest CLAC/STAC state.

use super::*;
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::x86_set_ac_shape_valid;
use crate::smir::lower::{X86_GUEST_AC_FLAG_OFFSET, X86_GUEST_TSC_FN_OFFSET};

#[test]
fn x86_clac_stac_state_layout_is_exact_and_appended() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, ac_flag),
        X86_GUEST_AC_FLAG_OFFSET as usize
    );
    assert_eq!(X86_GUEST_AC_FLAG_OFFSET, X86_GUEST_TSC_FN_OFFSET + 8);
    assert_eq!(GuestRegs::default().ac_flag, 0);
}

#[test]
fn x86_gate_admits_both_exact_clac_stac_operations() {
    for value in [false, true] {
        let op = OpKind::SetAC { value };
        assert!(x86_set_ac_shape_valid(&op));
        assert!(x86_gate(op));
    }
}

#[test]
fn non_x86_64_native_gates_reject_guest_ac_state_operations() {
    for value in [false, true] {
        assert!(!aarch64_gate(vec![OpKind::SetAC { value }], false));
        assert!(!x86_aarch64_gate(vec![OpKind::SetAC { value }]));
    }
}
