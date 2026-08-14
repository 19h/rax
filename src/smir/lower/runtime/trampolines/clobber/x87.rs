//! Fail-closed x87 state-backed operation validation for the native gate.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::lower::x86_64::x86_x87_state_shape_valid;

pub(super) fn x86_x87_op_shape_valid(op: &SmirOp) -> bool {
    !matches!(
        op.kind,
        OpKind::X86X87Control { .. } | OpKind::X86X87Data { .. }
    ) || x86_x87_state_shape_valid(op)
}
