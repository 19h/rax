//! Condition-code, NZCV, and conditional-select lowering

use crate::smir::lower::aarch64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    ArmDpRegShiftKind, OpKind, SmirOp, X86AdxKind, X86BlsKind, X86CountKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, ArmReg, AtomicOp, Avx10FP16Op, BlockId, Condition, ExtendOp, FenceKind,
    FpPrecision, FpRoundMode, MemWidth, MemoryOrder, OpWidth, ShiftOp, SignExtend, SrcOperand,
    VLaneOp, VReg, VecElementType, VecPermuteKind, VecReduceOp, VecUnaryOp, VecWidth,
};
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};

use super::{CodeBuffer, LowerError, LowerResult, Relocation, SmirLowerer};

impl Aarch64Lowerer {

    #[cfg(target_arch = "aarch64")]
    pub(crate) fn detect_flagm_available() -> bool {
        std::arch::is_aarch64_feature_detected!("flagm")
    }


    #[cfg(not(target_arch = "aarch64"))]
    pub(crate) fn detect_flagm_available() -> bool {
        true
    }


    #[cfg(target_arch = "aarch64")]
    pub(crate) fn detect_flagm2_available() -> bool {
        cfg!(target_feature = "flagm2")
    }


    #[cfg(not(target_arch = "aarch64"))]
    pub(crate) fn detect_flagm2_available() -> bool {
        true
    }


    #[cfg(test)]
    pub(crate) fn set_flagm_features_for_test(&mut self, flagm: bool, flagm2: bool) {
        self.flagm_available = flagm;
        self.flagm2_available = flagm2;
    }


    pub(crate) fn emit_cond_select(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        cond: u32,
        op: u32,
        op2: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (op << 30)
                | (0b11010100 << 21)
                | ((rm as u32) << 16)
                | (cond << 12)
                | (op2 << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }


    pub(crate) fn emit_cond_compare(
        &mut self,
        rn: u8,
        rm_imm5: u8,
        cond: u32,
        nzcv: u32,
        subtract: bool,
        immediate: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | ((subtract as u32) << 30)
                | (0b111010010 << 21)
                | ((rm_imm5 as u32) << 16)
                | (cond << 12)
                | ((immediate as u32) << 11)
                | ((rn as u32) << 5)
                | (nzcv & 0xf),
        );
        Ok(())
    }


    pub(crate) fn emit_flagm(&mut self, op2: u32) {
        self.emit(0xd500_401f | (op2 << 5));
    }


    pub(crate) fn constant_sub_nzcv(left: i64, right: i64, width: OpWidth) -> Result<u32, LowerError> {
        if !matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native CMP immediate width {width:?}"),
            });
        }

        let mask = width.mask();
        let sign_bit = width.sign_bit();
        let lhs = (left as u64) & mask;
        let rhs = (right as u64) & mask;
        let result = lhs.wrapping_sub(rhs) & mask;
        let n = u32::from((result & sign_bit) != 0);
        let z = u32::from(result == 0);
        let c = u32::from(lhs >= rhs);
        let v = u32::from(((lhs ^ rhs) & (lhs ^ result) & sign_bit) != 0);
        Ok((n << 3) | (z << 2) | (c << 1) | v)
    }


    pub(crate) fn lower_constant_cmp_nzcv(&mut self, nzcv: u32, width: OpWidth) -> Result<(), LowerError> {
        let emit_width = if width == OpWidth::W64 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        match nzcv & 0xf {
            0b0000 => self.emit_sysreg(31, ArmReg::Nzcv, false),
            0b0110 => self.emit_addsub_reg(31, 31, 31, true, true, emit_width),
            // 0b1000 deliberately falls through to the ccmp fallback: encoding
            // it as `emit_addsub_imm(31, 31, 1, ..)` assembles `cmp sp, #1`,
            // taking NZCV from SP - 1 (Rn = 31 is SP in add/sub-immediate).
            fallback => {
                self.emit_addsub_reg(31, 31, 31, true, true, emit_width)?;
                self.emit_cond_compare(31, 31, 1, fallback, true, false, emit_width)
            }
        }
    }


    pub(crate) fn arm_cond_code(cond: Condition) -> Result<u32, LowerError> {
        match cond {
            Condition::Eq => Ok(0),
            Condition::Ne => Ok(1),
            Condition::Uge => Ok(2),
            Condition::Ult => Ok(3),
            Condition::Negative => Ok(4),
            Condition::Positive => Ok(5),
            Condition::Overflow => Ok(6),
            Condition::NoOverflow => Ok(7),
            Condition::Ugt => Ok(8),
            Condition::Ule => Ok(9),
            Condition::Sge => Ok(10),
            Condition::Slt => Ok(11),
            Condition::Sgt => Ok(12),
            Condition::Sle => Ok(13),
            Condition::Always => Ok(14),
            Condition::Parity | Condition::NoParity => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native condition {cond:?}"),
            }),
        }
    }


    pub(crate) fn inverted_arm_cond_code(cond: Condition) -> Result<u32, LowerError> {
        let code = Self::arm_cond_code(cond)?;
        if code < 14 {
            Ok(code ^ 1)
        } else {
            Err(LowerError::UnsupportedOp {
                op: "AArch64 native inverted AL condition".into(),
            })
        }
    }


    pub(crate) fn is_nzcv(vreg: VReg) -> bool {
        matches!(vreg, VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)))
    }


    pub(crate) fn flagm_shl(op: &OpKind, dst: VReg, src: VReg, amount: u32) -> bool {
        matches!(
            op,
            OpKind::Shl {
                dst: op_dst,
                src: op_src,
                amount: op_amount,
                width: OpWidth::W32,
                flags,
            } if *op_dst == dst
                && *op_src == src
                && Self::src_shift_count_eq(op_amount, amount)
                && !flags.updates_any()
        )
    }


    pub(crate) fn flagm_shr(op: &OpKind, dst: VReg, src: VReg, amount: u32) -> bool {
        matches!(
            op,
            OpKind::Shr {
                dst: op_dst,
                src: op_src,
                amount: op_amount,
                width: OpWidth::W32,
                flags,
            } if *op_dst == dst
                && *op_src == src
                && Self::src_shift_count_eq(op_amount, amount)
                && !flags.updates_any()
        )
    }


    pub(crate) fn flagm_or_reg(op: &OpKind, dst: VReg, src1: VReg, src2: VReg) -> bool {
        matches!(
            op,
            OpKind::Or {
                dst: op_dst,
                src1: op_src1,
                src2: op_src2,
                width: OpWidth::W32,
                flags,
            } if *op_dst == dst
                && *op_src1 == src1
                && Self::src_reg_eq(op_src2, src2)
                && !flags.updates_any()
        )
    }


    pub(crate) fn flagm_and_imm(op: &OpKind, dst: VReg, src1: VReg, imm: i64) -> bool {
        matches!(
            op,
            OpKind::And {
                dst: op_dst,
                src1: op_src1,
                src2: op_src2,
                width: OpWidth::W32,
                flags,
            } if *op_dst == dst
                && *op_src1 == src1
                && Self::src_masked_imm_eq(op_src2, imm, OpWidth::W32)
                && !flags.updates_any()
        )
    }


    pub(crate) fn flagm_and_reg(op: &OpKind, dst: VReg, src1: VReg, src2: VReg) -> bool {
        matches!(
            op,
            OpKind::And {
                dst: op_dst,
                src1: op_src1,
                src2: op_src2,
                width: OpWidth::W32,
                flags,
            } if *op_dst == dst
                && *op_src1 == src1
                && Self::src_reg_eq(op_src2, src2)
                && !flags.updates_any()
        )
    }


    pub(crate) fn flagm_andnot_reg(op: &OpKind, dst: VReg, src1: VReg, src2: VReg) -> bool {
        matches!(
            op,
            OpKind::AndNot {
                dst: op_dst,
                src1: op_src1,
                src2: op_src2,
                width: OpWidth::W32,
                flags,
            } if *op_dst == dst
                && *op_src1 == src1
                && Self::src_reg_eq(op_src2, src2)
                && !flags.updates_any()
        )
    }


    pub(crate) fn flagm_mov_to_nzcv(op: &OpKind, src: VReg) -> bool {
        matches!(
            op,
            OpKind::Mov {
                dst,
                src: op_src,
                width: OpWidth::W32,
            } if Self::is_nzcv(*dst) && Self::src_reg_eq(op_src, src)
        )
    }


    pub(crate) fn emit_or_nzcv_const(&mut self, flags: u8, temp: u8, value: i64) -> Result<(), LowerError> {
        self.emit_mov_imm(temp, value, OpWidth::W32)?;
        self.emit_logic_shifted(flags, flags, temp, 0b01, false, 0, 0, OpWidth::W32)
    }


    /// Merge exactly the requested architectural NZCV outputs while retaining
    /// every other incoming flag bit verbatim.
    pub(crate) fn emit_merge_requested_nzcv(
        &mut self,
        saved: u8,
        produced: u8,
        requested: FlagSet,
    ) -> Result<(), LowerError> {
        if !requested.difference(FlagSet::NZCV).is_empty() {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native NZCV merge".into(),
                operand: format!("non-NZCV flag set {requested:?}"),
            });
        }
        if requested.is_empty() {
            return self.emit_sysreg(saved, ArmReg::Nzcv, false);
        }
        if requested == FlagSet::NZCV {
            return self.emit_sysreg(produced, ArmReg::Nzcv, false);
        }

        let mut mask = 0_i64;
        if requested.contains(FlagSet::SF) {
            mask |= NZCV_N;
        }
        if requested.contains(FlagSet::ZF) {
            mask |= NZCV_Z;
        }
        if requested.contains(FlagSet::CF) {
            mask |= NZCV_C;
        }
        if requested.contains(FlagSet::OF) {
            mask |= NZCV_V;
        }
        self.emit_logic_imm_mask(saved, saved, 0b00, !(mask as u32) as i64, OpWidth::W32)?;
        self.emit_logic_imm_mask(produced, produced, 0b00, mask, OpWidth::W32)?;
        self.emit_logic_shifted(saved, saved, produced, 0b01, false, 0, 0, OpWidth::W32)?;
        self.emit_sysreg(saved, ArmReg::Nzcv, false)
    }


    pub(crate) fn lower_test_condition(&mut self, dst: VReg, cond: Condition) -> Result<(), LowerError> {
        if cond == Condition::Always {
            return self.emit_mov_imm(Self::dst_gpr_arm_or_x86(dst)?, 1, OpWidth::W64);
        }
        self.emit_cond_select(
            Self::dst_gpr_arm_or_x86(dst)?,
            31,
            31,
            Self::inverted_arm_cond_code(cond)?,
            0,
            1,
            OpWidth::W64,
        )
    }


    pub(crate) fn lower_setcc(
        &mut self,
        dst: VReg,
        cond: Condition,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64 => {
                if matches!(dst, VReg::Arch(ArchReg::X86(_)))
                    && matches!(width, OpWidth::W8 | OpWidth::W16)
                {
                    let dst = Self::dst_gpr_arm_or_x86(dst)?;
                    let scratches = Self::scratch_regs(&[dst], 1)?;
                    self.emit_scratch_save(&scratches);
                    self.lower_test_condition(
                        VReg::Arch(ArchReg::Arm(ArmReg::X(scratches[0]))),
                        cond,
                    )?;
                    self.emit_bitfield(dst, scratches[0], 0b01, 0, width.bits() - 1, OpWidth::W64)?;
                    self.emit_scratch_restore(&scratches);
                    Ok(())
                } else {
                    self.lower_test_condition(dst, cond)
                }
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native SetCC width {other:?}"),
            }),
        }
    }


    pub(crate) fn cond_select_false_transform(
        op: &OpKind,
    ) -> Option<(VReg, VReg, CondSelectFalseOp, OpWidth)> {
        match op {
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            } if !flags.updates_any()
                && matches!(width, OpWidth::W32 | OpWidth::W64)
                && Self::src_masked_imm_eq(src2, 1, *width) =>
            {
                Some((*dst, *src1, CondSelectFalseOp::Increment, *width))
            }
            OpKind::Not { dst, src, width } => {
                Some((*dst, *src, CondSelectFalseOp::Invert, *width))
            }
            OpKind::Neg {
                dst,
                src,
                width,
                flags,
            } if !flags.updates_any() => Some((*dst, *src, CondSelectFalseOp::Negate, *width)),
            _ => None,
        }
    }


    pub(crate) fn cond_compare_op_args(op: &OpKind) -> Option<(VReg, VReg, &SrcOperand, bool, OpWidth)> {
        match op {
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            } if flags.updates_any() => Some((*dst, *src1, src2, false, *width)),
            OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            } if flags.updates_any() => Some((*dst, *src1, src2, true, *width)),
            _ => None,
        }
    }


    pub(crate) fn cond_compare_src2(src2: &SrcOperand) -> Result<CondCompareSource, LowerError> {
        match src2 {
            SrcOperand::Reg(reg) => Ok(CondCompareSource::Encoded {
                rm_imm5: Self::gpr_arm_or_x86(*reg)?,
                immediate: false,
            }),
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) if (0..=31).contains(imm) => {
                Ok(CondCompareSource::Encoded {
                    rm_imm5: *imm as u8,
                    immediate: true,
                })
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => Ok(CondCompareSource::Immediate(*imm)),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native conditional compare source {other:?}"),
            }),
        }
    }


    pub(crate) fn cond_compare_nzcv(nzcv: i64) -> Result<u32, LowerError> {
        if nzcv >= 0 && (nzcv & !0xf000_0000) == 0 {
            Ok(((nzcv as u32) >> 28) & 0xf)
        } else {
            Err(LowerError::InvalidOperand {
                op: "AArch64 conditional compare fallback NZCV".into(),
                operand: format!("{nzcv:#x}"),
            })
        }
    }


    pub(crate) fn native_nzcv_mask(set: FlagSet) -> Result<i64, LowerError> {
        if !set.difference(FlagSet::NZCV).is_empty() {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 selective NZCV update".into(),
                operand: format!("non-NZCV flag set {set:?}"),
            });
        }
        let mut mask = 0;
        if set.contains(FlagSet::SF) {
            mask |= NZCV_N;
        }
        if set.contains(FlagSet::ZF) {
            mask |= NZCV_Z;
        }
        if set.contains(FlagSet::CF) {
            mask |= NZCV_C;
        }
        if set.contains(FlagSet::OF) {
            mask |= NZCV_V;
        }
        Ok(mask)
    }


    pub(crate) fn lower_with_selected_nzcv<F>(&mut self, set: FlagSet, produce: F) -> Result<(), LowerError>
    where
        F: FnOnce(&mut Self) -> Result<(), LowerError>,
    {
        let mask = Self::native_nzcv_mask(set)?;
        if mask == 0 {
            return produce(self);
        }

        // X16/X17 are outside the admitted AArch32 R0-R14 identity set. Nested
        // lowerers may select the same scratch registers, but every nested use
        // is stack-saved and restores this outer snapshot before returning.
        let scratches = Self::scratch_regs(&[], 2)?;
        let saved = scratches[0];
        let produced = scratches[1];
        self.emit_scratch_save(&scratches);
        self.emit_sysreg(saved, ArmReg::Nzcv, true)?;
        produce(self)?;
        self.emit_sysreg(produced, ArmReg::Nzcv, true)?;
        self.emit_logic_imm_mask(saved, saved, 0b00, !(mask as u32) as i64, OpWidth::W32)?;
        self.emit_logic_imm_mask(produced, produced, 0b00, mask, OpWidth::W32)?;
        self.emit_logic_shifted(saved, saved, produced, 0b01, false, 0, 0, OpWidth::W32)?;
        self.emit_sysreg(saved, ArmReg::Nzcv, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }
}
