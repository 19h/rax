//! Scalar MMU-helper admission for the x86-64 native clobber gate.

use super::super::x86_jit_mem_address_shape_valid;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, MemWidth, VReg};

/// Admit only scalar MMU-helper transfers that the x86-64 state-backed
/// lowerer can reconstruct without allocator-owned values. The subsequent
/// generic clobber checks reject RSP/RBP destinations; those registers remain
/// valid address components and store sources because helpers read them from
/// `GuestRegs` rather than the host stack/frame registers.
pub(crate) fn x86_jit_scalar_mem_shape_valid(op: &OpKind) -> bool {
    let state_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());
    let scalar_width = |width: &MemWidth| {
        matches!(
            width,
            MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8
        )
    };

    match op {
        OpKind::Load {
            dst, addr, width, ..
        } => state_gpr(dst) && scalar_width(width) && x86_jit_mem_address_shape_valid(addr),
        OpKind::Store { src, addr, width } => {
            (state_gpr(src) || matches!(src, VReg::Imm(_)))
                && scalar_width(width)
                && x86_jit_mem_address_shape_valid(addr)
        }
        _ => false,
    }
}
