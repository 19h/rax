//! JSON serialization for A64 integer width transitions.

use super::{OracleJson, Value, json};
use crate::smir::ir::ops::{ArmSimdLongOp, ArmSimdNarrowOp};

pub(super) fn long_json(operation: &ArmSimdLongOp) -> Value {
    json!({
        "opcode": "arm_simd_long",
        "dst": operation.dst.oracle_json(),
        "src1": operation.src1.oracle_json(),
        "src2": operation.src2.oracle_json(),
        "elem": operation.elem.oracle_json(),
        "kind": format!("{:?}", operation.kind),
        "signed": operation.signed,
        "part": format!("{:?}", operation.part),
    })
}

pub(super) fn narrow_json(operation: &ArmSimdNarrowOp) -> Value {
    json!({
        "opcode": "arm_simd_narrow",
        "dst": operation.dst.oracle_json(),
        "src1": operation.src1.oracle_json(),
        "src2": operation.src2.oracle_json(),
        "elem": operation.elem.oracle_json(),
        "kind": format!("{:?}", operation.kind),
        "part": format!("{:?}", operation.part),
    })
}
