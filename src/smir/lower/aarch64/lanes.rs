//! SIMD broadcasts, insertion, extraction, and comparisons.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{SignExtend, VLaneOp, VReg, VecCmpCond, VecElementType, VecWidth};
use crate::smir::lower::LowerError;

use super::Aarch64Lowerer;

/// Validate native-only lane families against the AArch64 vector-state ABI.
pub(crate) fn aarch64_neon_lane_shape_valid(op: &SmirOp) -> bool {
    let OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: lane_op,
        set_ovf: false,
        ..
    } = &op.kind
    else {
        return false;
    };
    let shape_valid = match lane_op {
        VLaneOp::ShiftSigned8 | VLaneOp::ShiftSigned8Rnd => {
            Aarch64Lowerer::simd_scalar_or_vector_integer_shape(*elem, *lanes).is_ok()
        }
        VLaneOp::AndNot | VLaneOp::OrNot => Aarch64Lowerer::simd_lane_width(*elem, *lanes).is_ok(),
        _ => false,
    };
    shape_valid
        && op.x86_hint.is_none()
        && [*dst, *src1, *src2]
            .into_iter()
            .all(|reg| Aarch64Lowerer::fp_reg(reg).is_ok())
}

/// Validate comparison operands and provenance for the AArch64 vector-state ABI.
pub(crate) fn aarch64_vcmp_shape_valid(op: &SmirOp) -> bool {
    let OpKind::VCmp {
        dst,
        src1,
        src2,
        cond,
        elem,
        lanes,
    } = &op.kind
    else {
        return false;
    };
    if op.x86_hint.is_some() {
        return false;
    }

    let result_and_source_are_vectors = [*dst, *src1]
        .into_iter()
        .all(|reg| Aarch64Lowerer::fp_reg(reg).is_ok());
    let comparison_valid = match src2 {
        VReg::Imm(0) => Aarch64Lowerer::simd_integer_compare_zero_opcode(*cond).is_some(),
        _ => {
            Aarch64Lowerer::fp_reg(*src2).is_ok()
                && Aarch64Lowerer::simd_integer_compare_opcode(*cond).is_some()
        }
    };
    result_and_source_are_vectors
        && comparison_valid
        && Aarch64Lowerer::simd_scalar_or_vector_integer_shape(*elem, *lanes).is_ok()
}

impl Aarch64Lowerer {
    fn simd_integer_compare_zero_opcode(cond: VecCmpCond) -> Option<(u32, u32)> {
        match cond {
            VecCmpCond::Gt => Some((0, 0b01000)),
            VecCmpCond::Ge => Some((1, 0b01000)),
            VecCmpCond::Eq => Some((0, 0b01001)),
            VecCmpCond::Le => Some((1, 0b01001)),
            VecCmpCond::Lt => Some((0, 0b01010)),
            _ => None,
        }
    }

    fn simd_integer_compare_opcode(cond: VecCmpCond) -> Option<(u32, u32, bool)> {
        match cond {
            VecCmpCond::Eq => Some((1, 0b10001, false)),
            VecCmpCond::Gt => Some((0, 0b00110, false)),
            VecCmpCond::Ge => Some((0, 0b00111, false)),
            VecCmpCond::Gtu => Some((1, 0b00110, false)),
            VecCmpCond::Geu => Some((1, 0b00111, false)),
            VecCmpCond::Lt => Some((0, 0b00110, true)),
            VecCmpCond::Le => Some((0, 0b00111, true)),
            VecCmpCond::Ltu => Some((1, 0b00110, true)),
            VecCmpCond::Leu => Some((1, 0b00111, true)),
            _ => None,
        }
    }

    pub(crate) fn lower_vcmp(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        cond: VecCmpCond,
        elem: VecElementType,
        lanes: u8,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let mut rn = Self::fp_reg(src1)?;
        let shape = Self::simd_scalar_or_vector_integer_shape(elem, lanes)?;
        if src2 == VReg::Imm(0) {
            let (u, opcode) = Self::simd_integer_compare_zero_opcode(cond).ok_or_else(|| {
                LowerError::UnsupportedOp {
                    op: format!("AArch64 integer comparison against zero {cond:?}"),
                }
            })?;
            self.emit_integer_simd_two_misc(rd, rn, &shape, u, opcode);
            return Ok(());
        }
        let mut rm = Self::fp_reg(src2)?;
        let (u, opcode, swap_sources) =
            Self::simd_integer_compare_opcode(cond).ok_or_else(|| LowerError::UnsupportedOp {
                op: format!("AArch64 native vector compare {cond:?}"),
            })?;
        if swap_sources {
            std::mem::swap(&mut rn, &mut rm);
        }
        self.emit_integer_simd_three_same(rd, rn, rm, &shape, u, opcode);
        Ok(())
    }

    pub(crate) fn lower_vbroadcast(
        &mut self,
        dst: VReg,
        scalar: VReg,
        elem: VecElementType,
        lanes: u8,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::gpr_arm_or_x86(scalar)?;
        let (q, size) = Self::simd_broadcast_shape(elem, lanes)?;
        self.emit_simd_dup_general(rd, rn, q, size);
        Ok(())
    }

    pub(crate) fn lower_vinsert_lane(
        &mut self,
        dst: VReg,
        vec: VReg,
        scalar: VReg,
        lane: u8,
        elem: VecElementType,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(vec)?;
        let rm = Self::gpr_arm_or_x86(scalar)?;
        let (_, imm5) = Self::simd_lane_imm5(elem, lane)?;
        if rd != rn {
            self.lower_vmov(dst, vec, VecWidth::V128)?;
        }
        self.emit_simd_ins_general(rd, rm, imm5);
        Ok(())
    }

    pub(crate) fn lower_vextract_lane(
        &mut self,
        dst: VReg,
        vec: VReg,
        lane: u8,
        elem: VecElementType,
        sign: SignExtend,
    ) -> Result<(), LowerError> {
        let rd = Self::dst_gpr_arm_or_x86(dst)?;
        let rn = Self::fp_reg(vec)?;
        let (size, imm5) = Self::simd_lane_imm5(elem, lane)?;
        match (sign, size) {
            (SignExtend::Zero, 3) | (SignExtend::Sign, 3) => {
                self.emit_simd_umov(rd, rn, imm5, true);
            }
            (SignExtend::Zero, _) => {
                self.emit_simd_umov(rd, rn, imm5, false);
            }
            (SignExtend::Sign, _) => {
                self.emit_simd_smov(rd, rn, imm5);
            }
        }
        Ok(())
    }
}
