//! Runtime admission for native x86 packed rotates.

/// Exact register-only VPROL[DQ]/VPROR[DQ] and
/// VPROLV[DQ]/VPRORV[DQ] semantic shape. Immediate forms have no count vector;
/// variable forms use a same-width count vector and reserve `amount = 0`.
pub(crate) fn x86_packed_rotate_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::X86PackedRotate {
        dst,
        src,
        count,
        mask,
        amount,
        width,
        elem,
        zeroing,
        ..
    } = op
    else {
        return false;
    };
    let vector_matches_width = |reg: &VReg| {
        matches!(
            (reg, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                VecWidth::V512
            )
        )
    };

    vector_matches_width(dst)
        && vector_matches_width(src)
        && count.is_none_or(|count| vector_matches_width(&count) && *amount == 0)
        && matches!(elem, VecElementType::I32 | VecElementType::I64)
        && !(*zeroing && mask.is_none())
        && mask.is_none_or(|mask| matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
}
