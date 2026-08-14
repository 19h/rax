//! Stable JSON serialization for ordered x86 stack-frame transactions.

use super::{OracleJson, Value, debug_name, json};
use crate::smir::ir::ops::{X86EnterOp, X86LeaveOp, X86LeaveWidth, X86StackFlagsOp};

impl OracleJson for X86LeaveWidth {
    fn oracle_json(&self) -> Value {
        json!(debug_name(self))
    }
}

pub(super) fn enter_json(op: &X86EnterOp) -> Value {
    json!({
        "opcode": "x86_enter",
        "allocation_size": op.allocation_size.oracle_json(),
        "nesting_level": op.nesting_level.oracle_json(),
        "width": op.width.oracle_json(),
        "requires_apx": op.requires_apx.oracle_json(),
        "next_pc": op.next_pc.oracle_json(),
    })
}

pub(super) fn stack_flags_json(op: &X86StackFlagsOp) -> Value {
    json!({
        "opcode": "x86_stack_flags",
        "kind": op.kind.oracle_json(),
        "width": op.width.oracle_json(),
        "requires_apx": op.requires_apx.oracle_json(),
        "next_pc": op.next_pc.oracle_json(),
    })
}

pub(super) fn leave_json(op: &X86LeaveOp) -> Value {
    json!({
        "opcode": "x86_leave",
        "width": op.width.oracle_json(),
        "requires_apx": op.requires_apx.oracle_json(),
        "next_pc": op.next_pc.oracle_json(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leave_schema_exposes_width_feature_requirement_and_exact_successor() {
        assert_eq!(
            leave_json(&X86LeaveOp {
                width: X86LeaveWidth::W16,
                requires_apx: true,
                next_pc: 0x1234,
            }),
            json!({
                "opcode": "x86_leave",
                "width": "W16",
                "requires_apx": true,
                "next_pc": 0x1234,
            })
        );
    }
}
