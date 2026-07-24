//! Stable JSON serialization for the x86 FMA3 SMIR boundary.

use super::{OracleJson, Value, json};
use crate::smir::ir::ops::X86FmaOp;

pub(super) fn op_json(op: &X86FmaOp) -> Value {
    json!({
        "opcode": "x86_fma",
        "dst": op.dst.oracle_json(),
        "src1": op.src1.oracle_json(),
        "src2": op.src2.oracle_json(),
        "src3": op.src3.oracle_json(),
        "mask": op.mask.oracle_json(),
        "elem": op.elem.oracle_json(),
        "kind": op.kind.oracle_json(),
        "order": op.order.oracle_json(),
        "round": op.round.oracle_json(),
        "lanes": op.lanes.oracle_json(),
    })
}
