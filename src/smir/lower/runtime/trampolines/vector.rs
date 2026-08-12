//! trampolines::vector tests

use super::*;
use crate::smir::lower::runtime::*;

/// Return whether `op` is one of the register-only x86 vector operations whose
/// interpreter semantics and native EVEX encoding are both regression-covered.
/// Every operand must be architectural: virtual vector values still require a
/// separate vector allocator/spill discipline and therefore remain ineligible.
pub(crate) fn x86_packed_shift_imm_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShiftImm {
        dst,
        src,
        width,
        elem,
        shift,
        byte_lane,
        ..
    } = op
    else {
        return false;
    };
    let valid_vector = |reg: &VReg| {
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
    let valid_operation = if *byte_lane {
        *elem == VecElementType::I8 && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr)
    } else {
        matches!(
            elem,
            VecElementType::I16 | VecElementType::I32 | VecElementType::I64
        ) && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr)
    };
    valid_vector(dst) && valid_vector(src) && valid_operation
}
pub(crate) fn x86_packed_shift_imm_feature_requirements(
    op: &crate::smir::ir::ops::OpKind,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShiftImm {
        dst,
        src,
        width,
        elem,
        shift,
        ..
    } = op
    else {
        return (false, false, false);
    };
    let high = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
            ))
        )
    };
    let evex = *width == VecWidth::V512
        || high(dst)
        || high(src)
        || (*elem == VecElementType::I64 && *shift == ShiftOp::Asr);
    if evex {
        (false, false, *width != VecWidth::V512)
    } else {
        (*width == VecWidth::V128, *width == VecWidth::V256, false)
    }
}
pub(crate) fn x86_packed_shift_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShift {
        dst,
        src,
        count,
        width,
        elem,
        shift,
    } = op
    else {
        return false;
    };
    let vector = |reg: &VReg| {
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
    vector(dst)
        && vector(src)
        && matches!(count, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))))
        && matches!(
            elem,
            VecElementType::I16 | VecElementType::I32 | VecElementType::I64
        )
        && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr)
}
pub(crate) fn x86_packed_shift_feature_requirements(
    op: &crate::smir::ir::ops::OpKind,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, ShiftOp, VReg, VecElementType, VecWidth, X86Reg};
    let OpKind::X86PackedShift {
        dst,
        src,
        count,
        width,
        elem,
        shift,
    } = op
    else {
        return (false, false, false);
    };
    let high = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
            ))
        )
    };
    let evex = *width == VecWidth::V512
        || high(dst)
        || high(src)
        || high(count)
        || (*elem == VecElementType::I64 && *shift == ShiftOp::Asr);
    if evex {
        (false, false, *width != VecWidth::V512)
    } else {
        (*width == VecWidth::V128, *width == VecWidth::V256, false)
    }
}
pub(crate) fn x86_vector_width_from_lanes(
    elem: crate::smir::ir::types::VecElementType,
    lanes: u8,
) -> Option<crate::smir::ir::types::VecWidth> {
    use crate::smir::ir::types::VecWidth;

    [VecWidth::V128, VecWidth::V256, VecWidth::V512]
        .into_iter()
        .find(|width| width.lanes(elem) as u8 == lanes)
}
/// Exact PAVGB/PAVGW and VPAVGB/VPAVGW semantic shape. The instructions use
/// unsigned lanes and round upward: `(a + b + 1) >> 1`. Restricting native
/// admission to this complete shape prevents the width-general Hexagon VLane
/// family from inheriting an unrelated x86 encoding.
pub(crate) fn x86_vector_integer_average_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VLaneOp, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::AvgRnd,
        signed: false,
        set_ovf: false,
    } = op
    else {
        return false;
    };
    if !matches!(elem, VecElementType::I8 | VecElementType::I16) {
        return false;
    }
    let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
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
    [dst, src1, src2].into_iter().all(vector_matches_width)
}
/// Exact PSIGNB/PSIGNW/PSIGND and VPSIGNB/VPSIGNW/VPSIGND semantic shape.
/// This operation uses signed control lanes but wrapping negation of the data
/// lane, and the ISA exposes only legacy 128-bit and VEX 128/256-bit forms.
pub(crate) fn x86_vector_integer_sign_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VLaneOp, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::Sign,
        signed: true,
        set_ovf: false,
    } = op
    else {
        return false;
    };
    if !matches!(
        elem,
        VecElementType::I8 | VecElementType::I16 | VecElementType::I32
    ) {
        return false;
    }
    let Some(width @ (VecWidth::V128 | VecWidth::V256)) =
        x86_vector_width_from_lanes(*elem, *lanes)
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
            )
        )
    };
    [dst, src1, src2].into_iter().all(vector_matches_width)
}
/// Exact packed-integer PMIN*/PMAX* and VPMIN*/VPMAX* semantic shape. The
/// x86 encodings cover signed and unsigned byte, word, and dword lanes; qword
/// lanes exist only in EVEX. Architectural register-width matching prevents
/// unrelated Hexagon VLane operations from entering the x86 native path.
pub(crate) fn x86_vector_integer_minmax_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VLaneOp, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::Min | VLaneOp::Max,
        set_ovf: false,
        ..
    } = op
    else {
        return false;
    };
    if !matches!(
        elem,
        VecElementType::I8 | VecElementType::I16 | VecElementType::I32 | VecElementType::I64
    ) {
        return false;
    }
    let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
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
    [dst, src1, src2].into_iter().all(vector_matches_width)
}
pub(crate) fn x86_vector_integer_minmax_encoding(
    op: &crate::smir::ir::ops::OpKind,
) -> Option<(crate::smir::ir::ops::X86VecMap, u8)> {
    use crate::smir::ir::ops::{OpKind, X86VecMap};
    use crate::smir::ir::types::{VLaneOp, VecElementType};

    let OpKind::VLane {
        elem, op, signed, ..
    } = op
    else {
        return None;
    };
    match (*elem, *op, *signed) {
        (VecElementType::I8, VLaneOp::Min, false) => Some((X86VecMap::Map0F, 0xDA)),
        (VecElementType::I8, VLaneOp::Max, false) => Some((X86VecMap::Map0F, 0xDE)),
        (VecElementType::I16, VLaneOp::Min, true) => Some((X86VecMap::Map0F, 0xEA)),
        (VecElementType::I16, VLaneOp::Max, true) => Some((X86VecMap::Map0F, 0xEE)),
        (VecElementType::I8, VLaneOp::Min, true) => Some((X86VecMap::Map0F38, 0x38)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, true) => {
            Some((X86VecMap::Map0F38, 0x39))
        }
        (VecElementType::I16, VLaneOp::Min, false) => Some((X86VecMap::Map0F38, 0x3A)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, false) => {
            Some((X86VecMap::Map0F38, 0x3B))
        }
        (VecElementType::I8, VLaneOp::Max, true) => Some((X86VecMap::Map0F38, 0x3C)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, true) => {
            Some((X86VecMap::Map0F38, 0x3D))
        }
        (VecElementType::I16, VLaneOp::Max, false) => Some((X86VecMap::Map0F38, 0x3E)),
        (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, false) => {
            Some((X86VecMap::Map0F38, 0x3F))
        }
        _ => None,
    }
}
/// Exact PSADBW/VPSADBW semantic shape. Every consecutive group of eight
/// unsigned-byte absolute differences produces one zero-extended qword result.
/// Restricting admission to architectural registers of the declared width
/// excludes the width-general IR operation used by non-x86 source ISAs.
pub(crate) fn x86_vector_sad_bytes_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let OpKind::VSadBytes {
        dst,
        src1,
        src2,
        width,
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
    [dst, src1, src2].into_iter().all(vector_matches_width)
}
/// Exact register-only PHMINPOSUW/VPHMINPOSUW semantic shape. Both encodings
/// address only the architectural low 16 XMM registers. A separately
/// byte-validated sequence classifier admits the VEX memory-source pair while
/// preserving its load frontier.
pub(crate) fn x86_phminposuw_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, X86Reg};

    let OpKind::X86Phminposuw { dst, src } = op else {
        return false;
    };
    let low_xmm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))));
    low_xmm(dst) && low_xmm(src)
}
/// Exact MPSADBW/VMPSADBW semantic shape for the legacy and VEX encodings.
/// Each 128-bit lane produces eight unsigned-word sums from a four-byte
/// stationary block and eight sliding four-byte windows. AVX10.2 masking and
/// 512-bit replication are deliberately outside this classic shape.
pub(crate) fn x86_vector_mpsadbw_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let OpKind::VMpsadbw {
        dst,
        src1,
        src2,
        mask,
        width,
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
            )
        )
    };
    mask.is_none() && !zeroing && [dst, src1, src2].into_iter().all(vector_matches_width)
}
/// Exact non-accumulating PMADDUBSW/VPMADDUBSW semantic shape. `VReg::Imm(0)`
/// denotes the instruction's implicit all-zero word accumulator; unlike VNNI,
/// the architectural destination is not an input except in the legacy
/// destructive encoding where the lifter also names it as `src1`.
pub(crate) fn x86_vector_integer_maddubs_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask,
        src_elem,
        acc_elem,
        width,
        src1_unsigned,
        saturate,
        zeroing,
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

    *acc == VReg::Imm(0)
        && mask.is_none()
        && *src_elem == VecElementType::I8
        && *acc_elem == VecElementType::I16
        && *src1_unsigned
        && *saturate
        && !*zeroing
        && [dst, src1, src2].into_iter().all(vector_matches_width)
}
/// Exact non-accumulating PMADDWD/VPMADDWD semantic shape. The signed word
/// products are paired into wrapping signed dword sums; `VReg::Imm(0)` is the
/// canonical all-zero accumulator used to distinguish this from VNNI.
pub(crate) fn x86_vector_integer_maddwd_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask,
        src_elem,
        acc_elem,
        width,
        src1_unsigned,
        saturate,
        zeroing,
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

    *acc == VReg::Imm(0)
        && mask.is_none()
        && *src_elem == VecElementType::I16
        && *acc_elem == VecElementType::I32
        && !*src1_unsigned
        && !*saturate
        && !*zeroing
        && [dst, src1, src2].into_iter().all(vector_matches_width)
}
/// Exact PMULHW/PMULHUW/PMULHRSW semantic shapes. The first two retain bits
/// 31:16 of each signed or unsigned word product; PMULHRSW adds the
/// architectural 0x4000 rounding bias before an arithmetic right shift by 15.
pub(crate) fn x86_vector_integer_mul_shift_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecElementType, VecWidth, X86Reg};

    let OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem,
        lanes,
        signed1,
        signed2,
        shift_left,
        round,
        sat_bits,
        out_shift,
    } = op
    else {
        return false;
    };
    let width = match lanes {
        8 => VecWidth::V128,
        16 => VecWidth::V256,
        32 => VecWidth::V512,
        _ => return false,
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

    *src_elem == VecElementType::I16
        && *shift_left == 0
        && *sat_bits == 0
        && ((*signed1 && *signed2 && *round && *out_shift == 15)
            || (*signed1 == *signed2 && !*round && *out_shift == 16))
        && [dst, src1, src2].into_iter().all(vector_matches_width)
}
pub fn is_x86_native_vector_op(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    if let OpKind::X86Opmask(opmask) = op {
        return crate::smir::lower::x86_64::x86_opmask_native_shape_valid(opmask);
    }

    if !matches!(
        op,
        OpKind::VMov { .. }
            | OpKind::VAdd { .. }
            | OpKind::VSub { .. }
            | OpKind::VAddSubSat { .. }
            | OpKind::VMul { .. }
            | OpKind::VUnary { .. }
            | OpKind::VCmp { .. }
            | OpKind::VInterleave { .. }
            | OpKind::VPackSat { .. }
            | OpKind::VByteShuffle { .. }
            | OpKind::VHorizontalBin { .. }
            | OpKind::VMulShiftSat { .. }
            | OpKind::VLane { .. }
            | OpKind::VSadBytes { .. }
            | OpKind::X86Phminposuw { .. }
            | OpKind::X86MovMask { .. }
            | OpKind::X86MovdQ { .. }
            | OpKind::VMpsadbw { .. }
            | OpKind::VAnd { .. }
            | OpKind::VAndNot { .. }
            | OpKind::VOr { .. }
            | OpKind::VXor { .. }
            | OpKind::VPopcnt { .. }
            | OpKind::VShuffleBitQM { .. }
            | OpKind::VConflict { .. }
            | OpKind::VLeadingZeros { .. }
            | OpKind::X86PermuteBytesWords { .. }
            | OpKind::VCompress { .. }
            | OpKind::VExpand { .. }
            | OpKind::X86NarrowInt { .. }
            | OpKind::X86Aes { .. }
            | OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. }
            | OpKind::X86Sm3Msg1 { .. }
            | OpKind::X86Sm3Msg2 { .. }
            | OpKind::X86Sm3Rounds2 { .. }
            | OpKind::X86Sm4 { .. }
            | OpKind::X86PackedShiftImm { .. }
            | OpKind::X86PackedShift { .. }
            | OpKind::VDotProduct { .. }
            | OpKind::VDotProductBF16 { .. }
            | OpKind::VCvtFP32ToBF16 { .. }
            | OpKind::VFP16Arith { .. }
            | OpKind::X86GetExponent { .. }
            | OpKind::X86GetMantissa { .. }
            | OpKind::X86RoundScale { .. }
            | OpKind::X86Reduce { .. }
            | OpKind::X86Range { .. }
            | OpKind::X86FixupImm { .. }
            | OpKind::X86Exp2 { .. }
            | OpKind::X86Recip14 { .. }
            | OpKind::X86Rsqrt14 { .. }
            | OpKind::X86RecipFp16 { .. }
            | OpKind::X86RsqrtFp16 { .. }
            | OpKind::X86Recip28 { .. }
            | OpKind::X86Rsqrt28 { .. }
            | OpKind::X86ScaleF { .. }
            | OpKind::X86FP16Complex { .. }
            | OpKind::X86PackedIntToFp { .. }
            | OpKind::X86PackedFpToInt { .. }
            | OpKind::X86PackedIntToFp16 { .. }
            | OpKind::X86PackedFp16ToInt { .. }
            | OpKind::VMultiplyAdd52 { .. }
            | OpKind::X86PackedShiftVariable { .. }
            | OpKind::X86PackedRotate { .. }
            | OpKind::X86TernaryLogic { .. }
            | OpKind::X86PackedFunnelShift { .. }
            | OpKind::X86MultiShiftQB { .. }
    ) {
        return false;
    }

    if matches!(op, OpKind::VLane { .. })
        && !x86_vector_integer_average_shape_valid(op)
        && !x86_vector_integer_sign_shape_valid(op)
        && !x86_vector_integer_minmax_shape_valid(op)
    {
        return false;
    }

    if matches!(op, OpKind::VSadBytes { .. }) && !x86_vector_sad_bytes_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::X86Phminposuw { .. }) && !x86_phminposuw_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::X86MovMask { .. }) && !x86_mov_mask_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::X86MovdQ { .. }) && !x86_movd_q_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::VMpsadbw { .. }) && !x86_vector_mpsadbw_shape_valid(op) {
        return false;
    }

    if matches!(op, OpKind::VMulShiftSat { .. }) && !x86_vector_integer_mul_shift_shape_valid(op) {
        return false;
    }

    if let OpKind::VMov { dst, src, width } = op {
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
        if !vector_matches_width(dst) || !vector_matches_width(src) {
            return false;
        }
    }

    if let OpKind::VAnd {
        dst,
        src1,
        src2,
        width,
    }
    | OpKind::VAndNot {
        dst,
        src1,
        src2,
        width,
    }
    | OpKind::VOr {
        dst,
        src1,
        src2,
        width,
    }
    | OpKind::VXor {
        dst,
        src1,
        src2,
        width,
    } = op
    {
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
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VAdd {
        dst,
        src1,
        src2,
        elem,
        lanes,
    }
    | OpKind::VSub {
        dst,
        src1,
        src2,
        elem,
        lanes,
    }
    | OpKind::VAddSubSat {
        dst,
        src1,
        src2,
        elem,
        lanes,
        ..
    }
    | OpKind::VMul {
        dst,
        src1,
        src2,
        elem,
        lanes,
    } = op
    {
        let elem_valid = if matches!(op, OpKind::VAddSubSat { .. }) {
            matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
            )
        } else if matches!(op, OpKind::VMul { .. }) {
            matches!(
                elem,
                crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
        } else {
            matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
        };
        if !elem_valid {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
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
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VCmp {
        dst,
        src1,
        src2,
        cond,
        elem,
        lanes,
    } = op
    {
        if !matches!(
            (elem, cond),
            (
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecCmpCond::Eq | crate::smir::ir::types::VecCmpCond::Gt
            )
        ) {
            return false;
        }
        let Some(width @ (VecWidth::V128 | VecWidth::V256)) =
            x86_vector_width_from_lanes(*elem, *lanes)
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
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VInterleave {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        ..
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::I8
                | crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) || *block_lanes != (16 / elem.bytes()) as u8
        {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
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
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VPackSat {
        dst,
        src1,
        src2,
        src_elem,
        src_lanes,
        block_lanes,
        ..
    } = op
    {
        if !matches!(
            src_elem,
            crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
        ) || *block_lanes != (16 / src_elem.bytes()) as u8
        {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*src_elem, *src_lanes) else {
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
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        block_lanes,
    } = op
    {
        if *block_lanes != 16 {
            return false;
        }
        let Some(width) =
            x86_vector_width_from_lanes(crate::smir::ir::types::VecElementType::I8, *lanes)
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
        if ![dst, src, control].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VHorizontalBin {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        saturating,
        ..
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
        ) || *block_lanes != (16 / elem.bytes()) as u8
            || (*saturating && *elem != crate::smir::ir::types::VecElementType::I16)
        {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        if width == VecWidth::V512 {
            return false;
        }
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(vector_matches_width) {
            return false;
        }
    }

    if let OpKind::VUnary {
        dst,
        src,
        elem,
        lanes,
        op: crate::smir::ir::types::VecUnaryOp::Abs,
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::I8
                | crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) {
            return false;
        }
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
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
        if !vector_matches_width(dst) || !vector_matches_width(src) {
            return false;
        }
    } else if matches!(op, OpKind::VUnary { .. }) {
        return false;
    }

    if matches!(op, OpKind::X86PackedShiftImm { .. }) && !x86_packed_shift_imm_shape_valid(op) {
        return false;
    }
    if matches!(op, OpKind::X86PackedShift { .. }) && !x86_packed_shift_shape_valid(op) {
        return false;
    }
    if matches!(op, OpKind::X86PackedRotate { .. }) && !x86_packed_rotate_shape_valid(op) {
        return false;
    }

    if let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        src_elem,
        acc_elem,
        src1_unsigned,
        mask,
        width,
        zeroing,
        saturate,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        let vnni = dst == acc
            && [dst, acc, src1, src2].into_iter().all(valid_vector)
            && *acc_elem == crate::smir::ir::types::VecElementType::I32
            && *width != crate::smir::ir::types::VecWidth::V64
            && !(*zeroing && mask.is_none())
            && !matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
            && matches!(
                (src_elem, src1_unsigned),
                (crate::smir::ir::types::VecElementType::I8, true)
                    | (crate::smir::ir::types::VecElementType::I16, false)
            );
        let _ = saturate; // Both VNNI saturation modes are structurally valid.
        if !vnni
            && !x86_vector_integer_maddubs_shape_valid(op)
            && !x86_vector_integer_maddwd_shape_valid(op)
        {
            return false;
        }
    }

    if let OpKind::VConflict {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        if !valid_vector(dst)
            || !valid_vector(src)
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
        {
            return false;
        }
    }

    if let OpKind::VLeadingZeros {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        if !valid_vector(dst)
            || !valid_vector(src)
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            )
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86PermuteBytesWords {
        dst,
        table1,
        table2,
        indices,
        mask,
        elem,
        width,
        overwrite_table,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        let valid_alias = match table2 {
            None => !overwrite_table,
            Some(_) if *overwrite_table => dst == table1,
            Some(_) => dst == indices,
        };
        if ![dst, table1, indices].into_iter().all(valid_vector)
            || table2.is_some_and(|reg| !valid_vector(&reg))
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
            )
            || !valid_alias
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::VCompress {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    }
    | OpKind::VExpand {
        dst,
        src,
        mask,
        elem,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        if !valid_vector(dst)
            || !valid_vector(src)
            || !matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
                    | crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
                    | crate::smir::ir::types::VecElementType::F32
                    | crate::smir::ir::types::VecElementType::F64
            )
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86NarrowInt {
        dst,
        src,
        mask,
        src_elem,
        dst_elem,
        width,
        zeroing,
        ..
    } = op
    {
        let valid_source = matches!(
            (src, width),
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                crate::smir::ir::types::VecWidth::V128
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                crate::smir::ir::types::VecWidth::V256
            ) | (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                crate::smir::ir::types::VecWidth::V512
            )
        );
        let valid_pair = matches!(
            (src_elem, dst_elem),
            (
                crate::smir::ir::types::VecElementType::I16,
                crate::smir::ir::types::VecElementType::I8
            ) | (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::I8
            ) | (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::I8
            ) | (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::I16
            ) | (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::I16
            ) | (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::I32
            )
        );
        let output_bytes = width.lanes(*src_elem) * dst_elem.bytes();
        let valid_destination = if output_bytes <= 16 {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))))
        } else {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))))
        };
        if !valid_source
            || !valid_pair
            || !valid_destination
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86Aes {
        dst,
        src1,
        src2,
        width,
        op,
        imm,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        let valid = match op {
            crate::smir::ir::types::X86AesOp::Enc
            | crate::smir::ir::types::X86AesOp::EncLast
            | crate::smir::ir::types::X86AesOp::Dec
            | crate::smir::ir::types::X86AesOp::DecLast => {
                *imm == 0
                    && valid_vector(dst)
                    && valid_vector(src1)
                    && src2.is_some_and(|reg| valid_vector(&reg))
            }
            crate::smir::ir::types::X86AesOp::InvMixColumns
            | crate::smir::ir::types::X86AesOp::KeygenAssist => {
                *width == crate::smir::ir::types::VecWidth::V128
                    && matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
                    && matches!(src1, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
                    && src2.is_none()
                    && (*op == crate::smir::ir::types::X86AesOp::KeygenAssist || *imm == 0)
            }
        };
        if !valid {
            return false;
        }
    }

    let valid_sha512 = match op {
        OpKind::X86Sha512Msg1 { dst, src } => {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
        }
        OpKind::X86Sha512Msg2 { dst, src } => {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
        }
        OpKind::X86Sha512Rounds2 { dst, state, wk } => {
            matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(state, VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))))
                && matches!(wk, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))
        }
        _ => true,
    };
    if !valid_sha512 {
        return false;
    }

    let valid_sm3 = match op {
        OpKind::X86Sm3Msg1 { dst, src1, src2 } | OpKind::X86Sm3Msg2 { dst, src1, src2 } => {
            [dst, src1, src2]
                .into_iter()
                .all(|reg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15)))))
        }
        OpKind::X86Sm3Rounds2 {
            dst, state, words, ..
        } => [dst, state, words]
            .into_iter()
            .all(|reg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))))),
        _ => true,
    };
    if !valid_sm3 {
        return false;
    }

    if let OpKind::X86Sm4 {
        dst,
        src1,
        src2,
        width,
        ..
    } = op
    {
        let valid = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=15))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=15))),
                    crate::smir::ir::types::VecWidth::V256
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(valid) {
            return false;
        }
    }

    if let OpKind::VMultiplyAdd52 {
        dst,
        acc,
        src1,
        src2,
        mask,
        width,
        zeroing,
        ..
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        if dst != acc
            || ![dst, acc, src1, src2].into_iter().all(valid_vector)
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
        {
            return false;
        }
    }

    if let OpKind::VDotProductBF16 {
        dst,
        acc,
        src1,
        src2,
        mask,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(index) | X86Reg::Ymm(index) | X86Reg::Zmm(index)
                )) if *index <= 31
            )
        };
        if dst != acc
            || ![dst, acc, src1, src2].into_iter().all(valid_vector)
            || *width == crate::smir::ir::types::VecWidth::V64
            || (*zeroing && mask.is_none())
            || matches!(
                mask,
                Some(VReg::Arch(ArchReg::X86(X86Reg::K(0 | 8..=u8::MAX))))
            )
        {
            return false;
        }
    }

    if let OpKind::VCvtFP32ToBF16 {
        dst,
        src1,
        src2,
        mask,
        width,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg, expected: crate::smir::ir::types::VecWidth| {
            matches!(
                (reg, expected),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        let output_width = match (width, src2.is_some()) {
            (crate::smir::ir::types::VecWidth::V128, _) => crate::smir::ir::types::VecWidth::V128,
            (crate::smir::ir::types::VecWidth::V256, false) => {
                crate::smir::ir::types::VecWidth::V128
            }
            (crate::smir::ir::types::VecWidth::V256, true) => {
                crate::smir::ir::types::VecWidth::V256
            }
            (crate::smir::ir::types::VecWidth::V512, false) => {
                crate::smir::ir::types::VecWidth::V256
            }
            (crate::smir::ir::types::VecWidth::V512, true) => {
                crate::smir::ir::types::VecWidth::V512
            }
            (crate::smir::ir::types::VecWidth::V64, _) => return false,
        };
        if !valid_vector(dst, output_width)
            || !valid_vector(src1, *width)
            || src2.is_some_and(|src2| !valid_vector(&src2, *width))
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::VFP16Arith {
        dst,
        src1,
        src2,
        mask,
        op,
        round,
        width,
        lanes,
        zeroing,
    } = op
    {
        let valid_vector = |reg: &VReg| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    crate::smir::ir::types::VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    crate::smir::ir::types::VecWidth::V512
                )
            )
        };
        if ![dst, src1, src2].into_iter().all(valid_vector)
            || !matches!(
                op,
                crate::smir::ir::types::Avx10FP16Op::Add
                    | crate::smir::ir::types::Avx10FP16Op::Sub
                    | crate::smir::ir::types::Avx10FP16Op::Mul
                    | crate::smir::ir::types::Avx10FP16Op::Div
                    | crate::smir::ir::types::Avx10FP16Op::Min
                    | crate::smir::ir::types::Avx10FP16Op::Max
            )
            || *width == crate::smir::ir::types::VecWidth::V64
            || u32::from(*lanes) != width.lanes(crate::smir::ir::types::VecElementType::F16)
            || *round != crate::smir::ir::types::FpRoundMode::Dynamic
            || (*zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86GetExponent {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        suppress_exceptions,
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F16
                | crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        let valid_shape = if *scalar {
            *width == VecWidth::V128
                && *lanes == 1
                && vector_matches_width(dst, VecWidth::V128)
                && vector_matches_width(src, VecWidth::V128)
                && merge.is_some_and(|reg| vector_matches_width(&reg, VecWidth::V128))
        } else {
            *lanes == width.lanes(*elem) as u8
                && vector_matches_width(dst, *width)
                && vector_matches_width(src, *width)
                && merge.is_none()
                && (!*suppress_exceptions || *width == VecWidth::V512)
        };
        if !valid_shape {
            return false;
        }
    }

    if let OpKind::X86GetMantissa {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        suppress_exceptions,
        ..
    }
    | OpKind::X86RoundScale {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        suppress_exceptions,
        ..
    }
    | OpKind::X86Reduce {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        suppress_exceptions,
        ..
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F16
                | crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        let valid_shape = if *scalar {
            *width == VecWidth::V128
                && *lanes == 1
                && vector_matches_width(dst, VecWidth::V128)
                && vector_matches_width(src, VecWidth::V128)
                && merge.is_some_and(|reg| vector_matches_width(&reg, VecWidth::V128))
        } else {
            *lanes == width.lanes(*elem) as u8
                && vector_matches_width(dst, *width)
                && vector_matches_width(src, *width)
                && merge.is_none()
                && (!*suppress_exceptions || *width == VecWidth::V512)
        };
        if !valid_shape {
            return false;
        }
    }

    if let OpKind::X86PackedIntToFp {
        dst,
        src,
        mask,
        int_elem,
        fp_elem,
        signed,
        lanes,
        src_width,
        dst_width,
        mask_zeroing,
        zero_upper,
        round,
        suppress_exceptions,
    } = op
    {
        if !matches!(
            int_elem,
            crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) || !matches!(
            fp_elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) {
            return false;
        }
        let operation_bytes = u32::from(*lanes) * int_elem.bytes().max(fp_elem.bytes());
        let operation_width = match operation_bytes {
            16 => VecWidth::V128,
            32 => VecWidth::V256,
            64 => VecWidth::V512,
            _ => return false,
        };
        let exact_width = |bytes: u32| match bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let register_width = |bytes: u32| match bytes {
            0..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let expected_src_width = exact_width(u32::from(*lanes) * int_elem.bytes());
        let expected_dst_width = register_width(u32::from(*lanes) * fp_elem.bytes());
        let vector_matches_width = |reg: &VReg, width: VecWidth| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V64 | VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        let low_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(0..=15) | X86Reg::Ymm(0..=15) | X86Reg::Zmm(0..=15)
                ))
            )
        };
        let exact_no_er = *int_elem == crate::smir::ir::types::VecElementType::I32
            && *fp_elem == crate::smir::ir::types::VecElementType::F64;
        let legacy_shape = *signed
            && *int_elem == crate::smir::ir::types::VecElementType::I32
            && operation_width == VecWidth::V128
            && mask.is_none()
            && !*mask_zeroing
            && *round == crate::smir::ir::types::FpRoundMode::Dynamic
            && !*suppress_exceptions
            && low_vector(dst)
            && low_vector(src);
        if !vector_matches_width(src, expected_src_width)
            || !vector_matches_width(dst, expected_dst_width)
            || *src_width != expected_src_width
            || *dst_width != expected_dst_width
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || *round == crate::smir::ir::types::FpRoundMode::RoundNearestTiesAway
            || *suppress_exceptions != (*round != crate::smir::ir::types::FpRoundMode::Dynamic)
            || (*suppress_exceptions && (operation_width != VecWidth::V512 || exact_no_er))
            || (!*zero_upper && !legacy_shape)
        {
            return false;
        }
    }

    if let OpKind::X86ScaleF {
        dst,
        src1,
        src2,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        round,
        suppress_exceptions,
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F16
                | crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || (*suppress_exceptions != (*round != crate::smir::ir::types::FpRoundMode::Dynamic))
            || !matches!(
                round,
                crate::smir::ir::types::FpRoundMode::Dynamic
                    | crate::smir::ir::types::FpRoundMode::RoundNearest
                    | crate::smir::ir::types::FpRoundMode::RoundDown
                    | crate::smir::ir::types::FpRoundMode::RoundUp
                    | crate::smir::ir::types::FpRoundMode::RoundTowardZero
            )
        {
            return false;
        }
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        let register_width = if *scalar { VecWidth::V128 } else { *width };
        if !vector_matches_width(dst, register_width)
            || !vector_matches_width(src1, register_width)
            || !vector_matches_width(src2, register_width)
            || if *scalar {
                *width != VecWidth::V128 || *lanes != 1
            } else {
                *lanes != width.lanes(*elem) as u8
                    || (*suppress_exceptions && *width != VecWidth::V512)
            }
        {
            return false;
        }
    }

    if let OpKind::X86Range {
        dst,
        src1,
        src2,
        mask,
        elem,
        width,
        lanes,
        imm,
        scalar,
        mask_zeroing,
        suppress_exceptions,
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || *imm > 0x0F
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        let register_width = if *scalar { VecWidth::V128 } else { *width };
        if !vector_matches_width(dst, register_width)
            || !vector_matches_width(src1, register_width)
            || !vector_matches_width(src2, register_width)
            || if *scalar {
                *width != VecWidth::V128 || *lanes != 1
            } else {
                *lanes != width.lanes(*elem) as u8
                    || (*suppress_exceptions && *width != VecWidth::V512)
            }
        {
            return false;
        }
    }

    if let OpKind::X86FixupImm {
        dst,
        src1,
        src2,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        suppress_exceptions,
        ..
    } = op
    {
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        let register_width = if *scalar { VecWidth::V128 } else { *width };
        if !vector_matches_width(dst, register_width)
            || !vector_matches_width(src1, register_width)
            || !vector_matches_width(src2, register_width)
            || if *scalar {
                *width != VecWidth::V128 || *lanes != 1
            } else {
                *lanes != width.lanes(*elem) as u8
                    || (*suppress_exceptions && *width != VecWidth::V512)
            }
        {
            return false;
        }
    }

    if let OpKind::X86Exp2 {
        dst,
        src,
        mask,
        elem,
        width,
        lanes,
        mask_zeroing,
        ..
    } = op
    {
        let vector_matches_width =
            |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))));
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || *width != VecWidth::V512
            || *lanes != width.lanes(*elem) as u8
            || !vector_matches_width(dst)
            || !vector_matches_width(src)
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
        {
            return false;
        }
    }

    if let OpKind::X86Recip14 {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
    }
    | OpKind::X86Rsqrt14 {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
    } = op
    {
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || !vector_matches_width(dst, *width)
            || !vector_matches_width(src, *width)
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || if *scalar {
                *width != VecWidth::V128
                    || *lanes != 1
                    || !matches!(merge, Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31)))))
            } else {
                *lanes != width.lanes(*elem) as u8 || merge.is_some()
            }
        {
            return false;
        }
    }

    if let OpKind::X86RecipFp16 {
        dst,
        merge,
        src,
        mask,
        width,
        lanes,
        scalar,
        mask_zeroing,
    }
    | OpKind::X86RsqrtFp16 {
        dst,
        merge,
        src,
        mask,
        width,
        lanes,
        scalar,
        mask_zeroing,
    } = op
    {
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        if !vector_matches_width(dst, *width)
            || !vector_matches_width(src, *width)
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || if *scalar {
                *width != VecWidth::V128
                    || *lanes != 1
                    || !matches!(merge, Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31)))))
            } else {
                *lanes != width.lanes(crate::smir::ir::types::VecElementType::F16) as u8
                    || merge.is_some()
            }
        {
            return false;
        }
    }

    if let OpKind::X86Recip28 {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        ..
    } = op
    {
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || !vector_matches_width(dst, *width)
            || !vector_matches_width(src, *width)
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || if *scalar {
                *width != VecWidth::V128
                    || *lanes != 1
                    || !matches!(merge, Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31)))))
            } else {
                *width != VecWidth::V512 || *lanes != width.lanes(*elem) as u8 || merge.is_some()
            }
        {
            return false;
        }
    }

    if let OpKind::X86Rsqrt28 {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        ..
    } = op
    {
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        if !matches!(
            elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) || !vector_matches_width(dst, *width)
            || !vector_matches_width(src, *width)
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || if *scalar {
                *width != VecWidth::V128
                    || *lanes != 1
                    || !matches!(merge, Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31)))))
            } else {
                *width != VecWidth::V512 || *lanes != width.lanes(*elem) as u8 || merge.is_some()
            }
        {
            return false;
        }
    }

    if let OpKind::X86FP16Complex {
        dst,
        src1,
        src2,
        mask,
        width,
        pairs,
        scalar,
        mask_zeroing,
        round,
        ..
    } = op
    {
        let vector_matches_width = |reg: &VReg, expected: VecWidth| {
            matches!(
                (reg, expected),
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
        let register_width = if *scalar { VecWidth::V128 } else { *width };
        if !vector_matches_width(dst, register_width)
            || !vector_matches_width(src1, register_width)
            || !vector_matches_width(src2, register_width)
            || dst == src1
            || dst == src2
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || !matches!(
                round,
                crate::smir::ir::types::FpRoundMode::Dynamic
                    | crate::smir::ir::types::FpRoundMode::RoundNearest
                    | crate::smir::ir::types::FpRoundMode::RoundDown
                    | crate::smir::ir::types::FpRoundMode::RoundUp
                    | crate::smir::ir::types::FpRoundMode::RoundTowardZero
            )
            || if *scalar {
                *width != VecWidth::V128 || *pairs != 1
            } else {
                *pairs != (width.bytes() / 4) as u8
                    || (*round != crate::smir::ir::types::FpRoundMode::Dynamic
                        && *width != VecWidth::V512)
            }
        {
            return false;
        }
    }

    if let OpKind::X86PackedFpToInt {
        dst,
        src,
        mask,
        fp_elem,
        int_elem,
        signed,
        truncate,
        lanes,
        src_width,
        dst_width,
        mask_zeroing,
        zero_upper,
        round,
        suppress_exceptions,
    } = op
    {
        if !matches!(
            int_elem,
            crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) || !matches!(
            fp_elem,
            crate::smir::ir::types::VecElementType::F32
                | crate::smir::ir::types::VecElementType::F64
        ) {
            return false;
        }
        let operation_bytes = u32::from(*lanes) * int_elem.bytes().max(fp_elem.bytes());
        let operation_width = match operation_bytes {
            16 => VecWidth::V128,
            32 => VecWidth::V256,
            64 => VecWidth::V512,
            _ => return false,
        };
        let exact_width = |bytes: u32| match bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let register_width = |bytes: u32| match bytes {
            0..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let expected_src_width = exact_width(u32::from(*lanes) * fp_elem.bytes());
        let expected_dst_width = register_width(u32::from(*lanes) * int_elem.bytes());
        let vector_matches_width = |reg: &VReg, width: VecWidth| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V64 | VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        let low_vector = |reg: &VReg| {
            matches!(
                reg,
                VReg::Arch(ArchReg::X86(
                    X86Reg::Xmm(0..=15) | X86Reg::Ymm(0..=15) | X86Reg::Zmm(0..=15)
                ))
            )
        };
        let rounding_valid = if *truncate {
            *round == crate::smir::ir::types::FpRoundMode::RoundTowardZero
        } else {
            *round != crate::smir::ir::types::FpRoundMode::RoundNearestTiesAway
                && *suppress_exceptions == (*round != crate::smir::ir::types::FpRoundMode::Dynamic)
        };
        let legacy_shape = *signed
            && *int_elem == crate::smir::ir::types::VecElementType::I32
            && operation_width == VecWidth::V128
            && mask.is_none()
            && !*mask_zeroing
            && !*suppress_exceptions
            && low_vector(dst)
            && low_vector(src);
        if !vector_matches_width(src, expected_src_width)
            || !vector_matches_width(dst, expected_dst_width)
            || *src_width != expected_src_width
            || *dst_width != expected_dst_width
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || !rounding_valid
            || (*suppress_exceptions && operation_width != VecWidth::V512)
            || (!*zero_upper && !legacy_shape)
        {
            return false;
        }
    }

    if let OpKind::X86PackedIntToFp16 {
        dst,
        src,
        mask,
        int_elem,
        signed: _,
        lanes,
        src_width,
        dst_width,
        mask_zeroing,
        zero_upper,
        round,
        suppress_exceptions,
    } = op
    {
        if !matches!(
            int_elem,
            crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) {
            return false;
        }
        let expected_lanes = src_width.lanes(*int_elem) as u8;
        let dst_bytes = u32::from(expected_lanes) * 2;
        let expected_dst_width = match dst_bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let vector_matches_width = |reg: &VReg, width: VecWidth| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V64 | VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        if !vector_matches_width(src, *src_width)
            || !vector_matches_width(dst, expected_dst_width)
            || *lanes != expected_lanes
            || *dst_width != expected_dst_width
            || !*zero_upper
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || *round == crate::smir::ir::types::FpRoundMode::RoundNearestTiesAway
            || *suppress_exceptions != (*round != crate::smir::ir::types::FpRoundMode::Dynamic)
            || (*suppress_exceptions && *src_width != VecWidth::V512)
        {
            return false;
        }
    }

    if let OpKind::X86PackedFp16ToInt {
        dst,
        src,
        mask,
        int_elem,
        signed: _,
        truncate,
        lanes,
        src_width,
        dst_width,
        mask_zeroing,
        zero_upper,
        round,
        suppress_exceptions,
    } = op
    {
        if !matches!(
            int_elem,
            crate::smir::ir::types::VecElementType::I16
                | crate::smir::ir::types::VecElementType::I32
                | crate::smir::ir::types::VecElementType::I64
        ) {
            return false;
        }
        let expected_lanes = dst_width.lanes(*int_elem) as u8;
        let src_bytes = u32::from(expected_lanes) * 2;
        let expected_src_width = match src_bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let vector_matches_width = |reg: &VReg, width: VecWidth| {
            matches!(
                (reg, width),
                (
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(0..=31))),
                    VecWidth::V64 | VecWidth::V128
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Ymm(0..=31))),
                    VecWidth::V256
                ) | (
                    VReg::Arch(ArchReg::X86(X86Reg::Zmm(0..=31))),
                    VecWidth::V512
                )
            )
        };
        let rounding_valid = if *truncate {
            *round == crate::smir::ir::types::FpRoundMode::RoundTowardZero
        } else {
            *suppress_exceptions == (*round != crate::smir::ir::types::FpRoundMode::Dynamic)
                && *round != crate::smir::ir::types::FpRoundMode::RoundNearestTiesAway
        };
        if !vector_matches_width(src, expected_src_width)
            || !vector_matches_width(dst, *dst_width)
            || *lanes != expected_lanes
            || *src_width != expected_src_width
            || !*zero_upper
            || (*mask_zeroing && mask.is_none())
            || mask.is_some_and(|mask| !matches!(mask, VReg::Arch(ArchReg::X86(X86Reg::K(1..=7)))))
            || !rounding_valid
            || (*suppress_exceptions && *dst_width != VecWidth::V512)
        {
            return false;
        }
    }

    let non_accumulating_madd =
        x86_vector_integer_maddubs_shape_valid(op) || x86_vector_integer_maddwd_shape_valid(op);
    if matches!(op, OpKind::X86MovMask { .. } | OpKind::X86MovdQ { .. }) {
        // Shape validation above already checked the scalar GPR destination
        // or source and the vector counterpart; the generic vector-only
        // register filter below intentionally cannot describe mixed families.
        return true;
    }
    op.dests().into_iter().chain(op.source_vregs()).all(|reg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(_) | X86Reg::Ymm(_) | X86Reg::Zmm(_) | X86Reg::K(_)
            ))
        ) || (non_accumulating_madd && reg == VReg::Imm(0))
    })
}
/// Admit only exact destructive register-register MMX operations. The classic
/// encoding metadata is part of the contract: V64 IR alone is insufficient to
/// distinguish MMX from malformed or synthetic vector operations. Exact m64
/// source sequences are admitted separately by the helper-backed MMX gate.
pub fn is_x86_native_mmx_op(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::ops::{OpKind, X86OpHint, X86SsePrefix};
    use crate::smir::ir::types::{
        ArchReg, OpWidth, ShiftOp, SignExtend, VLaneOp, VReg, VecCmpCond, VecElementType,
        VecUnaryOp, VecWidth, X86Reg,
    };

    if let OpKind::VMov { dst, src, width } = &op.kind {
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if mm(dst) || mm(src) {
            return *width == VecWidth::V64
                && mm(dst)
                && mm(src)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseMov {
                        prefix: X86SsePrefix::None,
                        opcode: 0x6F | 0x7F,
                    })
                );
        }
    }

    if let OpKind::VInsertLane {
        dst,
        vec,
        scalar,
        lane,
        elem,
    } = &op.kind
    {
        let safe_gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 15 && !matches!(index, 4 | 5)));
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if mm(dst) || mm(vec) {
            return dst == vec
                && mm(dst)
                && mm(vec)
                && safe_gpr(scalar)
                && *lane < 4
                && *elem == VecElementType::I16
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0xC4,
                    })
                );
        }
    }

    if let OpKind::VExtractLane {
        dst,
        vec,
        lane,
        elem,
        sign,
    } = &op.kind
    {
        let safe_gpr = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 15 && !matches!(index, 4 | 5)));
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if mm(vec) {
            return safe_gpr(dst)
                && *lane < 4
                && *elem == VecElementType::I16
                && *sign == SignExtend::Zero
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0xC5,
                    })
                );
        }
    }

    if let OpKind::X86PackedShuffleImm {
        dst,
        src,
        width,
        elem,
        high_words,
        ..
    } = &op.kind
    {
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if mm(dst) || mm(src) {
            return *width == VecWidth::V64
                && *elem == VecElementType::I16
                && high_words.is_none()
                && mm(dst)
                && mm(src)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x70,
                    })
                );
        }
    }

    if let OpKind::X86PackedAlignRight {
        dst,
        high,
        low,
        width,
        ..
    } = &op.kind
    {
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if [dst, high, low].into_iter().any(mm) {
            return *width == VecWidth::V64
                && dst == high
                && [dst, high, low].into_iter().all(mm)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x0F,
                    })
                );
        }
    }

    if matches!(op.kind, OpKind::X86MovdQ { .. }) {
        return crate::smir::lower::x86_64::x86_native_mmx_movd_q_shape_valid(op);
    }

    if let OpKind::X86MovMask {
        dst,
        src,
        elem,
        lanes,
        dst_width,
    } = &op.kind
    {
        let safe_gpr = matches!(
            dst,
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
        );
        let mm = matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if mm {
            return safe_gpr
                && *elem == VecElementType::I8
                && *lanes == 8
                && matches!(dst_width, OpWidth::W32 | OpWidth::W64)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0xD7,
                    })
                );
        }
    }

    if let OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        block_lanes,
    } = &op.kind
    {
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        return *lanes == 8
            && *block_lanes == 8
            && dst == src
            && [dst, src, control].into_iter().all(mm)
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x00,
                })
            );
    }

    if let OpKind::VUnary {
        dst,
        src,
        elem,
        lanes,
        op: VecUnaryOp::Abs,
    } = &op.kind
    {
        let expected = match (*elem, *lanes) {
            (VecElementType::I8, 8) => Some(0x1C),
            (VecElementType::I16, 4) => Some(0x1D),
            (VecElementType::I32, 2) => Some(0x1E),
            _ => None,
        };
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        return expected.is_some()
            && mm(dst)
            && mm(src)
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                }) if Some(opcode) == expected
            );
    }

    if let OpKind::VDotProduct {
        dst,
        acc,
        src1,
        src2,
        mask,
        src_elem,
        acc_elem,
        width,
        src1_unsigned,
        saturate,
        zeroing,
    } = &op.kind
    {
        let exact_maddubs = *acc == VReg::Imm(0)
            && mask.is_none()
            && *src_elem == VecElementType::I8
            && *acc_elem == VecElementType::I16
            && *width == VecWidth::V64
            && *src1_unsigned
            && *saturate
            && !*zeroing;
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if exact_maddubs {
            return dst == src1
                && [dst, src1, src2].into_iter().all(mm)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x04,
                    })
                );
        }
    }

    if let OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem,
        lanes,
        signed1,
        signed2,
        shift_left,
        round,
        sat_bits,
        out_shift,
    } = &op.kind
    {
        let exact_mulhrsw = *src_elem == VecElementType::I16
            && *lanes == 4
            && *signed1
            && *signed2
            && *shift_left == 0
            && *round
            && *sat_bits == 0
            && *out_shift == 15;
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        if exact_mulhrsw {
            return dst == src1
                && [dst, src1, src2].into_iter().all(mm)
                && matches!(
                    op.x86_hint,
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0x0B,
                    })
                );
        }
    }

    if let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::Sign,
        signed,
        set_ovf,
    } = &op.kind
    {
        let expected = match (*elem, *lanes, *signed, *set_ovf) {
            (VecElementType::I8, 8, true, false) => Some(0x08),
            (VecElementType::I16, 4, true, false) => Some(0x09),
            (VecElementType::I32, 2, true, false) => Some(0x0A),
            _ => None,
        };
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        return expected.is_some()
            && dst == src1
            && [dst, src1, src2].into_iter().all(mm)
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                }) if Some(opcode) == expected
            );
    }

    if let OpKind::VHorizontalBin {
        dst,
        src1,
        src2,
        elem,
        lanes,
        block_lanes,
        subtract,
        saturating,
    } = &op.kind
    {
        let expected = match (*elem, *lanes, *block_lanes, *subtract, *saturating) {
            (VecElementType::I16, 4, 4, false, false) => Some(0x01),
            (VecElementType::I32, 2, 2, false, false) => Some(0x02),
            (VecElementType::I16, 4, 4, false, true) => Some(0x03),
            (VecElementType::I16, 4, 4, true, false) => Some(0x05),
            (VecElementType::I32, 2, 2, true, false) => Some(0x06),
            (VecElementType::I16, 4, 4, true, true) => Some(0x07),
            _ => None,
        };
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        return expected.is_some()
            && dst == src1
            && [dst, src1, src2].into_iter().all(mm)
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                }) if Some(opcode) == expected
            );
    }

    if let OpKind::X86PackedShiftImm {
        dst,
        src,
        width,
        elem,
        shift,
        byte_lane,
        ..
    } = &op.kind
    {
        let expected = match (*width, *elem, *shift, *byte_lane) {
            (VecWidth::V64, VecElementType::I16, ShiftOp::Lsr, false) => Some(0x71),
            (VecWidth::V64, VecElementType::I16, ShiftOp::Asr, false) => Some(0x71),
            (VecWidth::V64, VecElementType::I16, ShiftOp::Lsl, false) => Some(0x71),
            (VecWidth::V64, VecElementType::I32, ShiftOp::Lsr, false) => Some(0x72),
            (VecWidth::V64, VecElementType::I32, ShiftOp::Asr, false) => Some(0x72),
            (VecWidth::V64, VecElementType::I32, ShiftOp::Lsl, false) => Some(0x72),
            (VecWidth::V64, VecElementType::I64, ShiftOp::Lsr, false) => Some(0x73),
            (VecWidth::V64, VecElementType::I64, ShiftOp::Lsl, false) => Some(0x73),
            _ => None,
        };
        let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));
        return expected.is_some()
            && dst == src
            && mm(dst)
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                }) if Some(opcode) == expected
            );
    }

    let (dst, src1, src2, expected_opcode) = match &op.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xDB)),
        OpKind::VAndNot {
            dst,
            src1,
            src2,
            width,
        } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xDF)),
        OpKind::VOr {
            dst,
            src1,
            src2,
            width,
        } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xEB)),
        OpKind::VXor {
            dst,
            src1,
            src2,
            width,
        } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xEF)),
        OpKind::VAdd {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => (
            dst,
            src1,
            src2,
            match (*elem, *lanes) {
                (VecElementType::I8, 8) => Some(0xFC),
                (VecElementType::I16, 4) => Some(0xFD),
                (VecElementType::I32, 2) => Some(0xFE),
                (VecElementType::I64, 1) => Some(0xD4),
                _ => None,
            },
        ),
        OpKind::VSub {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => (
            dst,
            src1,
            src2,
            match (*elem, *lanes) {
                (VecElementType::I8, 8) => Some(0xF8),
                (VecElementType::I16, 4) => Some(0xF9),
                (VecElementType::I32, 2) => Some(0xFA),
                (VecElementType::I64, 1) => Some(0xFB),
                _ => None,
            },
        ),
        OpKind::VAddSubSat {
            dst,
            src1,
            src2,
            elem,
            lanes,
            subtract,
            signed,
        } => (
            dst,
            src1,
            src2,
            match (*elem, *lanes, *subtract, *signed) {
                (VecElementType::I8, 8, false, true) => Some(0xEC),
                (VecElementType::I16, 4, false, true) => Some(0xED),
                (VecElementType::I8, 8, false, false) => Some(0xDC),
                (VecElementType::I16, 4, false, false) => Some(0xDD),
                (VecElementType::I8, 8, true, true) => Some(0xE8),
                (VecElementType::I16, 4, true, true) => Some(0xE9),
                (VecElementType::I8, 8, true, false) => Some(0xD8),
                (VecElementType::I16, 4, true, false) => Some(0xD9),
                _ => None,
            },
        ),
        OpKind::VCmp {
            dst,
            src1,
            src2,
            cond,
            elem,
            lanes,
        } => (
            dst,
            src1,
            src2,
            match (*elem, *lanes, *cond) {
                (VecElementType::I8, 8, VecCmpCond::Gt) => Some(0x64),
                (VecElementType::I16, 4, VecCmpCond::Gt) => Some(0x65),
                (VecElementType::I32, 2, VecCmpCond::Gt) => Some(0x66),
                (VecElementType::I8, 8, VecCmpCond::Eq) => Some(0x74),
                (VecElementType::I16, 4, VecCmpCond::Eq) => Some(0x75),
                (VecElementType::I32, 2, VecCmpCond::Eq) => Some(0x76),
                _ => None,
            },
        ),
        OpKind::VInterleave {
            dst,
            src1,
            src2,
            elem,
            lanes,
            block_lanes,
            high,
        } => (
            dst,
            src1,
            src2,
            match (*elem, *lanes, *block_lanes, *high) {
                (VecElementType::I8, 8, 8, false) => Some(0x60),
                (VecElementType::I16, 4, 4, false) => Some(0x61),
                (VecElementType::I32, 2, 2, false) => Some(0x62),
                (VecElementType::I8, 8, 8, true) => Some(0x68),
                (VecElementType::I16, 4, 4, true) => Some(0x69),
                (VecElementType::I32, 2, 2, true) => Some(0x6A),
                _ => None,
            },
        ),
        OpKind::VPackSat {
            dst,
            src1,
            src2,
            src_elem,
            to_unsigned,
            src_lanes,
            block_lanes,
        } => (
            dst,
            src2,
            src1,
            match (*src_elem, *src_lanes, *block_lanes, *to_unsigned) {
                (VecElementType::I16, 4, 4, false) => Some(0x63),
                (VecElementType::I16, 4, 4, true) => Some(0x67),
                (VecElementType::I32, 2, 2, false) => Some(0x6B),
                _ => None,
            },
        ),
        OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op: lane_op,
            signed,
            set_ovf,
        } => (
            dst,
            src1,
            src2,
            match (*elem, *lanes, *lane_op, *signed, *set_ovf) {
                (VecElementType::I8, 8, VLaneOp::Min, false, false) => Some(0xDA),
                (VecElementType::I8, 8, VLaneOp::Max, false, false) => Some(0xDE),
                (VecElementType::I16, 4, VLaneOp::Min, true, false) => Some(0xEA),
                (VecElementType::I16, 4, VLaneOp::Max, true, false) => Some(0xEE),
                (VecElementType::I8, 8, VLaneOp::AvgRnd, false, false) => Some(0xE0),
                (VecElementType::I16, 4, VLaneOp::AvgRnd, false, false) => Some(0xE3),
                _ => None,
            },
        ),
        OpKind::VDotProduct {
            dst,
            acc,
            src1,
            src2,
            mask,
            src_elem,
            acc_elem,
            width,
            src1_unsigned,
            saturate,
            zeroing,
        } => (
            dst,
            src1,
            src2,
            (*acc == VReg::Imm(0)
                && mask.is_none()
                && *src_elem == VecElementType::I16
                && *acc_elem == VecElementType::I32
                && *width == VecWidth::V64
                && !*src1_unsigned
                && !*saturate
                && !*zeroing)
                .then_some(0xF5),
        ),
        OpKind::VSadBytes {
            dst,
            src1,
            src2,
            width,
        } => (dst, src1, src2, (*width == VecWidth::V64).then_some(0xF6)),
        OpKind::X86PackedShift {
            dst,
            src,
            count,
            width,
            elem,
            shift,
        } => (
            dst,
            src,
            count,
            match (*width, *elem, *shift) {
                (VecWidth::V64, VecElementType::I16, ShiftOp::Lsr) => Some(0xD1),
                (VecWidth::V64, VecElementType::I32, ShiftOp::Lsr) => Some(0xD2),
                (VecWidth::V64, VecElementType::I64, ShiftOp::Lsr) => Some(0xD3),
                (VecWidth::V64, VecElementType::I16, ShiftOp::Asr) => Some(0xE1),
                (VecWidth::V64, VecElementType::I32, ShiftOp::Asr) => Some(0xE2),
                (VecWidth::V64, VecElementType::I16, ShiftOp::Lsl) => Some(0xF1),
                (VecWidth::V64, VecElementType::I32, ShiftOp::Lsl) => Some(0xF2),
                (VecWidth::V64, VecElementType::I64, ShiftOp::Lsl) => Some(0xF3),
                _ => None,
            },
        ),
        OpKind::VMul {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => (
            dst,
            src1,
            src2,
            (*elem == VecElementType::I16 && *lanes == 4).then_some(0xD5),
        ),
        OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem,
            lanes,
            signed1,
            signed2,
            shift_left,
            round,
            sat_bits,
            out_shift,
        } => (
            dst,
            src1,
            src2,
            match (
                *src_elem,
                *lanes,
                *signed1,
                *signed2,
                *shift_left,
                *round,
                *sat_bits,
                *out_shift,
            ) {
                (VecElementType::I16, 4, false, false, 0, false, 0, 16) => Some(0xE4),
                (VecElementType::I16, 4, true, true, 0, false, 0, 16) => Some(0xE5),
                _ => None,
            },
        ),
        _ => return false,
    };
    let mm = |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(X86Reg::Mm(0..=7))));

    expected_opcode.is_some()
        && dst == src1
        && [dst, src1, src2].into_iter().all(mm)
        && matches!(
            op.x86_hint,
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode,
            }) if Some(opcode) == expected_opcode
        )
}
pub(crate) fn x86_vector_move_encoding_valid(
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;

    match opcode {
        0x10 | 0x11 | 0x28 | 0x29 => {
            matches!(prefix, X86SsePrefix::None | X86SsePrefix::OpSize)
        }
        0x6F | 0x7F => {
            matches!(prefix, X86SsePrefix::OpSize | X86SsePrefix::Rep)
                || evex && prefix == X86SsePrefix::Repne
        }
        _ => false,
    }
}
pub(crate) fn x86_vector_logic_encoding_valid(
    kind: &crate::smir::ir::ops::OpKind,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
    w: bool,
) -> bool {
    use crate::smir::ir::ops::{OpKind, X86SsePrefix};

    let opcode_matches_kind = matches!(
        (kind, opcode),
        (OpKind::VAnd { .. }, 0x54 | 0xDB)
            | (OpKind::VAndNot { .. }, 0x55 | 0xDF)
            | (OpKind::VOr { .. }, 0x56 | 0xEB)
            | (OpKind::VXor { .. }, 0x57 | 0xEF)
    );
    if !opcode_matches_kind {
        return false;
    }

    match opcode {
        0x54..=0x57 => {
            matches!(prefix, X86SsePrefix::None | X86SsePrefix::OpSize)
                && (!evex
                    || matches!(
                        (prefix, w),
                        (X86SsePrefix::None, false) | (X86SsePrefix::OpSize, true)
                    ))
        }
        0xDB | 0xDF | 0xEB | 0xEF => prefix == X86SsePrefix::OpSize,
        _ => false,
    }
}
pub(crate) fn x86_vector_integer_arithmetic_encoding_valid(
    kind: &crate::smir::ir::ops::OpKind,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
    w: bool,
) -> bool {
    use crate::smir::ir::ops::{OpKind, X86SsePrefix};
    use crate::smir::ir::types::VecElementType;

    let (elem, expected_opcode) = match kind {
        OpKind::VAdd { elem, .. } => (
            *elem,
            match elem {
                VecElementType::I8 => 0xFC,
                VecElementType::I16 => 0xFD,
                VecElementType::I32 => 0xFE,
                VecElementType::I64 => 0xD4,
                _ => return false,
            },
        ),
        OpKind::VSub { elem, .. } => (
            *elem,
            match elem {
                VecElementType::I8 => 0xF8,
                VecElementType::I16 => 0xF9,
                VecElementType::I32 => 0xFA,
                VecElementType::I64 => 0xFB,
                _ => return false,
            },
        ),
        OpKind::VAddSubSat {
            elem,
            subtract,
            signed,
            ..
        } => (
            *elem,
            match (*elem, *subtract, *signed) {
                (VecElementType::I8, false, true) => 0xEC,
                (VecElementType::I16, false, true) => 0xED,
                (VecElementType::I8, false, false) => 0xDC,
                (VecElementType::I16, false, false) => 0xDD,
                (VecElementType::I8, true, true) => 0xE8,
                (VecElementType::I16, true, true) => 0xE9,
                (VecElementType::I8, true, false) => 0xD8,
                (VecElementType::I16, true, false) => 0xD9,
                _ => return false,
            },
        ),
        OpKind::VMul { elem, .. } => (
            *elem,
            match elem {
                VecElementType::I16 => 0xD5,
                VecElementType::I32 | VecElementType::I64 => 0x40,
                _ => return false,
            },
        ),
        _ => return false,
    };
    prefix == X86SsePrefix::OpSize
        && opcode == expected_opcode
        && (!evex
            || match elem {
                VecElementType::I32 => !w,
                VecElementType::I64 => w,
                VecElementType::I8 | VecElementType::I16 => true,
                _ => false,
            })
}
pub(crate) fn x86_vector_integer_arithmetic_map_valid(
    kind: &crate::smir::ir::ops::OpKind,
    map: crate::smir::ir::ops::X86VecMap,
) -> bool {
    use crate::smir::ir::ops::{OpKind, X86VecMap};
    use crate::smir::ir::types::VecElementType;

    match kind {
        OpKind::VMul {
            elem: VecElementType::I32 | VecElementType::I64,
            ..
        } => map == X86VecMap::Map0F38,
        _ => map == X86VecMap::Map0F,
    }
}
pub(crate) fn x86_vector_integer_abs_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
    evex: bool,
    w: bool,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match elem {
                VecElementType::I8 => 0x1C,
                VecElementType::I16 => 0x1D,
                VecElementType::I32 => 0x1E,
                VecElementType::I64 => 0x1F,
                _ => return false,
            }
        && (!evex
            || match elem {
                VecElementType::I32 => !w,
                VecElementType::I64 => w,
                VecElementType::I8 | VecElementType::I16 => true,
                _ => false,
            })
}
pub(crate) fn x86_vector_integer_compare_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    cond: crate::smir::ir::types::VecCmpCond,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::{VecCmpCond, VecElementType};

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (elem, cond) {
                (VecElementType::I8, VecCmpCond::Gt) => 0x64,
                (VecElementType::I16, VecCmpCond::Gt) => 0x65,
                (VecElementType::I32, VecCmpCond::Gt) => 0x66,
                (VecElementType::I8, VecCmpCond::Eq) => 0x74,
                (VecElementType::I16, VecCmpCond::Eq) => 0x75,
                (VecElementType::I32, VecCmpCond::Eq) => 0x76,
                (VecElementType::I64, VecCmpCond::Eq) => 0x29,
                (VecElementType::I64, VecCmpCond::Gt) => 0x37,
                _ => return false,
            }
}
pub(crate) fn x86_vector_integer_interleave_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    high: bool,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (elem, high) {
                (VecElementType::I8, false) => 0x60,
                (VecElementType::I16, false) => 0x61,
                (VecElementType::I32, false) => 0x62,
                (VecElementType::I64, false) => 0x6C,
                (VecElementType::I8, true) => 0x68,
                (VecElementType::I16, true) => 0x69,
                (VecElementType::I32, true) => 0x6A,
                (VecElementType::I64, true) => 0x6D,
                _ => return false,
            }
}
pub(crate) fn x86_vector_integer_pack_encoding_valid(
    src_elem: crate::smir::ir::types::VecElementType,
    to_unsigned: bool,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (src_elem, to_unsigned) {
                (VecElementType::I16, false) => 0x63,
                (VecElementType::I16, true) => 0x67,
                (VecElementType::I32, false) => 0x6B,
                (VecElementType::I32, true) => 0x2B,
                _ => return false,
            }
}
pub(crate) fn x86_vector_integer_horizontal_encoding_valid(
    elem: crate::smir::ir::types::VecElementType,
    subtract: bool,
    saturating: bool,
    prefix: crate::smir::ir::ops::X86SsePrefix,
    opcode: u8,
) -> bool {
    use crate::smir::ir::ops::X86SsePrefix;
    use crate::smir::ir::types::VecElementType;

    prefix == X86SsePrefix::OpSize
        && opcode
            == match (elem, subtract, saturating) {
                (VecElementType::I16, false, false) => 0x01,
                (VecElementType::I32, false, false) => 0x02,
                (VecElementType::I16, false, true) => 0x03,
                (VecElementType::I16, true, false) => 0x05,
                (VecElementType::I32, true, false) => 0x06,
                (VecElementType::I16, true, true) => 0x07,
                _ => return false,
            }
}
/// Validate encoding metadata that can change the native opcode selected for
/// an otherwise well-formed architectural vector operation. This keeps
/// malformed SMIR from using native-vector admission to execute an arbitrary
/// hinted opcode.
pub(crate) fn x86_native_vector_smir_op(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecMap};
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    if !is_x86_native_vector_op(&op.kind) {
        return false;
    }
    if let OpKind::X86Opmask(opmask) = &op.kind {
        return op.x86_hint.is_none()
            && crate::smir::lower::x86_64::x86_opmask_native_shape_valid(opmask);
    }
    let low_vector = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(0..=15) | X86Reg::Ymm(0..=15) | X86Reg::Zmm(0..=15)
            ))
        )
    };

    if let OpKind::X86GetExponent {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let (expected_map, expected_w) = match elem {
            crate::smir::ir::types::VecElementType::F16 => (X86VecMap::Map6, false),
            crate::smir::ir::types::VecElementType::F32 => (X86VecMap::Map0F38, false),
            crate::smir::ir::types::VecElementType::F64 => (X86VecMap::Map0F38, true),
            _ => return false,
        };
        let expected_opcode = if *scalar { 0x43 } else { 0x42 };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w,
            }) if map == expected_map
                && opcode == expected_opcode
                && encoded_width == *width
                && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86GetMantissa {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let (expected_pp, expected_w) = match elem {
            crate::smir::ir::types::VecElementType::F16 => {
                (crate::smir::ir::ops::X86SsePrefix::None, false)
            }
            crate::smir::ir::types::VecElementType::F32 => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, false)
            }
            crate::smir::ir::types::VecElementType::F64 => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, true)
            }
            _ => return false,
        };
        let expected_opcode = if *scalar { 0x27 } else { 0x26 };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) if pp == expected_pp
                && opcode == expected_opcode
                && encoded_width == *width
                && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86RoundScale {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let (expected_pp, expected_opcode, expected_w) = match (elem, scalar) {
            (crate::smir::ir::types::VecElementType::F16, false) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x08, false)
            }
            (crate::smir::ir::types::VecElementType::F16, true) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x0A, false)
            }
            (crate::smir::ir::types::VecElementType::F32, false) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x08, false)
            }
            (crate::smir::ir::types::VecElementType::F32, true) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x0A, false)
            }
            (crate::smir::ir::types::VecElementType::F64, false) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x09, true)
            }
            (crate::smir::ir::types::VecElementType::F64, true) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x0B, true)
            }
            _ => return false,
        };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) if pp == expected_pp
                && opcode == expected_opcode
                && encoded_width == *width
                && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86Reduce {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let (expected_pp, expected_w) = match elem {
            crate::smir::ir::types::VecElementType::F16 => {
                (crate::smir::ir::ops::X86SsePrefix::None, false)
            }
            crate::smir::ir::types::VecElementType::F32 => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, false)
            }
            crate::smir::ir::types::VecElementType::F64 => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, true)
            }
            _ => return false,
        };
        let expected_opcode = if *scalar { 0x57 } else { 0x56 };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) if pp == expected_pp
                && opcode == expected_opcode
                && encoded_width == *width
                && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86Range {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let expected_w = match elem {
            crate::smir::ir::types::VecElementType::F32 => false,
            crate::smir::ir::types::VecElementType::F64 => true,
            _ => return false,
        };
        let expected_opcode = if *scalar { 0x51 } else { 0x50 };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w,
            }) if opcode == expected_opcode && encoded_width == *width && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86FixupImm {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let expected_w = match elem {
            crate::smir::ir::types::VecElementType::F32 => false,
            crate::smir::ir::types::VecElementType::F64 => true,
            _ => return false,
        };
        let expected_opcode = if *scalar { 0x55 } else { 0x54 };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w,
            }) if opcode == expected_opcode && encoded_width == *width && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86Exp2 { elem, width, .. } = &op.kind {
        let expected_w = match elem {
            crate::smir::ir::types::VecElementType::F32 => false,
            crate::smir::ir::types::VecElementType::F64 => true,
            _ => return false,
        };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode: 0xC8,
                width: encoded_width,
                w,
            }) if encoded_width == *width && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86Recip14 {
        elem,
        width,
        scalar,
        ..
    }
    | OpKind::X86Rsqrt14 {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let rsqrt = matches!(&op.kind, OpKind::X86Rsqrt14 { .. });
        let expected_w = match elem {
            crate::smir::ir::types::VecElementType::F32 => false,
            crate::smir::ir::types::VecElementType::F64 => true,
            _ => return false,
        };
        let expected_opcode = match (rsqrt, *scalar) {
            (false, false) => 0x4C,
            (false, true) => 0x4D,
            (true, false) => 0x4E,
            (true, true) => 0x4F,
        };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w,
            }) if opcode == expected_opcode && encoded_width == *width && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86RecipFp16 { width, scalar, .. } | OpKind::X86RsqrtFp16 { width, scalar, .. } =
        &op.kind
    {
        let rsqrt = matches!(&op.kind, OpKind::X86RsqrtFp16 { .. });
        let expected_opcode = match (rsqrt, *scalar) {
            (false, false) => 0x4C,
            (false, true) => 0x4D,
            (true, false) => 0x4E,
            (true, true) => 0x4F,
        };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map6,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w: false,
            }) if opcode == expected_opcode && encoded_width == *width
        ) {
            return false;
        }
    }

    if let OpKind::X86Recip28 {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let expected_w = match elem {
            crate::smir::ir::types::VecElementType::F32 => false,
            crate::smir::ir::types::VecElementType::F64 => true,
            _ => return false,
        };
        let expected_opcode = if *scalar { 0xCB } else { 0xCA };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w,
            }) if opcode == expected_opcode && encoded_width == *width && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86Rsqrt28 {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let expected_w = match elem {
            crate::smir::ir::types::VecElementType::F32 => false,
            crate::smir::ir::types::VecElementType::F64 => true,
            _ => return false,
        };
        let expected_opcode = if *scalar { 0xCD } else { 0xCC };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w,
            }) if opcode == expected_opcode && encoded_width == *width && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86ScaleF {
        elem,
        width,
        scalar,
        ..
    } = &op.kind
    {
        let (expected_map, expected_w) = match elem {
            crate::smir::ir::types::VecElementType::F16 => (X86VecMap::Map6, false),
            crate::smir::ir::types::VecElementType::F32 => (X86VecMap::Map0F38, false),
            crate::smir::ir::types::VecElementType::F64 => (X86VecMap::Map0F38, true),
            _ => return false,
        };
        let expected_opcode = if *scalar { 0x2D } else { 0x2C };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map,
                pp: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode,
                width: encoded_width,
                w,
            }) if map == expected_map
                && opcode == expected_opcode
                && encoded_width == *width
                && w == expected_w
        ) {
            return false;
        }
    }

    if let OpKind::X86FP16Complex {
        width,
        scalar,
        accumulate,
        conjugate,
        ..
    } = &op.kind
    {
        let expected_pp = if *conjugate {
            crate::smir::ir::ops::X86SsePrefix::Repne
        } else {
            crate::smir::ir::ops::X86SsePrefix::Rep
        };
        let expected_opcode = match (*accumulate, *scalar) {
            (true, false) => 0x56,
            (true, true) => 0x57,
            (false, false) => 0xD6,
            (false, true) => 0xD7,
        };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map6,
                pp,
                opcode,
                width: encoded_width,
                w: false,
            }) if pp == expected_pp && opcode == expected_opcode && encoded_width == *width
        ) {
            return false;
        }
    }

    if let OpKind::X86PackedIntToFp {
        dst,
        src,
        int_elem,
        fp_elem,
        signed,
        lanes,
        zero_upper,
        ..
    } = &op.kind
    {
        let (expected_pp, expected_opcode, expected_w) = match (int_elem, fp_elem, signed) {
            (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::F32,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::None, 0x5B, false),
            (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::F32,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::None, 0x5B, true),
            (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::F64,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::Rep, 0xE6, false),
            (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::F64,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::Rep, 0xE6, true),
            (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::F32,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::Repne, 0x7A, false),
            (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::F32,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::Repne, 0x7A, true),
            (
                crate::smir::ir::types::VecElementType::I32,
                crate::smir::ir::types::VecElementType::F64,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::Rep, 0x7A, false),
            (
                crate::smir::ir::types::VecElementType::I64,
                crate::smir::ir::types::VecElementType::F64,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::Rep, 0x7A, true),
            _ => return false,
        };
        let operation_width = match u32::from(*lanes) * int_elem.bytes().max(fp_elem.bytes()) {
            16 => VecWidth::V128,
            32 => VecWidth::V256,
            64 => VecWidth::V512,
            _ => return false,
        };
        let legacy_family = *signed
            && *int_elem == crate::smir::ir::types::VecElementType::I32
            && operation_width == VecWidth::V128;
        let vex_family = *signed
            && *int_elem == crate::smir::ir::types::VecElementType::I32
            && matches!(operation_width, VecWidth::V128 | VecWidth::V256);
        let valid_hint = match op.x86_hint {
            None => !*zero_upper && legacy_family && low_vector(dst) && low_vector(src),
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp,
                opcode,
                width,
                ..
            }) => {
                *zero_upper
                    && vex_family
                    && low_vector(dst)
                    && low_vector(src)
                    && pp == expected_pp
                    && opcode == expected_opcode
                    && width == operation_width
            }
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp,
                opcode,
                width,
                w,
            }) => {
                *zero_upper
                    && pp == expected_pp
                    && opcode == expected_opcode
                    && width == operation_width
                    && w == expected_w
            }
            _ => false,
        };
        if !valid_hint {
            return false;
        }
    }

    if let OpKind::X86PackedFpToInt {
        dst,
        src,
        fp_elem,
        int_elem,
        signed,
        truncate,
        lanes,
        zero_upper,
        ..
    } = &op.kind
    {
        let (expected_pp, expected_opcode, expected_w) = match (fp_elem, int_elem, signed, truncate)
        {
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I32,
                true,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x5B, false),
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I32,
                true,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::Rep, 0x5B, false),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I32,
                true,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::Repne, 0xE6, true),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I32,
                true,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0xE6, true),
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I64,
                true,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7B, false),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I64,
                true,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7B, true),
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I64,
                true,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7A, false),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I64,
                true,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7A, true),
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I32,
                false,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::None, 0x79, false),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I32,
                false,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::None, 0x79, true),
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I32,
                false,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::None, 0x78, false),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I32,
                false,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::None, 0x78, true),
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I64,
                false,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x79, false),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I64,
                false,
                false,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x79, true),
            (
                crate::smir::ir::types::VecElementType::F32,
                crate::smir::ir::types::VecElementType::I64,
                false,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x78, false),
            (
                crate::smir::ir::types::VecElementType::F64,
                crate::smir::ir::types::VecElementType::I64,
                false,
                true,
            ) => (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x78, true),
            _ => return false,
        };
        let operation_width = match u32::from(*lanes) * int_elem.bytes().max(fp_elem.bytes()) {
            16 => VecWidth::V128,
            32 => VecWidth::V256,
            64 => VecWidth::V512,
            _ => return false,
        };
        let legacy_family = *signed
            && *int_elem == crate::smir::ir::types::VecElementType::I32
            && operation_width == VecWidth::V128;
        let vex_family = *signed
            && *int_elem == crate::smir::ir::types::VecElementType::I32
            && matches!(operation_width, VecWidth::V128 | VecWidth::V256);
        let valid_hint = match op.x86_hint {
            None => !*zero_upper && legacy_family && low_vector(dst) && low_vector(src),
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp,
                opcode,
                width,
                ..
            }) => {
                *zero_upper
                    && vex_family
                    && low_vector(dst)
                    && low_vector(src)
                    && pp == expected_pp
                    && opcode == expected_opcode
                    && width == operation_width
            }
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp,
                opcode,
                width,
                w,
            }) => {
                *zero_upper
                    && pp == expected_pp
                    && opcode == expected_opcode
                    && width == operation_width
                    && w == expected_w
            }
            _ => false,
        };
        if !valid_hint {
            return false;
        }
    }

    if let OpKind::X86PackedIntToFp16 {
        int_elem,
        signed,
        src_width,
        ..
    } = &op.kind
    {
        let expected = match (int_elem, signed) {
            (crate::smir::ir::types::VecElementType::I16, true) => {
                (crate::smir::ir::ops::X86SsePrefix::Rep, 0x7D, false)
            }
            (crate::smir::ir::types::VecElementType::I16, false) => {
                (crate::smir::ir::ops::X86SsePrefix::Repne, 0x7D, false)
            }
            (crate::smir::ir::types::VecElementType::I32, true) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x5B, false)
            }
            (crate::smir::ir::types::VecElementType::I32, false) => {
                (crate::smir::ir::ops::X86SsePrefix::Repne, 0x7A, false)
            }
            (crate::smir::ir::types::VecElementType::I64, true) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x5B, true)
            }
            (crate::smir::ir::types::VecElementType::I64, false) => {
                (crate::smir::ir::ops::X86SsePrefix::Repne, 0x7A, true)
            }
            _ => return false,
        };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map5,
                pp,
                opcode,
                width,
                w,
            }) if (pp, opcode, w) == expected && width == *src_width
        ) {
            return false;
        }
    }

    if let OpKind::X86PackedFp16ToInt {
        int_elem,
        signed,
        truncate,
        dst_width,
        ..
    } = &op.kind
    {
        let expected = match (int_elem, signed, truncate) {
            (crate::smir::ir::types::VecElementType::I16, true, false) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7D)
            }
            (crate::smir::ir::types::VecElementType::I16, true, true) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7C)
            }
            (crate::smir::ir::types::VecElementType::I16, false, false) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x7D)
            }
            (crate::smir::ir::types::VecElementType::I16, false, true) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x7C)
            }
            (crate::smir::ir::types::VecElementType::I32, true, false) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x5B)
            }
            (crate::smir::ir::types::VecElementType::I32, true, true) => {
                (crate::smir::ir::ops::X86SsePrefix::Rep, 0x5B)
            }
            (crate::smir::ir::types::VecElementType::I32, false, false) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x79)
            }
            (crate::smir::ir::types::VecElementType::I32, false, true) => {
                (crate::smir::ir::ops::X86SsePrefix::None, 0x78)
            }
            (crate::smir::ir::types::VecElementType::I64, true, false) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7B)
            }
            (crate::smir::ir::types::VecElementType::I64, true, true) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x7A)
            }
            (crate::smir::ir::types::VecElementType::I64, false, false) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x79)
            }
            (crate::smir::ir::types::VecElementType::I64, false, true) => {
                (crate::smir::ir::ops::X86SsePrefix::OpSize, 0x78)
            }
            _ => return false,
        };
        if !matches!(
            op.x86_hint,
            Some(X86OpHint::EvexOp {
                map: X86VecMap::Map5,
                pp,
                opcode,
                width,
                w: false,
            }) if (pp, opcode) == expected && width == *dst_width
        ) {
            return false;
        }
    }

    if let OpKind::VMov { dst, src, width } = &op.kind {
        return match op.x86_hint {
            Some(X86OpHint::SseMov { prefix, opcode }) => {
                *width == VecWidth::V128
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_move_encoding_valid(prefix, opcode, false)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_move_encoding_valid(pp, opcode, false)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && x86_vector_move_encoding_valid(pp, opcode, true)
            }
            _ => false,
        };
    }

    let logic_operands = match &op.kind {
        OpKind::VAnd {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VAndNot {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VOr {
            dst,
            src1,
            src2,
            width,
        }
        | OpKind::VXor {
            dst,
            src1,
            src2,
            width,
        } => Some((dst, src1, src2, width)),
        _ => None,
    };
    if let Some((dst, src1, src2, width)) = logic_operands {
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_logic_encoding_valid(&op.kind, prefix, opcode, false, false)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_logic_encoding_valid(&op.kind, pp, opcode, false, w)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == *width
                    && x86_vector_logic_encoding_valid(&op.kind, pp, opcode, true, w)
            }
            _ => false,
        };
    }

    if let OpKind::VCmp {
        dst,
        src1,
        src2,
        cond,
        elem,
        lanes,
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        let expected_map = if *elem == crate::smir::ir::types::VecElementType::I64 {
            X86VecMap::Map0F38
        } else {
            X86VecMap::Map0F
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_compare_encoding_valid(*elem, *cond, prefix, opcode)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_compare_encoding_valid(*elem, *cond, pp, opcode)
            }
            _ => false,
        };
    }

    if let OpKind::VInterleave {
        dst,
        src1,
        src2,
        elem,
        lanes,
        high,
        ..
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_interleave_encoding_valid(*elem, *high, prefix, opcode)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_interleave_encoding_valid(*elem, *high, pp, opcode)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && encoded_width == width
                    && x86_vector_integer_interleave_encoding_valid(*elem, *high, pp, opcode)
                    && match elem {
                        crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16 => true,
                        crate::smir::ir::types::VecElementType::I32 => !w,
                        crate::smir::ir::types::VecElementType::I64 => w,
                        _ => false,
                    }
            }
            _ => false,
        };
    }

    if let OpKind::VPackSat {
        dst,
        src1,
        src2,
        src_elem,
        to_unsigned,
        src_lanes,
        ..
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*src_elem, *src_lanes) else {
            return false;
        };
        let expected_map =
            if *src_elem == crate::smir::ir::types::VecElementType::I32 && *to_unsigned {
                X86VecMap::Map0F38
            } else {
                X86VecMap::Map0F
            };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src2
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_pack_encoding_valid(
                        *src_elem,
                        *to_unsigned,
                        prefix,
                        opcode,
                    )
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_pack_encoding_valid(*src_elem, *to_unsigned, pp, opcode)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == expected_map
                    && encoded_width == width
                    && (*src_elem == crate::smir::ir::types::VecElementType::I16 || !w)
                    && x86_vector_integer_pack_encoding_valid(*src_elem, *to_unsigned, pp, opcode)
            }
            _ => false,
        };
    }

    if let OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        ..
    } = &op.kind
    {
        let Some(width) =
            x86_vector_width_from_lanes(crate::smir::ir::types::VecElementType::I8, *lanes)
        else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src
                    && [dst, src, control].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x00
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x00
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src, control].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x00
                    && encoded_width == width
            }
            _ => false,
        };
    }

    if let OpKind::VHorizontalBin {
        dst,
        src1,
        src2,
        elem,
        lanes,
        subtract,
        saturating,
        ..
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_horizontal_encoding_valid(
                        *elem,
                        *subtract,
                        *saturating,
                        prefix,
                        opcode,
                    )
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && x86_vector_integer_horizontal_encoding_valid(
                        *elem,
                        *subtract,
                        *saturating,
                        pp,
                        opcode,
                    )
            }
            _ => false,
        };
    }

    if x86_vector_integer_mul_shift_shape_valid(&op.kind) {
        let OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            lanes,
            signed1,
            round,
            ..
        } = &op.kind
        else {
            unreachable!("validated PMULH[RU]SW shape is VMulShiftSat");
        };
        let width = match lanes {
            8 => VecWidth::V128,
            16 => VecWidth::V256,
            32 => VecWidth::V512,
            _ => unreachable!("validated PMULH[RU]SW lane count"),
        };
        let (expected_map, expected_opcode) = if *round {
            (X86VecMap::Map0F38, 0x0B)
        } else if *signed1 {
            (X86VecMap::Map0F, 0xE5)
        } else {
            (X86VecMap::Map0F, 0xE4)
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
            }
            _ => false,
        };
    }

    if x86_vector_integer_average_shape_valid(&op.kind) {
        let OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } = &op.kind
        else {
            unreachable!("validated PAVG shape is a VLane");
        };
        let width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated PAVG shape has an x86 vector width");
        let expected_opcode = match elem {
            crate::smir::ir::types::VecElementType::I8 => 0xE0,
            crate::smir::ir::types::VecElementType::I16 => 0xE3,
            _ => unreachable!("validated PAVG element width"),
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
            }
            _ => false,
        };
    }

    if x86_vector_integer_sign_shape_valid(&op.kind) {
        let OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } = &op.kind
        else {
            unreachable!("validated PSIGN shape is a VLane");
        };
        let width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated PSIGN shape has an x86 vector width");
        let expected_opcode = match elem {
            crate::smir::ir::types::VecElementType::I8 => 0x08,
            crate::smir::ir::types::VecElementType::I16 => 0x09,
            crate::smir::ir::types::VecElementType::I32 => 0x0A,
            _ => unreachable!("validated PSIGN element width"),
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            _ => false,
        };
    }

    if x86_vector_integer_minmax_shape_valid(&op.kind) {
        let OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        } = &op.kind
        else {
            unreachable!("validated packed min/max shape is a VLane");
        };
        let width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated packed min/max shape has an x86 vector width");
        let (expected_map, expected_opcode) = x86_vector_integer_minmax_encoding(&op.kind)
            .expect("validated packed min/max shape has an x86 encoding");
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *elem != crate::smir::ir::types::VecElementType::I64
                    && width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                *elem != crate::smir::ir::types::VecElementType::I64
                    && map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == expected_map
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == width
                    && match elem {
                        crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16 => true,
                        crate::smir::ir::types::VecElementType::I32 => !w,
                        crate::smir::ir::types::VecElementType::I64 => w,
                        _ => false,
                    }
            }
            _ => false,
        };
    }

    if x86_vector_sad_bytes_shape_valid(&op.kind) {
        let OpKind::VSadBytes {
            dst,
            src1,
            src2,
            width,
        } = &op.kind
        else {
            unreachable!("validated PSADBW shape is VSadBytes");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF6
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF6
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF6
                    && encoded_width == *width
            }
            _ => false,
        };
    }

    if x86_phminposuw_shape_valid(&op.kind) {
        let OpKind::X86Phminposuw { dst, src } = &op.kind else {
            unreachable!("validated PHMINPOSUW shape is X86Phminposuw");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x41
                    && low_vector(dst)
                    && low_vector(src)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x41
                    && width == VecWidth::V128
                    && low_vector(dst)
                    && low_vector(src)
            }
            _ => false,
        };
    }

    if x86_movd_q_shape_valid(&op.kind) {
        let OpKind::X86MovdQ {
            dst,
            src,
            width,
            zero_upper,
        } = &op.kind
        else {
            unreachable!("validated MOVD/MOVQ shape is X86MovdQ");
        };
        let vector_dst = matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Xmm(_))));
        let xmm = if vector_dst { dst } else { src };
        let expected_opcode = if vector_dst { 0x6E } else { 0x7E };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && !*zero_upper
                    && low_vector(xmm)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == VecWidth::V128
                    && w == (*width == crate::smir::ir::types::OpWidth::W64)
                    && *zero_upper == vector_dst
                    && low_vector(xmm)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == expected_opcode
                    && encoded_width == VecWidth::V128
                    && w == (*width == crate::smir::ir::types::OpWidth::W64)
                    && *zero_upper == vector_dst
            }
            _ => false,
        };
    }

    if x86_mov_mask_shape_valid(&op.kind) {
        let OpKind::X86MovMask {
            elem,
            lanes,
            dst_width,
            ..
        } = &op.kind
        else {
            unreachable!("validated MOVMSK shape is X86MovMask");
        };
        let expected_width = x86_vector_width_from_lanes(*elem, *lanes)
            .expect("validated MOVMSK shape has an x86 vector width");
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                expected_width == VecWidth::V128
                    && matches!(
                        dst_width,
                        crate::smir::ir::types::OpWidth::W32 | crate::smir::ir::types::OpWidth::W64
                    )
                    && match (opcode, prefix, elem) {
                        (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::None,
                            crate::smir::ir::types::VecElementType::F32,
                        )
                        | (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::F64,
                        )
                        | (
                            0xD7,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::I8,
                        ) => true,
                        _ => false,
                    }
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && width == expected_width
                    && width != VecWidth::V512
                    && *dst_width == crate::smir::ir::types::OpWidth::W32
                    && match (opcode, pp, elem) {
                        (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::None,
                            crate::smir::ir::types::VecElementType::F32,
                        )
                        | (
                            0x50,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::F64,
                        )
                        | (
                            0xD7,
                            crate::smir::ir::ops::X86SsePrefix::OpSize,
                            crate::smir::ir::types::VecElementType::I8,
                        ) => true,
                        _ => false,
                    }
            }
            _ => false,
        };
    }

    if x86_vector_mpsadbw_shape_valid(&op.kind) {
        let OpKind::VMpsadbw {
            dst,
            src1,
            src2,
            width,
            ..
        } = &op.kind
        else {
            unreachable!("validated MPSADBW shape is VMpsadbw");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x42
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F3A
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x42
                    && encoded_width == *width
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            _ => false,
        };
    }

    if x86_vector_integer_maddubs_shape_valid(&op.kind) {
        let OpKind::VDotProduct {
            dst,
            src1,
            src2,
            width,
            ..
        } = &op.kind
        else {
            unreachable!("validated PMADDUBSW shape is a VDotProduct");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x04
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x04
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F38
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0x04
                    && encoded_width == *width
            }
            _ => false,
        };
    }

    if x86_vector_integer_maddwd_shape_valid(&op.kind) {
        let OpKind::VDotProduct {
            dst,
            src1,
            src2,
            width,
            ..
        } = &op.kind
        else {
            unreachable!("validated PMADDWD shape is a VDotProduct");
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                *width == VecWidth::V128
                    && dst == src1
                    && [dst, src1, src2].into_iter().all(low_vector)
                    && prefix == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF5
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF5
                    && encoded_width == *width
                    && *width != VecWidth::V512
                    && [dst, src1, src2].into_iter().all(low_vector)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                ..
            }) => {
                map == X86VecMap::Map0F
                    && pp == crate::smir::ir::ops::X86SsePrefix::OpSize
                    && opcode == 0xF5
                    && encoded_width == *width
            }
            _ => false,
        };
    }

    if let OpKind::VUnary {
        dst,
        src,
        elem,
        lanes,
        op: crate::smir::ir::types::VecUnaryOp::Abs,
    } = &op.kind
    {
        let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
            return false;
        };
        return match op.x86_hint {
            Some(X86OpHint::SseOp { prefix, opcode }) => {
                width == VecWidth::V128
                    && *elem != crate::smir::ir::types::VecElementType::I64
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_integer_abs_encoding_valid(*elem, prefix, opcode, false, false)
            }
            Some(X86OpHint::VexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F38
                    && encoded_width == width
                    && width != VecWidth::V512
                    && *elem != crate::smir::ir::types::VecElementType::I64
                    && low_vector(dst)
                    && low_vector(src)
                    && x86_vector_integer_abs_encoding_valid(*elem, pp, opcode, false, w)
            }
            Some(X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: encoded_width,
                w,
            }) => {
                map == X86VecMap::Map0F38
                    && encoded_width == width
                    && x86_vector_integer_abs_encoding_valid(*elem, pp, opcode, true, w)
            }
            _ => false,
        };
    }

    let (dst, src1, src2, elem, lanes) = match &op.kind {
        OpKind::VAdd {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VSub {
            dst,
            src1,
            src2,
            elem,
            lanes,
        }
        | OpKind::VAddSubSat {
            dst,
            src1,
            src2,
            elem,
            lanes,
            ..
        }
        | OpKind::VMul {
            dst,
            src1,
            src2,
            elem,
            lanes,
        } => (dst, src1, src2, elem, lanes),
        _ => return true,
    };
    let Some(width) = x86_vector_width_from_lanes(*elem, *lanes) else {
        return false;
    };

    match op.x86_hint {
        Some(X86OpHint::SseOp { prefix, opcode }) => {
            width == VecWidth::V128
                && dst == src1
                && [dst, src1, src2].into_iter().all(low_vector)
                && !matches!(
                    op.kind,
                    OpKind::VMul {
                        elem: crate::smir::ir::types::VecElementType::I64,
                        ..
                    }
                )
                && x86_vector_integer_arithmetic_encoding_valid(
                    &op.kind, prefix, opcode, false, false,
                )
        }
        Some(X86OpHint::VexOp {
            map,
            pp,
            opcode,
            width: encoded_width,
            w,
        }) => {
            x86_vector_integer_arithmetic_map_valid(&op.kind, map)
                && encoded_width == width
                && width != VecWidth::V512
                && [dst, src1, src2].into_iter().all(low_vector)
                && !matches!(
                    op.kind,
                    OpKind::VMul {
                        elem: crate::smir::ir::types::VecElementType::I64,
                        ..
                    }
                )
                && x86_vector_integer_arithmetic_encoding_valid(&op.kind, pp, opcode, false, w)
        }
        Some(X86OpHint::EvexOp {
            map,
            pp,
            opcode,
            width: encoded_width,
            w,
        }) => {
            x86_vector_integer_arithmetic_map_valid(&op.kind, map)
                && encoded_width == width
                && x86_vector_integer_arithmetic_encoding_valid(&op.kind, pp, opcode, true, w)
        }
        _ => false,
    }
}
pub(crate) fn x86_vector_move_needs_vl(op: &crate::smir::ir::ops::SmirOp) -> bool {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let OpKind::VMov { dst, src, width } = &op.kind else {
        return false;
    };
    if *width == VecWidth::V512 {
        return false;
    }
    let high_vector = |reg: &VReg| {
        matches!(
            reg,
            VReg::Arch(ArchReg::X86(
                X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
            ))
        )
    };
    high_vector(dst) || high_vector(src) || matches!(op.x86_hint, Some(X86OpHint::EvexOp { .. }))
}
/// Return `(AVX, AVX2, AVX-512DQ, AVX-512VL)` requirements for an admitted
/// architectural vector-logic operation. VEX integer logic needs AVX2 only at
/// 256 bits; EVEX floating logical encodings are in AVX-512DQ, while EVEX
/// integer D/Q encodings are in AVX-512F.
pub(crate) fn x86_vector_logic_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(
        op.kind,
        OpKind::VAnd { .. } | OpKind::VAndNot { .. } | OpKind::VOr { .. } | OpKind::VXor { .. }
    ) {
        return (false, false, false, false);
    }

    match op.x86_hint {
        Some(X86OpHint::VexOp { opcode, width, .. }) => {
            let integer_256 =
                width == VecWidth::V256 && matches!(opcode, 0xDB | 0xDF | 0xEB | 0xEF);
            (true, integer_256, false, false)
        }
        Some(X86OpHint::EvexOp { opcode, width, .. }) => (
            false,
            false,
            matches!(opcode, 0x54..=0x57),
            width != VecWidth::V512,
        ),
        _ => (false, false, false, false),
    }
}
/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted wrapping or
/// saturating packed-integer add/subtract operation.
pub(crate) fn x86_vector_integer_arithmetic_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(
        op.kind,
        OpKind::VAdd { .. } | OpKind::VSub { .. } | OpKind::VAddSubSat { .. }
    ) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}
/// Return `(SSE4.1, AVX, AVX2, AVX-512DQ, AVX-512VL)` requirements for an
/// admitted low-product packed-integer multiply operation. AVX-512F/BW are
/// already unconditional trampoline requirements.
pub(crate) fn x86_vector_integer_multiply_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{VecElementType, VecWidth};

    let OpKind::VMul { elem, .. } = op.kind else {
        return (false, false, false, false, false);
    };
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (elem == VecElementType::I32, false, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => {
            (false, true, width == VecWidth::V256, false, false)
        }
        Some(X86OpHint::EvexOp { width, .. }) => (
            false,
            false,
            false,
            elem == VecElementType::I64,
            width != VecWidth::V512,
        ),
        _ => (false, false, false, false, false),
    }
}
/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted packed
/// integer absolute-value operation. AVX-512F/BW are unconditional trampoline
/// requirements and cover EVEX dword/qword and byte/word forms respectively.
pub(crate) fn x86_vector_integer_abs_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(
        op.kind,
        OpKind::VUnary {
            op: crate::smir::ir::types::VecUnaryOp::Abs,
            ..
        }
    ) {
        return (false, false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}
/// Return `(SSE4.1, SSE4.2, AVX, AVX2)` requirements for an admitted fixed-
/// predicate packed-integer comparison. Byte/word/dword legacy forms use the
/// x86-64 baseline SSE2 feature.
pub(crate) fn x86_vector_integer_compare_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{VecCmpCond, VecElementType, VecWidth};

    let OpKind::VCmp { elem, cond, .. } = op.kind else {
        return (false, false, false, false);
    };
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (
            elem == VecElementType::I64 && cond == VecCmpCond::Eq,
            elem == VecElementType::I64 && cond == VecCmpCond::Gt,
            false,
            false,
        ),
        Some(X86OpHint::VexOp { width, .. }) => (false, false, true, width == VecWidth::V256),
        _ => (false, false, false, false),
    }
}
/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted packed-integer
/// interleave. Legacy forms use baseline SSE2; EVEX.512 forms do not need VL.
pub(crate) fn x86_vector_integer_interleave_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(op.kind, OpKind::VInterleave { .. }) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}
/// Return `(SSE4.1, AVX, AVX2, AVX-512VL)` requirements for an admitted packed
/// saturating narrow. Legacy PACKUSDW is the only non-baseline SSE2 form.
pub(crate) fn x86_vector_integer_pack_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::{VecElementType, VecWidth};

    let OpKind::VPackSat {
        src_elem,
        to_unsigned,
        ..
    } = op.kind
    else {
        return (false, false, false, false);
    };
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (
            src_elem == VecElementType::I32 && to_unsigned,
            false,
            false,
            false,
        ),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}
/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted packed
/// byte shuffle. EVEX.512 uses AVX-512BW without the VL extension.
pub(crate) fn x86_vector_byte_shuffle_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(op.kind, OpKind::VByteShuffle { .. }) {
        return (false, false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}
/// Return `(SSSE3, AVX, AVX2)` requirements for an admitted packed-integer
/// horizontal operation. VEX.128 uses AVX; VEX.256 uses AVX2.
pub(crate) fn x86_vector_integer_horizontal_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !matches!(op.kind, OpKind::VHorizontalBin { .. }) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256),
        _ => (false, false, false),
    }
}
/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PMULHW/PMULHUW/PMULHRSW operation. Legacy PMULHW/PMULHUW are baseline SSE2;
/// legacy PMULHRSW requires SSSE3. EVEX.512 uses the trampoline's AVX-512BW;
/// EVEX.128/256 additionally require AVX-512VL.
pub(crate) fn x86_vector_integer_mul_shift_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{OpKind, X86OpHint};
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_mul_shift_shape_valid(&op.kind) {
        return (false, false, false, false);
    }
    let legacy_needs_ssse3 = matches!(op.kind, OpKind::VMulShiftSat { round: true, .. });
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (legacy_needs_ssse3, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}
/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PAVGB/PAVGW/VPAVGB/VPAVGW. Legacy forms are baseline SSE2 on x86-64;
/// EVEX.512 uses the trampoline's unconditional AVX-512BW requirement.
pub(crate) fn x86_vector_integer_average_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_average_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}
/// Return `(SSSE3, AVX, AVX2)` requirements for an admitted
/// PSIGNB/PSIGNW/PSIGND or VPSIGNB/VPSIGNW/VPSIGND operation.
pub(crate) fn x86_vector_integer_sign_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_sign_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256),
        _ => (false, false, false),
    }
}
/// Return `(SSE4.1, AVX, AVX2, AVX-512VL)` requirements for an admitted
/// packed-integer minimum/maximum. The original unsigned-byte/signed-word
/// legacy encodings are baseline SSE2; other legacy encodings require SSE4.1.
pub(crate) fn x86_vector_integer_minmax_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::{X86OpHint, X86VecMap};
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_minmax_shape_valid(&op.kind) {
        return (false, false, false, false);
    }
    let map = x86_vector_integer_minmax_encoding(&op.kind)
        .map(|(map, _)| map)
        .expect("validated packed min/max shape has an x86 encoding");
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (map == X86VecMap::Map0F38, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}
/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PSADBW/VPSADBW. Legacy PSADBW is baseline SSE2 on x86-64; EVEX.512 uses
/// the trampoline's unconditional AVX-512BW requirement.
pub(crate) fn x86_vector_sad_bytes_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_sad_bytes_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}
/// Return `(SSE4.1, AVX)` requirements for an admitted register-only
/// PHMINPOSUW/VPHMINPOSUW operation.
pub(crate) fn x86_phminposuw_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool) {
    use crate::smir::ir::ops::X86OpHint;

    if !x86_phminposuw_shape_valid(&op.kind) {
        return (false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false),
        Some(X86OpHint::VexOp { .. }) => (false, true),
        _ => (false, false),
    }
}
/// Return `(SSE4.1, AVX, AVX2)` requirements for an admitted
/// MPSADBW/VMPSADBW. VEX.128 requires AVX; VEX.256 additionally requires
/// AVX2. AVX10.2 EVEX forms are not admitted by this classic path.
pub(crate) fn x86_vector_mpsadbw_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_mpsadbw_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256),
        _ => (false, false, false),
    }
}
/// Return `(SSSE3, AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PMADDUBSW/VPMADDUBSW. EVEX.512 requires AVX-512BW, which the vector-state
/// trampoline already requires unconditionally; EVEX.128/256 additionally
/// require AVX-512VL.
pub(crate) fn x86_vector_integer_maddubs_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_maddubs_shape_valid(&op.kind) {
        return (false, false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (true, false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (false, true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, false, width != VecWidth::V512),
        _ => (false, false, false, false),
    }
}
/// Return `(AVX, AVX2, AVX-512VL)` requirements for an admitted
/// PMADDWD/VPMADDWD. Legacy PMADDWD is baseline SSE2 on x86-64. EVEX.512 uses
/// AVX-512BW, already required by the vector-state trampoline; EVEX.128/256
/// additionally require AVX-512VL.
pub(crate) fn x86_vector_integer_maddwd_feature_requirements(
    op: &crate::smir::ir::ops::SmirOp,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::ir::types::VecWidth;

    if !x86_vector_integer_maddwd_shape_valid(&op.kind) {
        return (false, false, false);
    }
    match op.x86_hint {
        Some(X86OpHint::SseOp { .. }) => (false, false, false),
        Some(X86OpHint::VexOp { width, .. }) => (true, width == VecWidth::V256, false),
        Some(X86OpHint::EvexOp { width, .. }) => (false, false, width != VecWidth::V512),
        _ => (false, false, false),
    }
}
/// Whether any executable (non-exit) block contains an admitted native vector
/// operation. This controls vector-state marshalling in the entry trampoline.
pub fn uses_x86_native_vectors_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    if x86_native_replay_feature_requirements(func, excluded).any {
        return true;
    }
    func.blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .any(|block| {
            block
                .ops
                .iter()
                .any(|op| x86_native_vector_smir_op(op) || x86_jit_vector_mem_shape_valid(&op.kind))
        })
}

/// Return `(AES-NI, VAES, AVX-512VL)` requirements contributed by an admitted
/// `X86Aes` operation. Low-register 128-bit rounds are re-encoded with VEX and
/// require AES+AVX; 256-bit or EVEX rounds require VAES. High 128/256-bit
/// registers require EVEX.VL, while 512-bit rounds use EVEX without VL.
pub(crate) fn x86_aes_feature_requirements(
    op: &crate::smir::ir::ops::OpKind,
) -> (bool, bool, bool) {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86AesOp, X86Reg};

    let OpKind::X86Aes {
        dst,
        src1,
        src2,
        width,
        op,
        ..
    } = op
    else {
        return (false, false, false);
    };
    match op {
        X86AesOp::InvMixColumns | X86AesOp::KeygenAssist => (true, false, false),
        X86AesOp::Enc | X86AesOp::EncLast | X86AesOp::Dec | X86AesOp::DecLast => {
            let high_vector = |reg: &VReg| {
                matches!(
                    reg,
                    VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(16..=31) | X86Reg::Ymm(16..=31) | X86Reg::Zmm(16..=31)
                    ))
                )
            };
            let high_register =
                high_vector(dst) || high_vector(src1) || src2.is_some_and(|reg| high_vector(&reg));
            if *width == VecWidth::V128 && !high_register {
                (true, false, false)
            } else {
                (false, true, *width != VecWidth::V512 && high_register)
            }
        }
    }
}
pub(crate) fn x86_sha512_feature_required(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. }
    )
}
pub(crate) fn x86_sm3_feature_required(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;

    matches!(
        op,
        OpKind::X86Sm3Msg1 { .. } | OpKind::X86Sm3Msg2 { .. } | OpKind::X86Sm3Rounds2 { .. }
    )
}
pub(crate) fn x86_sm4_feature_required(op: &crate::smir::ir::ops::OpKind) -> bool {
    matches!(op, crate::smir::ir::ops::OpKind::X86Sm4 { .. })
}

fn x86_has_direct_native_vector_op_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    for block in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
    {
        let replay = crate::smir::ir::x86_native_replay_spans(block, &func.x86_instruction_bytes);
        let mut virtual_definitions = std::collections::HashMap::new();
        let mut virtual_uses = std::collections::HashMap::new();
        for op in &block.ops {
            for reg in op.kind.dests() {
                if matches!(reg, crate::smir::ir::types::VReg::Virtual(_)) {
                    *virtual_definitions.entry(reg).or_insert(0usize) += 1;
                }
            }
            for reg in op.kind.source_vregs() {
                if matches!(reg, crate::smir::ir::types::VReg::Virtual(_)) {
                    *virtual_uses.entry(reg).or_insert(0usize) += 1;
                }
            }
        }
        let mut index = 0usize;
        while index < block.ops.len() {
            if let Some(span) = replay.get(&index) {
                index = span.end;
                continue;
            }
            if let Some(sequence) = super::x86_jit_vex_masked_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                index += sequence.consumed;
                continue;
            }
            let op = &block.ops[index];
            if x86_native_vector_smir_op(op) || x86_jit_vector_mem_shape_valid(&op.kind) {
                return true;
            }
            index += 1;
        }
    }
    false
}

/// Whether every executable vector operation is covered by a replay span that
/// needs only YMM0-YMM15 plus MXCSR at the native boundary. Upper ZMM halves
/// and opmask state then remain state-backed.
pub(crate) fn x86_native_vector_uses_avx_ymm16_only_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    let replay = x86_native_replay_feature_requirements(func, excluded);
    replay.all_spans_support_avx_ymm16 && !x86_has_direct_native_vector_op_excluding(func, excluded)
}

/// Verify that this host can execute every admitted vector opcode in `func`.
/// General vector regions require AVX512F for 512-bit VMOVDQU64/KMOVW and
/// AVX512BW for full-width KMOVQ; spans proven to observe at most K[15:0] use
/// the fail-closed low-16 opmask state mode instead. Replay-only AVX-YMM16-safe
/// regions use a separate AVX bridge and therefore require no AVX-512 feature.
pub fn x86_native_vector_features_supported_excluding(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::VecWidth;

    let replay = x86_native_replay_feature_requirements(func, excluded);
    if replay.all_spans_support_avx_ymm16
        && !x86_has_direct_native_vector_op_excluding(func, excluded)
    {
        #[cfg(target_arch = "x86_64")]
        {
            return replay.x86_host_supported();
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            return false;
        }
    }
    let mut any = replay.any;
    let mut needs_bw = replay.needs_avx512bw;
    let mut needs_vl = replay.needs_avx512vl;
    let mut needs_vbmi = replay.needs_avx512vbmi;
    let mut needs_vbmi2 = false;
    let mut needs_bitalg = replay.needs_avx512bitalg;
    let mut needs_vpopcntdq = replay.needs_avx512vpopcntdq;
    let mut needs_vnni = false;
    let mut needs_ifma = false;
    let mut needs_bf16 = replay.needs_avx512bf16;
    let mut needs_cd = replay.needs_avx512cd;
    let mut needs_fp16 = replay.needs_avx512fp16;
    let mut needs_er = replay.needs_avx512er;
    let mut needs_aes = replay.needs_aes;
    let mut needs_vaes = replay.needs_vaes;
    let mut needs_sha512 = false;
    let mut needs_sm3 = false;
    let mut needs_sm4 = false;
    let mut needs_shift_avx = false;
    let mut needs_shift_avx2 = false;
    let mut needs_logic_avx = false;
    let mut needs_logic_avx2 = false;
    let mut needs_dq = replay.needs_avx512dq;
    let mut needs_int_arith_avx = false;
    let mut needs_int_arith_avx2 = false;
    let mut needs_mul_sse41 = false;
    let mut needs_mul_avx = false;
    let mut needs_mul_avx2 = false;
    let mut needs_mul_dq = false;
    let mut needs_abs_ssse3 = false;
    let mut needs_abs_avx = false;
    let mut needs_abs_avx2 = false;
    let mut needs_cmp_sse41 = false;
    let mut needs_cmp_sse42 = false;
    let mut needs_cmp_avx = false;
    let mut needs_cmp_avx2 = false;
    let mut needs_interleave_avx = false;
    let mut needs_interleave_avx2 = false;
    let mut needs_pack_sse41 = false;
    let mut needs_pack_avx = false;
    let mut needs_pack_avx2 = false;
    let mut needs_byte_shuffle_ssse3 = false;
    let mut needs_byte_shuffle_avx = false;
    let mut needs_byte_shuffle_avx2 = false;
    let mut needs_horizontal_ssse3 = false;
    let mut needs_horizontal_avx = false;
    let mut needs_horizontal_avx2 = false;
    let mut needs_mul_shift_ssse3 = false;
    let mut needs_mul_shift_avx = false;
    let mut needs_mul_shift_avx2 = false;
    let mut needs_average_avx = false;
    let mut needs_average_avx2 = false;
    let mut needs_sign_ssse3 = false;
    let mut needs_sign_avx = false;
    let mut needs_sign_avx2 = false;
    let mut needs_minmax_sse41 = false;
    let mut needs_minmax_avx = false;
    let mut needs_minmax_avx2 = false;
    let mut needs_sad_bytes_avx = false;
    let mut needs_sad_bytes_avx2 = false;
    let mut needs_phminposuw_sse41 = false;
    let mut needs_phminposuw_avx = false;
    let mut needs_mov_mask_avx = false;
    let mut needs_mov_mask_avx2 = false;
    let mut needs_mpsadbw_sse41 = false;
    let mut needs_mpsadbw_avx = false;
    let mut needs_mpsadbw_avx2 = false;
    let mut needs_maddubs_ssse3 = false;
    let mut needs_maddubs_avx = false;
    let mut needs_maddubs_avx2 = false;
    let mut needs_maddwd_avx = false;
    let mut needs_maddwd_avx2 = false;

    for op in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .flat_map(|block| &block.ops)
        .filter(|op| x86_native_vector_smir_op(op) || x86_jit_vector_mem_shape_valid(&op.kind))
    {
        any = true;
        let kind = &op.kind;
        needs_bw |= !matches!(
            kind,
            OpKind::X86Exp2 { .. }
                | OpKind::X86Recip14 { .. }
                | OpKind::X86Rsqrt14 { .. }
                | OpKind::X86Recip28 { .. }
                | OpKind::X86Rsqrt28 { .. }
        );
        if let OpKind::X86Opmask(opmask) = kind {
            needs_dq |= crate::smir::lower::x86_64::x86_opmask_needs_avx512dq(opmask);
            continue;
        }
        let width = match kind {
            OpKind::VMov { width, .. }
            | OpKind::VAnd { width, .. }
            | OpKind::VAndNot { width, .. }
            | OpKind::VOr { width, .. }
            | OpKind::VXor { width, .. }
            | OpKind::VPopcnt { width, .. }
            | OpKind::VShuffleBitQM { width, .. }
            | OpKind::VConflict { width, .. }
            | OpKind::VLeadingZeros { width, .. }
            | OpKind::X86PermuteBytesWords { width, .. }
            | OpKind::VCompress { width, .. }
            | OpKind::VExpand { width, .. }
            | OpKind::X86NarrowInt { width, .. }
            | OpKind::X86Aes { width, .. }
            | OpKind::X86PackedShiftImm { width, .. }
            | OpKind::X86PackedShift { width, .. }
            | OpKind::VSadBytes { width, .. }
            | OpKind::VMpsadbw { width, .. }
            | OpKind::VDotProduct { width, .. }
            | OpKind::VDotProductBF16 { width, .. }
            | OpKind::VCvtFP32ToBF16 { width, .. }
            | OpKind::VFP16Arith { width, .. }
            | OpKind::X86GetExponent { width, .. }
            | OpKind::X86GetMantissa { width, .. }
            | OpKind::X86RoundScale { width, .. }
            | OpKind::X86Reduce { width, .. }
            | OpKind::X86Range { width, .. }
            | OpKind::X86FixupImm { width, .. }
            | OpKind::X86Exp2 { width, .. }
            | OpKind::X86Recip14 { width, .. }
            | OpKind::X86Rsqrt14 { width, .. }
            | OpKind::X86RecipFp16 { width, .. }
            | OpKind::X86RsqrtFp16 { width, .. }
            | OpKind::X86Recip28 { width, .. }
            | OpKind::X86Rsqrt28 { width, .. }
            | OpKind::X86ScaleF { width, .. }
            | OpKind::X86FP16Complex { width, .. }
            | OpKind::X86PackedIntToFp {
                src_width: width, ..
            }
            | OpKind::X86PackedFpToInt {
                dst_width: width, ..
            }
            | OpKind::X86PackedIntToFp16 {
                src_width: width, ..
            }
            | OpKind::X86PackedFp16ToInt {
                dst_width: width, ..
            }
            | OpKind::VMultiplyAdd52 { width, .. }
            | OpKind::X86PackedShiftVariable { width, .. }
            | OpKind::X86PackedRotate { width, .. }
            | OpKind::X86TernaryLogic { width, .. }
            | OpKind::X86PackedFunnelShift { width, .. }
            | OpKind::X86MultiShiftQB { width, .. }
            | OpKind::VLoad { width, .. }
            | OpKind::VStore { width, .. } => *width,
            OpKind::X86Phminposuw { .. } => VecWidth::V128,
            OpKind::X86MovdQ { .. } => VecWidth::V128,
            OpKind::X86MovMask { elem, lanes, .. } => x86_vector_width_from_lanes(*elem, *lanes)
                .expect("admitted MOVMSK operation has exact lanes"),
            OpKind::VAdd { elem, lanes, .. }
            | OpKind::VSub { elem, lanes, .. }
            | OpKind::VAddSubSat { elem, lanes, .. }
            | OpKind::VMul { elem, lanes, .. }
            | OpKind::VUnary { elem, lanes, .. }
            | OpKind::VCmp { elem, lanes, .. }
            | OpKind::VInterleave { elem, lanes, .. }
            | OpKind::VHorizontalBin { elem, lanes, .. }
            | OpKind::VMulShiftSat {
                src_elem: elem,
                lanes,
                ..
            }
            | OpKind::VLane { elem, lanes, .. } => x86_vector_width_from_lanes(*elem, *lanes)
                .expect("admitted integer vector operation has exact lanes"),
            OpKind::VPackSat {
                src_elem,
                src_lanes,
                ..
            } => x86_vector_width_from_lanes(*src_elem, *src_lanes)
                .expect("admitted integer pack has exact lanes"),
            OpKind::VByteShuffle { lanes, .. } => {
                x86_vector_width_from_lanes(crate::smir::ir::types::VecElementType::I8, *lanes)
                    .expect("admitted byte shuffle has exact lanes")
            }
            OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. } => VecWidth::V256,
            OpKind::X86Sm3Msg1 { .. }
            | OpKind::X86Sm3Msg2 { .. }
            | OpKind::X86Sm3Rounds2 { .. } => VecWidth::V128,
            OpKind::X86Sm4 { width, .. } => *width,
            _ => unreachable!("filtered to native vector operations"),
        };
        let (aes, vaes, aes_vl) = x86_aes_feature_requirements(kind);
        let (shift_avx, shift_avx2, shift_vl) = x86_packed_shift_imm_feature_requirements(kind);
        let (count_avx, count_avx2, count_vl) = x86_packed_shift_feature_requirements(kind);
        let (logic_avx, logic_avx2, logic_dq, logic_vl) = x86_vector_logic_feature_requirements(op);
        let (int_arith_avx, int_arith_avx2, int_arith_vl) =
            x86_vector_integer_arithmetic_feature_requirements(op);
        let (mul_sse41, mul_avx, mul_avx2, mul_dq, mul_vl) =
            x86_vector_integer_multiply_feature_requirements(op);
        let (abs_ssse3, abs_avx, abs_avx2, abs_vl) =
            x86_vector_integer_abs_feature_requirements(op);
        let (cmp_sse41, cmp_sse42, cmp_avx, cmp_avx2) =
            x86_vector_integer_compare_feature_requirements(op);
        let (interleave_avx, interleave_avx2, interleave_vl) =
            x86_vector_integer_interleave_feature_requirements(op);
        let (pack_sse41, pack_avx, pack_avx2, pack_vl) =
            x86_vector_integer_pack_feature_requirements(op);
        let (byte_shuffle_ssse3, byte_shuffle_avx, byte_shuffle_avx2, byte_shuffle_vl) =
            x86_vector_byte_shuffle_feature_requirements(op);
        let (horizontal_ssse3, horizontal_avx, horizontal_avx2) =
            x86_vector_integer_horizontal_feature_requirements(op);
        let (mul_shift_ssse3, mul_shift_avx, mul_shift_avx2, mul_shift_vl) =
            x86_vector_integer_mul_shift_feature_requirements(op);
        let (average_avx, average_avx2, average_vl) =
            x86_vector_integer_average_feature_requirements(op);
        let (sign_ssse3, sign_avx, sign_avx2) = x86_vector_integer_sign_feature_requirements(op);
        let (minmax_sse41, minmax_avx, minmax_avx2, minmax_vl) =
            x86_vector_integer_minmax_feature_requirements(op);
        let (sad_bytes_avx, sad_bytes_avx2, sad_bytes_vl) =
            x86_vector_sad_bytes_feature_requirements(op);
        let (phminposuw_sse41, phminposuw_avx) = x86_phminposuw_feature_requirements(op);
        let (mov_mask_avx, mov_mask_avx2) = x86_mov_mask_feature_requirements(op);
        let (mpsadbw_sse41, mpsadbw_avx, mpsadbw_avx2) =
            x86_vector_mpsadbw_feature_requirements(op);
        let (maddubs_ssse3, maddubs_avx, maddubs_avx2, maddubs_vl) =
            x86_vector_integer_maddubs_feature_requirements(op);
        let (maddwd_avx, maddwd_avx2, maddwd_vl) =
            x86_vector_integer_maddwd_feature_requirements(op);
        needs_vl |= match kind {
            OpKind::VMov { .. } => x86_vector_move_needs_vl(op),
            OpKind::VAnd { .. }
            | OpKind::VAndNot { .. }
            | OpKind::VOr { .. }
            | OpKind::VXor { .. } => logic_vl,
            OpKind::VAdd { .. } | OpKind::VSub { .. } | OpKind::VAddSubSat { .. } => int_arith_vl,
            OpKind::VMul { .. } => mul_vl,
            OpKind::VUnary { .. } => abs_vl,
            OpKind::VCmp { .. } => false,
            OpKind::VInterleave { .. } => interleave_vl,
            OpKind::VPackSat { .. } => pack_vl,
            OpKind::VByteShuffle { .. } => byte_shuffle_vl,
            OpKind::VHorizontalBin { .. } => false,
            OpKind::VMulShiftSat { .. } => mul_shift_vl,
            OpKind::VLane {
                op: crate::smir::ir::types::VLaneOp::AvgRnd,
                ..
            } => average_vl,
            OpKind::VLane {
                op: crate::smir::ir::types::VLaneOp::Sign,
                ..
            } => false,
            OpKind::VLane {
                op: crate::smir::ir::types::VLaneOp::Min | crate::smir::ir::types::VLaneOp::Max,
                ..
            } => minmax_vl,
            OpKind::VSadBytes { .. } => sad_bytes_vl,
            OpKind::X86Phminposuw { .. } => false,
            OpKind::X86MovdQ { .. } => false,
            OpKind::X86MovMask { .. } => false,
            OpKind::VMpsadbw { .. } => false,
            OpKind::VDotProduct { .. } if x86_vector_integer_maddubs_shape_valid(kind) => {
                maddubs_vl
            }
            OpKind::VDotProduct { .. } if x86_vector_integer_maddwd_shape_valid(kind) => maddwd_vl,
            OpKind::X86PackedIntToFp { .. } | OpKind::X86PackedFpToInt { .. } => {
                matches!(
                    op.x86_hint,
                    Some(crate::smir::ir::ops::X86OpHint::EvexOp { width, .. })
                        if width != VecWidth::V512
                )
            }
            OpKind::X86GetExponent { scalar, .. }
            | OpKind::X86GetMantissa { scalar, .. }
            | OpKind::X86RoundScale { scalar, .. }
            | OpKind::X86Reduce { scalar, .. }
            | OpKind::X86Range { scalar, .. }
            | OpKind::X86FixupImm { scalar, .. }
            | OpKind::X86ScaleF { scalar, .. }
            | OpKind::X86FP16Complex { scalar, .. } => !*scalar && width != VecWidth::V512,
            OpKind::X86Recip14 { scalar, .. } | OpKind::X86Rsqrt14 { scalar, .. } => {
                !*scalar && width != VecWidth::V512
            }
            OpKind::X86RecipFp16 { scalar, .. } | OpKind::X86RsqrtFp16 { scalar, .. } => {
                !*scalar && width != VecWidth::V512
            }
            OpKind::X86Exp2 { .. } | OpKind::X86Recip28 { .. } | OpKind::X86Rsqrt28 { .. } => false,
            OpKind::X86Aes { .. } => aes_vl,
            OpKind::X86PackedShiftImm { .. } => shift_vl,
            OpKind::X86PackedShift { .. } => count_vl,
            OpKind::X86Sha512Msg1 { .. }
            | OpKind::X86Sha512Msg2 { .. }
            | OpKind::X86Sha512Rounds2 { .. }
            | OpKind::X86Sm3Msg1 { .. }
            | OpKind::X86Sm3Msg2 { .. }
            | OpKind::X86Sm3Rounds2 { .. }
            | OpKind::X86Sm4 { .. }
            | OpKind::VLoad { .. }
            | OpKind::VStore { .. } => false,
            _ => width != VecWidth::V512,
        };
        needs_vbmi |= matches!(
            kind,
            OpKind::X86MultiShiftQB { .. } | OpKind::X86PermuteBytesWords { .. }
        );
        needs_vbmi2 |= matches!(kind, OpKind::X86PackedFunnelShift { .. })
            || matches!(
                kind,
                OpKind::VCompress {
                    elem: crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16,
                    ..
                } | OpKind::VExpand {
                    elem: crate::smir::ir::types::VecElementType::I8
                        | crate::smir::ir::types::VecElementType::I16,
                    ..
                }
            );
        if let OpKind::VPopcnt { elem, .. } = kind {
            needs_bitalg |= matches!(
                elem,
                crate::smir::ir::types::VecElementType::I8
                    | crate::smir::ir::types::VecElementType::I16
            );
            needs_vpopcntdq |= matches!(
                elem,
                crate::smir::ir::types::VecElementType::I32
                    | crate::smir::ir::types::VecElementType::I64
            );
        }
        needs_bitalg |= matches!(kind, OpKind::VShuffleBitQM { .. });
        needs_vnni |= matches!(kind, OpKind::VDotProduct { .. })
            && !x86_vector_integer_maddubs_shape_valid(kind)
            && !x86_vector_integer_maddwd_shape_valid(kind);
        needs_ifma |= matches!(kind, OpKind::VMultiplyAdd52 { .. });
        needs_bf16 |= matches!(
            kind,
            OpKind::VDotProductBF16 { .. } | OpKind::VCvtFP32ToBF16 { .. }
        );
        needs_cd |= matches!(
            kind,
            OpKind::VConflict { .. } | OpKind::VLeadingZeros { .. }
        );
        needs_fp16 |= matches!(
            kind,
            OpKind::VFP16Arith { .. }
                | OpKind::X86RecipFp16 { .. }
                | OpKind::X86RsqrtFp16 { .. }
                | OpKind::X86FP16Complex { .. }
                | OpKind::X86GetExponent {
                    elem: crate::smir::ir::types::VecElementType::F16,
                    ..
                }
                | OpKind::X86GetMantissa {
                    elem: crate::smir::ir::types::VecElementType::F16,
                    ..
                }
                | OpKind::X86RoundScale {
                    elem: crate::smir::ir::types::VecElementType::F16,
                    ..
                }
                | OpKind::X86Reduce {
                    elem: crate::smir::ir::types::VecElementType::F16,
                    ..
                }
                | OpKind::X86ScaleF {
                    elem: crate::smir::ir::types::VecElementType::F16,
                    ..
                }
                | OpKind::X86PackedIntToFp16 { .. }
                | OpKind::X86PackedFp16ToInt { .. }
        );
        needs_er |= matches!(
            kind,
            OpKind::X86Exp2 { .. } | OpKind::X86Recip28 { .. } | OpKind::X86Rsqrt28 { .. }
        );
        needs_aes |= aes;
        needs_vaes |= vaes;
        needs_sha512 |= x86_sha512_feature_required(kind);
        needs_sm3 |= x86_sm3_feature_required(kind);
        needs_sm4 |= x86_sm4_feature_required(kind);
        needs_shift_avx |= shift_avx || count_avx;
        needs_shift_avx2 |= shift_avx2 || count_avx2;
        needs_logic_avx |= logic_avx;
        needs_logic_avx2 |= logic_avx2;
        needs_dq |= logic_dq
            || matches!(
                kind,
                OpKind::X86PackedIntToFp {
                    int_elem: crate::smir::ir::types::VecElementType::I64,
                    ..
                } | OpKind::X86PackedFpToInt {
                    int_elem: crate::smir::ir::types::VecElementType::I64,
                    ..
                } | OpKind::X86Reduce {
                    elem: crate::smir::ir::types::VecElementType::F32
                        | crate::smir::ir::types::VecElementType::F64,
                    ..
                } | OpKind::X86Range { .. }
            );
        needs_int_arith_avx |= int_arith_avx;
        needs_int_arith_avx2 |= int_arith_avx2;
        needs_mul_sse41 |= mul_sse41;
        needs_mul_avx |= mul_avx;
        needs_mul_avx2 |= mul_avx2;
        needs_mul_dq |= mul_dq;
        needs_abs_ssse3 |= abs_ssse3;
        needs_abs_avx |= abs_avx;
        needs_abs_avx2 |= abs_avx2;
        needs_cmp_sse41 |= cmp_sse41;
        needs_cmp_sse42 |= cmp_sse42;
        needs_cmp_avx |= cmp_avx;
        needs_cmp_avx2 |= cmp_avx2;
        needs_interleave_avx |= interleave_avx;
        needs_interleave_avx2 |= interleave_avx2;
        needs_pack_sse41 |= pack_sse41;
        needs_pack_avx |= pack_avx;
        needs_pack_avx2 |= pack_avx2;
        needs_byte_shuffle_ssse3 |= byte_shuffle_ssse3;
        needs_byte_shuffle_avx |= byte_shuffle_avx;
        needs_byte_shuffle_avx2 |= byte_shuffle_avx2;
        needs_horizontal_ssse3 |= horizontal_ssse3;
        needs_horizontal_avx |= horizontal_avx;
        needs_horizontal_avx2 |= horizontal_avx2;
        needs_mul_shift_ssse3 |= mul_shift_ssse3;
        needs_mul_shift_avx |= mul_shift_avx;
        needs_mul_shift_avx2 |= mul_shift_avx2;
        needs_average_avx |= average_avx;
        needs_average_avx2 |= average_avx2;
        needs_sign_ssse3 |= sign_ssse3;
        needs_sign_avx |= sign_avx;
        needs_sign_avx2 |= sign_avx2;
        needs_minmax_sse41 |= minmax_sse41;
        needs_minmax_avx |= minmax_avx;
        needs_minmax_avx2 |= minmax_avx2;
        needs_sad_bytes_avx |= sad_bytes_avx;
        needs_sad_bytes_avx2 |= sad_bytes_avx2;
        needs_phminposuw_sse41 |= phminposuw_sse41;
        needs_phminposuw_avx |= phminposuw_avx;
        needs_mov_mask_avx |= mov_mask_avx;
        needs_mov_mask_avx2 |= mov_mask_avx2;
        needs_mpsadbw_sse41 |= mpsadbw_sse41;
        needs_mpsadbw_avx |= mpsadbw_avx;
        needs_mpsadbw_avx2 |= mpsadbw_avx2;
        needs_maddubs_ssse3 |= maddubs_ssse3;
        needs_maddubs_avx |= maddubs_avx;
        needs_maddubs_avx2 |= maddubs_avx2;
        needs_maddwd_avx |= maddwd_avx;
        needs_maddwd_avx2 |= maddwd_avx2;
    }

    if !any {
        return true;
    }

    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx512f")
            && (!needs_bw || std::is_x86_feature_detected!("avx512bw"))
            && (!needs_vl || std::is_x86_feature_detected!("avx512vl"))
            && (!needs_vbmi || std::is_x86_feature_detected!("avx512vbmi"))
            && (!needs_vbmi2 || std::is_x86_feature_detected!("avx512vbmi2"))
            && (!needs_bitalg || std::is_x86_feature_detected!("avx512bitalg"))
            && (!needs_vpopcntdq || std::is_x86_feature_detected!("avx512vpopcntdq"))
            && (!needs_vnni || std::is_x86_feature_detected!("avx512vnni"))
            && (!needs_ifma || std::is_x86_feature_detected!("avx512ifma"))
            && (!needs_bf16 || std::is_x86_feature_detected!("avx512bf16"))
            && (!needs_cd || std::is_x86_feature_detected!("avx512cd"))
            && (!needs_fp16 || std::is_x86_feature_detected!("avx512fp16"))
            && replay.x86_host_supported()
            && (!needs_er || x86_host_has_avx512er())
            && (!(needs_dq || needs_mul_dq) || std::is_x86_feature_detected!("avx512dq"))
            && (!needs_aes || std::is_x86_feature_detected!("aes"))
            && (!needs_vaes || std::is_x86_feature_detected!("vaes"))
            && (!needs_sha512
                || (std::is_x86_feature_detected!("avx2")
                    && std::is_x86_feature_detected!("sha512")))
            && (!needs_sm3
                || (std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("sm3")))
            && (!needs_sm4
                || (std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("sm4")))
            && (!needs_shift_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_shift_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_logic_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_logic_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_int_arith_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_int_arith_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_mul_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_mul_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mul_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_abs_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_abs_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_abs_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_cmp_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_cmp_sse42 || std::is_x86_feature_detected!("sse4.2"))
            && (!needs_cmp_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_cmp_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_interleave_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_interleave_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_pack_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_pack_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_pack_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_byte_shuffle_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_byte_shuffle_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_byte_shuffle_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_horizontal_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_horizontal_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_horizontal_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_mul_shift_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_mul_shift_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mul_shift_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_average_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_average_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_sign_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_sign_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_sign_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_minmax_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_minmax_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_minmax_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_sad_bytes_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_sad_bytes_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_phminposuw_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_phminposuw_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mov_mask_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mov_mask_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_mpsadbw_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!needs_mpsadbw_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_mpsadbw_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_maddubs_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!needs_maddubs_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_maddubs_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!needs_maddwd_avx || std::is_x86_feature_detected!("avx"))
            && (!needs_maddwd_avx2 || std::is_x86_feature_detected!("avx2"))
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (
            needs_bw,
            needs_vl,
            needs_vbmi,
            needs_vbmi2,
            needs_bitalg,
            needs_vpopcntdq,
            needs_vnni,
            needs_ifma,
            needs_bf16,
            needs_cd,
            needs_fp16,
            needs_er,
            needs_aes,
            needs_vaes,
            needs_sha512,
            needs_sm3,
            needs_sm4,
            needs_shift_avx,
            needs_shift_avx2,
            needs_logic_avx,
            needs_logic_avx2,
            needs_dq,
            needs_int_arith_avx,
            needs_int_arith_avx2,
            needs_mul_sse41,
            needs_mul_avx,
            needs_mul_avx2,
            needs_mul_dq,
            needs_abs_ssse3,
            needs_abs_avx,
            needs_abs_avx2,
            needs_cmp_sse41,
            needs_cmp_sse42,
            needs_cmp_avx,
            needs_cmp_avx2,
            needs_interleave_avx,
            needs_interleave_avx2,
            needs_pack_sse41,
            needs_pack_avx,
            needs_pack_avx2,
            needs_byte_shuffle_ssse3,
            needs_byte_shuffle_avx,
            needs_byte_shuffle_avx2,
            needs_horizontal_ssse3,
            needs_horizontal_avx,
            needs_horizontal_avx2,
            needs_average_avx,
            needs_average_avx2,
            needs_sign_ssse3,
            needs_sign_avx,
            needs_sign_avx2,
            needs_minmax_sse41,
            needs_minmax_avx,
            needs_minmax_avx2,
            needs_sad_bytes_avx,
            needs_sad_bytes_avx2,
            needs_phminposuw_sse41,
            needs_phminposuw_avx,
            needs_mov_mask_avx,
            needs_mov_mask_avx2,
            needs_mpsadbw_sse41,
            needs_mpsadbw_avx,
            needs_mpsadbw_avx2,
            needs_maddubs_ssse3,
            needs_maddubs_avx,
            needs_maddubs_avx2,
            needs_maddwd_avx,
            needs_maddwd_avx2,
        );
        false
    }
}
/// Admit only architectural vector moves whose register class exactly matches
/// the transfer width. Virtual vector temporaries remain ineligible because the
/// identity bridge has no stable state slot for them across an MMU helper call.
pub(crate) fn x86_jit_vector_mem_shape_valid(op: &crate::smir::ir::ops::OpKind) -> bool {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::{ArchReg, VReg, VecWidth, X86Reg};

    let vector_matches_width = |reg: &VReg, width: VecWidth| {
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

    match op {
        OpKind::VLoad { dst, addr, width } => {
            vector_matches_width(dst, *width) && x86_jit_mem_address_shape_valid(addr)
        }
        OpKind::VStore { src, addr, width } => {
            vector_matches_width(src, *width) && x86_jit_mem_address_shape_valid(addr)
        }
        _ => false,
    }
}
