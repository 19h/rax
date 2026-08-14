//! trampolines::misc tests

use super::*;
use crate::smir::lower::runtime::*;

#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx512er() -> bool {
    // CPUID.(EAX=07H,ECX=0):EBX[27] enumerates AVX512ER. AVX-512 OS-state
    // enablement is checked independently through the AVX512F runtime probe.
    unsafe {
        let max_leaf = std::arch::x86_64::__cpuid(0).eax;
        max_leaf >= 7 && std::arch::x86_64::__cpuid_count(7, 0).ebx & (1 << 27) != 0
    }
}
pub(crate) fn x86_cwd_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

    matches!(
        op,
        OpKind::Cwd {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        }
    )
}
pub(crate) fn x86_carry_rotate_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::flags::{FlagSet, FlagUpdate};
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

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
    let defined_flags = FlagSet::CF.union(FlagSet::OF);

    matches!(
        op,
        OpKind::Rcl {
            dst,
            src,
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            flags: FlagUpdate::Specific(set),
        }
        | OpKind::Rcr {
            dst,
            src,
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            flags: FlagUpdate::Specific(set),
        } if native_gpr(dst) && native_gpr(src) && *set == defined_flags
    )
}
pub(crate) fn x86_bswap_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

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
        OpKind::Bswap {
            dst,
            src,
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
        } if native_gpr(dst) && native_gpr(src)
    )
}
fn x86_mulx_shape_valid_for(
    op: &crate::smir::ir::ops::SmirOp,
    valid_gpr: impl Fn(&crate::smir::ir::types::VReg) -> bool,
) -> bool {
    use crate::smir::ir::flags::FlagUpdate;
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg, X86Reg};

    matches!(op.x86_hint, Some(X86OpHint::Mulx))
        && matches!(
                &op.kind,
                OpKind::MulU {
                    dst_lo,
                    dst_hi: Some(dst_hi),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32 | OpWidth::W64,
                    flags: FlagUpdate::None,
                } if valid_gpr(dst_lo) && valid_gpr(dst_hi) && valid_gpr(src2)
        )
}

/// Validate a `MULX` that can use the x86-64 native identity GPR map directly.
pub(crate) fn x86_mulx_shape_valid(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

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

    x86_mulx_shape_valid_for(op, native_gpr)
}

/// Validate architectural `MULX` operands independently of a host register
/// map. The x86-64 gate applies its stricter identity/state-backed split,
/// whereas the AArch64 cross-lowerer can identity-map all 16 legacy GPRs.
pub(crate) fn x86_mulx_arch_shape_valid(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::types::{ArchReg, VReg};

    let arch_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());

    x86_mulx_shape_valid_for(op, arch_gpr)
}
