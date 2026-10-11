//! JSON serialization for A64 same-width integer saturation.

use super::{OracleJson, Value, json};
use crate::smir::ir::ops::ArmSimdSaturatingOp;

pub(super) fn operation_json(operation: &ArmSimdSaturatingOp) -> Value {
    json!({
        "opcode": "arm_simd_saturating",
        "dst": operation.dst.oracle_json(),
        "src1": operation.src1.oracle_json(),
        "src2": operation.src2.oracle_json(),
        "elem": operation.elem.oracle_json(),
        "kind": format!("{:?}", operation.kind),
        "signed": operation.signed,
        "form": format!("{:?}", operation.form),
    })
}
