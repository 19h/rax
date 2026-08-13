//! Native identity-map validation for x86 register `XCHG`.

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

/// Accept register-only exchanges that map directly onto x86-64/AArch64 host
/// registers. Guest RSP/RBP and APX EGPRs use the x86 host's state-backed path
/// and therefore remain outside this identity-map predicate.
pub(crate) fn x86_xchg_shape_valid(op: &OpKind) -> bool {
    let native_gpr = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Rax
                    | X86Reg::Rcx
                    | X86Reg::Rdx
                    | X86Reg::Rbx
                    | X86Reg::Rsi
                    | X86Reg::Rdi
                    | X86Reg::R8
                    | X86Reg::R9
                    | X86Reg::R10
                    | X86Reg::R11
                    | X86Reg::R12
                    | X86Reg::R13
                    | X86Reg::R14
                    | X86Reg::R15
            ))
        )
    };

    matches!(
        op,
        OpKind::Xchg {
            reg1,
            reg2,
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } if native_gpr(reg1) && native_gpr(reg2)
    )
}
