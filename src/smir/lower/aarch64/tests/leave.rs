//! Fail-closed cross-lowerer coverage for the x86 `LEAVE` transaction.

use super::*;
use crate::smir::ir::ops::{X86LeaveOp, X86LeaveWidth};

#[test]
fn rejects_leave_without_a_fault_precise_cross_host_helper() {
    for (width, requires_apx) in [
        (X86LeaveWidth::W16, false),
        (X86LeaveWidth::W64, false),
        (X86LeaveWidth::W16, true),
        (X86LeaveWidth::W64, true),
    ] {
        assert!(matches!(
            try_lower_single_op(OpKind::X86Leave(X86LeaveOp {
                width,
                requires_apx,
                next_pc: 1,
            })),
            Err(LowerError::UnsupportedOp { .. })
        ));
    }
}
