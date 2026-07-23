//! Stable JSON serialization for AVX-512 opmask SMIR operations.

use super::{OracleJson, Value, json};
use crate::smir::ir::ops::{
    X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource, X86OpmaskOp,
    X86OpmaskShiftKind, X86OpmaskTestKind,
};

impl OracleJson for X86OpmaskMoveSource {
    fn oracle_json(&self) -> Value {
        match self {
            Self::Mask(reg) => json!({
                "kind": "mask",
                "reg": reg.oracle_json(),
            }),
            Self::Gpr(reg) => json!({
                "kind": "gpr",
                "reg": reg.oracle_json(),
            }),
            Self::Memory(addr) => json!({
                "kind": "memory",
                "addr": addr.oracle_json(),
            }),
        }
    }
}

impl OracleJson for X86OpmaskMoveDestination {
    fn oracle_json(&self) -> Value {
        match self {
            Self::Gpr(reg) => json!({
                "kind": "gpr",
                "reg": reg.oracle_json(),
            }),
            Self::Memory(addr) => json!({
                "kind": "memory",
                "addr": addr.oracle_json(),
            }),
        }
    }
}

impl OracleJson for X86OpmaskOp {
    fn oracle_json(&self) -> Value {
        match self {
            Self::MoveToMask { dst, src, width } => json!({
                "operation": "move_to_mask",
                "dst": dst.oracle_json(),
                "src": src.oracle_json(),
                "width": width.oracle_json(),
            }),
            Self::MoveFromMask { dst, src, width } => json!({
                "operation": "move_from_mask",
                "dst": dst.oracle_json(),
                "src": src.oracle_json(),
                "width": width.oracle_json(),
            }),
            Self::Not { dst, src, width } => json!({
                "operation": "not",
                "dst": dst.oracle_json(),
                "src": src.oracle_json(),
                "width": width.oracle_json(),
            }),
            Self::Binary {
                kind,
                dst,
                src1,
                src2,
                width,
            } => json!({
                "operation": "binary",
                "kind": binary_kind(*kind),
                "dst": dst.oracle_json(),
                "src1": src1.oracle_json(),
                "src2": src2.oracle_json(),
                "width": width.oracle_json(),
            }),
            Self::Unpack {
                dst,
                src1,
                src2,
                width,
            } => json!({
                "operation": "unpack",
                "dst": dst.oracle_json(),
                "src1": src1.oracle_json(),
                "src2": src2.oracle_json(),
                "width": width.oracle_json(),
            }),
            Self::Shift {
                kind,
                dst,
                src,
                width,
                count,
            } => json!({
                "operation": "shift",
                "kind": shift_kind(*kind),
                "dst": dst.oracle_json(),
                "src": src.oracle_json(),
                "width": width.oracle_json(),
                "count": count,
            }),
            Self::Test {
                kind,
                src1,
                src2,
                width,
            } => json!({
                "operation": "test",
                "kind": test_kind(*kind),
                "src1": src1.oracle_json(),
                "src2": src2.oracle_json(),
                "width": width.oracle_json(),
            }),
        }
    }
}

fn binary_kind(kind: X86OpmaskBinaryKind) -> &'static str {
    match kind {
        X86OpmaskBinaryKind::Add => "add",
        X86OpmaskBinaryKind::And => "and",
        X86OpmaskBinaryKind::AndNot => "and_not",
        X86OpmaskBinaryKind::Or => "or",
        X86OpmaskBinaryKind::Xnor => "xnor",
        X86OpmaskBinaryKind::Xor => "xor",
    }
}

fn shift_kind(kind: X86OpmaskShiftKind) -> &'static str {
    match kind {
        X86OpmaskShiftKind::Left => "left",
        X86OpmaskShiftKind::Right => "right",
    }
}

fn test_kind(kind: X86OpmaskTestKind) -> &'static str {
    match kind {
        X86OpmaskTestKind::And => "and",
        X86OpmaskTestKind::Or => "or",
    }
}
