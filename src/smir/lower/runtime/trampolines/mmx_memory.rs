//! Fail-closed admission for helper-backed MMX memory transfers.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

/// Admit only the legacy `0F 6F /r` and `0F 7F /r` MMX MOVQ memory forms.
/// Virtual V64 temporaries remain ineligible because they have no stable state
/// slot across the Rust MMU helper boundary.
pub fn x86_jit_mmx_mem_shape_valid(op: &SmirOp) -> bool {
    let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
    match (&op.kind, op.x86_hint) {
        (
            OpKind::VLoad {
                dst,
                addr,
                width: VecWidth::V64,
            },
            Some(X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x6F,
            }),
        ) => mm(dst) && super::x86_jit_mem_address_shape_valid(addr),
        (
            OpKind::VStore {
                src,
                addr,
                width: VecWidth::V64,
            },
            Some(X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x7F,
            }),
        ) => mm(src) && super::x86_jit_mem_address_shape_valid(addr),
        _ => false,
    }
}
