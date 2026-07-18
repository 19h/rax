//! Floating-point and fused-multiply lowering

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
    pub(crate) fn detect_fp16_available() -> bool {
        std::arch::is_aarch64_feature_detected!("fp16")
    }


    #[cfg(not(target_arch = "aarch64"))]
    pub(crate) fn detect_fp16_available() -> bool {
        true
    }


    #[cfg(test)]
    pub(crate) fn set_fp16_available_for_test(&mut self, available: bool) {
        self.fp16_available = available;
    }


    pub(crate) fn emit_fp_two_source(
        &mut self,
        rd: u8,
        rn: u8,
        rm: u8,
        opcode: u32,
        precision: FpPrecision,
    ) -> Result<(), LowerError> {
        let fp_type = Self::fp_type(precision)?;
        self.emit(
            (0b00011110 << 24)
                | (fp_type << 22)
                | (1 << 21)
                | ((rm as u32) << 16)
                | ((opcode & 0xf) << 12)
                | (0b10 << 10)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
        Ok(())
    }


    pub(crate) fn emit_fp_one_source(
        &mut self,
        rd: u8,
        rn: u8,
        opcode: u32,
        precision: FpPrecision,
    ) -> Result<(), LowerError> {
        let fp_type = Self::fp_type(precision)?;
        self.emit(
            (0b00011110 << 24)
                | (fp_type << 22)
                | (1 << 21)
                | ((opcode & 0x1f) << 15)
                | (0b10000 << 10)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
        Ok(())
    }


    pub(crate) fn emit_fp_compare(
        &mut self,
        rn: u8,
        rm: u8,
        precision: FpPrecision,
    ) -> Result<(), LowerError> {
        let fp_type = Self::fp_type(precision)?;
        self.emit(
            (0b00011110 << 24)
                | (fp_type << 22)
                | (1 << 21)
                | ((rm as u32) << 16)
                | (0b1000 << 10)
                | ((rn as u32) << 5),
        );
        Ok(())
    }


    pub(crate) fn emit_fp_three_source(
        &mut self,
        rd: u8,
        rn: u8,
        rm: u8,
        ra: u8,
        o1: u32,
        o0: u32,
        precision: FpPrecision,
    ) -> Result<(), LowerError> {
        let fp_type = Self::fp_type(precision)?;
        self.emit(
            (0b00011111 << 24)
                | (fp_type << 22)
                | ((o1 & 1) << 21)
                | ((rm as u32) << 16)
                | ((o0 & 1) << 15)
                | ((ra as u32) << 10)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
        Ok(())
    }


    pub(crate) fn emit_int_to_fp(
        &mut self,
        rd: u8,
        rn: u8,
        int_width: OpWidth,
        fp_precision: FpPrecision,
        signed: bool,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(int_width)?;
        let ptype = Self::fp_type(fp_precision)?;
        let opcode = if signed { 0b010 } else { 0b011 };
        self.emit(
            (sf << 31)
                | (0b0011110 << 24)
                | (ptype << 22)
                | (1 << 21)
                | (opcode << 16)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
        Ok(())
    }


    pub(crate) fn emit_fp_to_int(
        &mut self,
        rd: u8,
        rn: u8,
        fp_precision: FpPrecision,
        int_width: OpWidth,
        signed: bool,
        round: FpRoundMode,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(int_width)?;
        let ptype = Self::fp_type(fp_precision)?;
        let (rmode, opcode) = if round == FpRoundMode::RoundNearestTiesAway {
            (0b00, if signed { 0b100 } else { 0b101 })
        } else {
            (
                Self::fp_to_int_rmode(round)?,
                if signed { 0b000 } else { 0b001 },
            )
        };
        self.emit(
            (sf << 31)
                | (0b0011110 << 24)
                | (ptype << 22)
                | (1 << 21)
                | (rmode << 19)
                | (opcode << 16)
                | ((rn as u32) << 5)
                | (rd as u32),
        );
        Ok(())
    }


    /// Lower a vector FP numeric min/max (FMAXNM/FMINNM): three-same FP, U=0,
    /// opcode 11000, with a = size<1> selecting max (0) vs min (1) and sz the
    /// single/double element width.
    pub(crate) fn lower_vfminmaxnm(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        lanes: u8,
        min: bool,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        let (q, sz) = Self::simd_float_shape(elem, lanes)?;
        let size = if min { 0b10 | sz } else { sz };
        self.emit_simd_three_same(rd, rn, rm, q, 0, size, 0b11000);
        Ok(())
    }


    pub(crate) fn lower_fp_binary(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        precision: FpPrecision,
        opcode: u32,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        self.emit_fp_two_source(rd, rn, rm, opcode, precision)
    }


    pub(crate) fn lower_fp_unary(
        &mut self,
        dst: VReg,
        src: VReg,
        precision: FpPrecision,
        opcode: u32,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        self.emit_fp_one_source(rd, rn, opcode, precision)
    }


    pub(crate) fn lower_fp_round(
        &mut self,
        dst: VReg,
        src: VReg,
        precision: FpPrecision,
        mode: FpRoundMode,
    ) -> Result<(), LowerError> {
        let opcode = match mode {
            FpRoundMode::RoundNearest => 0b01000,         // FRINTN
            FpRoundMode::RoundUp => 0b01001,              // FRINTP
            FpRoundMode::RoundDown => 0b01010,            // FRINTM
            FpRoundMode::RoundTowardZero => 0b01011,      // FRINTZ
            FpRoundMode::RoundNearestTiesAway => 0b01100, // FRINTA
            FpRoundMode::Dynamic => 0b01111,              // FRINTI
        };
        self.lower_fp_unary(dst, src, precision, opcode)
    }


    pub(crate) fn lower_fp_compare(
        &mut self,
        src1: VReg,
        src2: VReg,
        precision: FpPrecision,
    ) -> Result<(), LowerError> {
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        self.emit_fp_compare(rn, rm, precision)
    }


    pub(crate) fn lower_fp_convert(
        &mut self,
        dst: VReg,
        src: VReg,
        from: FpPrecision,
        to: FpPrecision,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src)?;
        if from == to {
            self.emit_fp_one_source(rd, rn, 0, from)
        } else {
            let opcode = Self::fp_convert_opcode(to)?;
            self.emit_fp_one_source(rd, rn, opcode, from)
        }
    }


    pub(crate) fn lower_int_to_fp(
        &mut self,
        dst: VReg,
        src: VReg,
        int_width: OpWidth,
        fp_precision: FpPrecision,
        signed: bool,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::gpr_arm_or_x86(src)?;
        match int_width {
            OpWidth::W32 | OpWidth::W64 => {
                self.emit_int_to_fp(rd, rn, int_width, fp_precision, signed)
            }
            OpWidth::W8 | OpWidth::W16 => {
                let scratches = Self::scratch_regs(&[rn], 1)?;
                let scratch = scratches[0];
                self.emit_scratch_save(&scratches);
                self.emit_bitfield(
                    scratch,
                    rn,
                    if signed { 0b00 } else { 0b10 },
                    0,
                    int_width.bits() - 1,
                    OpWidth::W32,
                )?;
                self.emit_int_to_fp(rd, scratch, OpWidth::W32, fp_precision, signed)?;
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native IntToFp width {other:?}"),
            }),
        }
    }


    pub(crate) fn lower_fp_to_int(
        &mut self,
        dst: VReg,
        src: VReg,
        fp_precision: FpPrecision,
        int_width: OpWidth,
        signed: bool,
        round: FpRoundMode,
    ) -> Result<(), LowerError> {
        let rd = Self::dst_gpr_arm_or_x86(dst)?;
        let rn = Self::fp_reg(src)?;
        let lower_width = match int_width {
            OpWidth::W8 | OpWidth::W16 => OpWidth::W64,
            OpWidth::W32 | OpWidth::W64 => int_width,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native FpToInt width {other:?}"),
                });
            }
        };

        if round == FpRoundMode::Dynamic {
            self.lower_fp_to_int_dynamic(rd, rn, fp_precision, lower_width, signed)?;
        } else {
            self.emit_fp_to_int(rd, rn, fp_precision, lower_width, signed, round)?;
        }

        match int_width {
            OpWidth::W8 | OpWidth::W16 => {
                self.emit_bitfield(rd, rd, 0b10, 0, int_width.bits() - 1, OpWidth::W32)
            }
            OpWidth::W32 | OpWidth::W64 => Ok(()),
            _ => unreachable!(),
        }
    }


    pub(crate) fn lower_fp_to_int_dynamic(
        &mut self,
        rd: u8,
        rn: u8,
        fp_precision: FpPrecision,
        int_width: OpWidth,
        signed: bool,
    ) -> Result<(), LowerError> {
        Self::fp_type(fp_precision)?;
        Self::sf(int_width)?;

        let scratches = Self::scratch_regs(&[rd], 1)?;
        let rmode_reg = scratches[0];
        let mut done_branches = Vec::with_capacity(3);

        self.emit_scratch_save(&scratches);
        self.emit_sysreg(rmode_reg, ArmReg::Fpcr, true)?;
        self.emit_bitfield(rmode_reg, rmode_reg, 0b10, 22, 23, OpWidth::W32)?;

        let not_nearest = self.code.position();
        self.emit(0xb500_0000 | (rmode_reg as u32));
        self.emit_fp_to_int(
            rd,
            rn,
            fp_precision,
            int_width,
            signed,
            FpRoundMode::RoundNearest,
        )?;
        let done = self.code.position();
        self.emit(0x1400_0000);
        done_branches.push(done);
        self.patch_compare_branch_to_current(not_nearest, rmode_reg, true)?;

        self.emit_addsub_imm(rmode_reg, rmode_reg, 1, true, false, OpWidth::W32)?;
        let not_up = self.code.position();
        self.emit(0xb500_0000 | (rmode_reg as u32));
        self.emit_fp_to_int(
            rd,
            rn,
            fp_precision,
            int_width,
            signed,
            FpRoundMode::RoundUp,
        )?;
        let done = self.code.position();
        self.emit(0x1400_0000);
        done_branches.push(done);
        self.patch_compare_branch_to_current(not_up, rmode_reg, true)?;

        self.emit_addsub_imm(rmode_reg, rmode_reg, 1, true, false, OpWidth::W32)?;
        let not_down = self.code.position();
        self.emit(0xb500_0000 | (rmode_reg as u32));
        self.emit_fp_to_int(
            rd,
            rn,
            fp_precision,
            int_width,
            signed,
            FpRoundMode::RoundDown,
        )?;
        let done = self.code.position();
        self.emit(0x1400_0000);
        done_branches.push(done);
        self.patch_compare_branch_to_current(not_down, rmode_reg, true)?;

        self.emit_fp_to_int(
            rd,
            rn,
            fp_precision,
            int_width,
            signed,
            FpRoundMode::RoundTowardZero,
        )?;
        for done in done_branches {
            self.patch_branch_to_current(done)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn lower_fp_fma(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src3: VReg,
        precision: FpPrecision,
    ) -> Result<(), LowerError> {
        let rd = Self::fp_reg(dst)?;
        let rn = Self::fp_reg(src1)?;
        let rm = Self::fp_reg(src2)?;
        let ra = Self::fp_reg(src3)?;
        self.emit_fp_three_source(rd, rn, rm, ra, 0, 0, precision)
    }


    pub(crate) fn scratch_fp_reg(avoid: &[u8]) -> Result<u8, LowerError> {
        for reg in (0_u8..=31).rev() {
            if !avoid.contains(&reg) {
                return Ok(reg);
            }
        }

        Err(LowerError::UnsupportedOp {
            op: "AArch64 native lowering needs a SIMD scratch register".to_string(),
        })
    }


    pub(crate) fn scratch_fp_reg_pair(avoid: &[u8]) -> Result<(u8, u8), LowerError> {
        for first in (0_u8..31).rev() {
            let second = first + 1;
            if !avoid.contains(&first) && !avoid.contains(&second) {
                return Ok((first, second));
            }
        }

        Err(LowerError::UnsupportedOp {
            op: "AArch64 native lowering needs a consecutive SIMD scratch pair".to_string(),
        })
    }


    pub(crate) fn double_shift_count_mask(width: OpWidth) -> Result<u64, LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => Ok(0x1f),
            OpWidth::W64 => Ok(0x3f),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native flag-setting double shift width {other:?}"),
            }),
        }
    }


    pub(crate) fn emit_double_shift_carry_imm(
        &mut self,
        flags: u8,
        temp: u8,
        original: u8,
        count: u32,
        left: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let bit = if left {
            width.bits() - count
        } else {
            count - 1
        };
        let no_carry = self.code.position();
        self.emit_test_branch(original, bit, false, 0)?;
        self.emit_or_nzcv_const(flags, temp, NZCV_C)?;
        self.patch_test_branch_to_current(no_carry, original, bit, false)
    }


    pub(crate) fn emit_double_shift_carry_reg(
        &mut self,
        flags: u8,
        temp: u8,
        original: u8,
        count: u8,
        left: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if left {
            self.emit_mov_imm(temp, i64::from(width.bits()), OpWidth::W64)?;
            self.emit_addsub_reg(temp, temp, count, true, false, OpWidth::W64)?;
        } else {
            self.emit_addsub_imm(temp, count, 1, true, false, OpWidth::W64)?;
        }
        self.emit_dp2(temp, original, temp, 0b1001, OpWidth::W64)?;
        self.emit_bitfield(temp, temp, 0b10, 0, 0, OpWidth::W32)?;
        self.emit_logic_shifted(flags, flags, temp, 0b01, false, 0, 29, OpWidth::W32)
    }


    pub(crate) fn emit_double_shift_overflow_from_result(
        &mut self,
        flags: u8,
        temp: u8,
        result: u8,
        original: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let emit_width = Self::shift_emit_width(width)?;
        let top_bit = width.bits() - 1;
        self.emit_logic_shifted(temp, result, original, 0b10, false, 0, 0, emit_width)?;
        let no_overflow = self.code.position();
        self.emit_test_branch(temp, top_bit, false, 0)?;
        self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
        self.patch_test_branch_to_current(no_overflow, temp, top_bit, false)
    }


    pub(crate) fn emit_finalize_double_shift_flags(
        &mut self,
        result: u8,
        original: u8,
        count_reg: Option<u8>,
        imm_count: Option<u32>,
        width: OpWidth,
        left: bool,
        flags: u8,
        temp: u8,
    ) -> Result<(), LowerError> {
        self.emit_init_shift_nz_flags(flags, temp, result, width)?;
        if let Some(count) = imm_count {
            self.emit_double_shift_carry_imm(flags, temp, original, count, left, width)?;
            if count == 1 {
                self.emit_double_shift_overflow_from_result(flags, temp, result, original, width)?;
            }
        } else {
            let count = count_reg.expect("register-count double shift flags need a count register");
            self.emit_double_shift_carry_reg(flags, temp, original, count, left, width)?;

            self.emit_addsub_imm(31, count, 1, true, true, OpWidth::W64)?;
            let not_one = self.code.position();
            self.emit(0x5400_0000 | Self::arm_cond_code(Condition::Ne)?);
            self.emit_double_shift_overflow_from_result(flags, temp, result, original, width)?;
            self.patch_cond_branch_to_current(not_one, Self::arm_cond_code(Condition::Ne)?)?;
        }
        self.emit_sysreg(flags, ArmReg::Nzcv, false)
    }


    pub(crate) fn lower_double_shift_imm_with_flags(
        &mut self,
        dst: u8,
        src: u8,
        amount: i64,
        left: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        Self::shift_emit_width(width)?;
        let mask = Self::double_shift_count_mask(width)?;
        let count = (amount as u64 & mask) as u32;
        let bits = width.bits();
        let top_bit = bits - 1;
        if count == 0 {
            if matches!(width, OpWidth::W8 | OpWidth::W16) {
                return self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32);
            }
            return self.emit_mov_reg(dst, dst, width);
        }
        if matches!(width, OpWidth::W8 | OpWidth::W16) && count > bits {
            return Err(LowerError::UnsupportedOp {
                op: format!(
                    "AArch64 native flag-setting {width:?} {} count greater than width",
                    if left { "Shld" } else { "Shrd" }
                ),
            });
        }

        let scratches = Self::scratch_regs(&[dst, src], 6)?;
        let original = scratches[0];
        let source = scratches[1];
        let left_part = scratches[2];
        let right_part = scratches[3];
        let flags = scratches[4];
        let temp = scratches[5];
        let emit_width = Self::shift_emit_width(width)?;

        self.emit_scratch_save(&scratches);
        self.emit_prepare_shift_flag_source(original, dst, width)?;
        self.emit_prepare_shift_flag_source(source, src, width)?;
        if left {
            self.lower_shift_imm(left_part, original, i64::from(count), ShiftOp::Lsl, width)?;
            self.lower_shift_imm(
                right_part,
                source,
                i64::from(bits - count),
                ShiftOp::Lsr,
                width,
            )?;
        } else {
            self.lower_shift_imm(left_part, original, i64::from(count), ShiftOp::Lsr, width)?;
            self.lower_shift_imm(
                right_part,
                source,
                i64::from(bits - count),
                ShiftOp::Lsl,
                width,
            )?;
        }
        self.emit_logic_shifted(dst, left_part, right_part, 0b01, false, 0, 0, emit_width)?;
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)?;
        }
        self.emit_finalize_double_shift_flags(
            dst,
            original,
            None,
            Some(count),
            width,
            left,
            flags,
            temp,
        )?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn lower_double_shift_reg_with_flags(
        &mut self,
        dst: u8,
        src: u8,
        amount: u8,
        left: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let mask = match width {
            OpWidth::W32 => 0x1f,
            OpWidth::W64 => 0x3f,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!(
                        "AArch64 native flag-setting register-count double shift width {other:?}"
                    ),
                });
            }
        };

        let scratches = Self::scratch_regs(&[dst, src, amount], 5)?;
        let original = scratches[0];
        let count = scratches[1];
        let shift_count = scratches[2];
        let left_part = scratches[3];
        let right_part = scratches[4];
        self.emit_scratch_save(&scratches);
        self.emit_prepare_shift_flag_source(original, dst, width)?;
        self.emit_mov_reg(count, amount, OpWidth::W64)?;
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(mask, OpWidth::W64)?;
        self.emit_logic_imm(count, count, 0b00, imm_n, immr, imms, OpWidth::W64)?;

        let zero_count = self.code.position();
        self.emit(0xb400_0000 | u32::from(count));
        if left {
            self.emit_dp2(left_part, original, count, 0b1000, width)?;
            self.emit_addsub_reg(shift_count, 31, count, true, false, OpWidth::W64)?;
            self.emit_dp2(right_part, src, shift_count, 0b1001, width)?;
        } else {
            self.emit_dp2(left_part, original, count, 0b1001, width)?;
            self.emit_addsub_reg(shift_count, 31, count, true, false, OpWidth::W64)?;
            self.emit_dp2(right_part, src, shift_count, 0b1000, width)?;
        }
        self.emit_logic_shifted(dst, left_part, right_part, 0b01, false, 0, 0, width)?;
        self.emit_finalize_double_shift_flags(
            dst,
            original,
            Some(count),
            None,
            width,
            left,
            left_part,
            right_part,
        )?;
        self.emit_scratch_restore(&scratches);
        let done = self.code.position();
        self.emit(0x1400_0000);

        self.patch_compare_branch_to_current(zero_count, count, false)?;
        self.emit_mov_reg(dst, original, width)?;
        self.emit_scratch_restore(&scratches);
        self.patch_branch_to_current(done)
    }


    pub(crate) fn lower_double_shift_reg(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: u8,
        left: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            return self.lower_subword_double_shift_reg(
                Self::dst_gpr_arm_or_x86(dst)?,
                Self::gpr_arm_or_x86(src)?,
                amount,
                left,
                width,
            );
        }

        let mask = match width {
            OpWidth::W32 => 0x1f,
            OpWidth::W64 => 0x3f,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native register-count double shift width {other:?}"),
                });
            }
        };

        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let src_reg = Self::gpr_arm_or_x86(src)?;
        let scratches = Self::scratch_regs(&[dst_reg, src_reg, amount], 3)?;
        let count = scratches[0];
        let left_part = scratches[1];
        let right_part = scratches[2];

        self.emit_scratch_save(&scratches);
        self.emit_mov_reg(left_part, dst_reg, width)?;
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(mask, width)?;
        self.emit_logic_imm(count, amount, 0b00, imm_n, immr, imms, width)?;
        self.emit_mov_reg(dst_reg, left_part, width)?;
        let zero_count = self.code.position();
        self.emit(0xb400_0000 | u32::from(count));

        if left {
            self.emit_dp2(left_part, left_part, count, 0b1000, width)?;
            self.emit_addsub_reg(count, 31, count, true, false, width)?;
            self.emit_dp2(right_part, src_reg, count, 0b1001, width)?;
        } else {
            self.emit_dp2(left_part, left_part, count, 0b1001, width)?;
            self.emit_addsub_reg(count, 31, count, true, false, width)?;
            self.emit_dp2(right_part, src_reg, count, 0b1000, width)?;
        }
        self.emit_logic_shifted(dst_reg, left_part, right_part, 0b01, false, 0, 0, width)?;
        self.patch_compare_branch_to_current(zero_count, count, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn lower_subword_double_shift_reg(
        &mut self,
        dst: u8,
        src: u8,
        amount: u8,
        left: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let bits = width.bits();
        let top_bit = bits - 1;
        let scratches = Self::scratch_regs(&[dst, src, amount], 4)?;
        let original = scratches[0];
        let source = scratches[1];
        let count = scratches[2];
        let temp = scratches[3];

        self.emit_scratch_save(&scratches);
        self.emit_bitfield(original, dst, 0b10, 0, top_bit, OpWidth::W32)?;
        self.emit_bitfield(source, src, 0b10, 0, top_bit, OpWidth::W32)?;
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(0x1f, OpWidth::W64)?;
        self.emit_logic_imm(count, amount, 0b00, imm_n, immr, imms, OpWidth::W64)?;

        let zero_count = self.code.position();
        self.emit(0xb400_0000 | u32::from(count));
        let width_or_larger_bits: &[u32] = match width {
            OpWidth::W8 => &[3, 4],
            OpWidth::W16 => &[4],
            _ => unreachable!("subword double shift width already checked"),
        };
        let mut width_or_larger = Vec::with_capacity(width_or_larger_bits.len());
        for &bit in width_or_larger_bits {
            let offset = self.code.position();
            self.emit_test_branch(count, bit, true, 0)?;
            width_or_larger.push((offset, bit));
        }

        if left {
            self.emit_dp2(dst, original, count, 0b1000, OpWidth::W32)?;
            self.emit_mov_imm(temp, i64::from(bits), OpWidth::W64)?;
            self.emit_addsub_reg(temp, temp, count, true, false, OpWidth::W64)?;
            self.emit_dp2(temp, source, temp, 0b1001, OpWidth::W32)?;
        } else {
            self.emit_dp2(dst, original, count, 0b1001, OpWidth::W32)?;
            self.emit_mov_imm(temp, i64::from(bits), OpWidth::W64)?;
            self.emit_addsub_reg(temp, temp, count, true, false, OpWidth::W64)?;
            self.emit_dp2(temp, source, temp, 0b1000, OpWidth::W32)?;
        }
        self.emit_logic_shifted(dst, dst, temp, 0b01, false, 0, 0, OpWidth::W32)?;
        self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)?;
        let done_main = self.code.position();
        self.emit(0x1400_0000);

        for (offset, bit) in width_or_larger {
            self.patch_test_branch_to_current(offset, count, bit, true)?;
        }
        self.emit_addsub_imm(temp, count, i64::from(bits), true, false, OpWidth::W64)?;
        let exact_width = self.code.position();
        self.emit(0xb400_0000 | u32::from(temp));
        self.emit_mov_reg(dst, original, OpWidth::W32)?;
        let done_undefined = self.code.position();
        self.emit(0x1400_0000);

        self.patch_compare_branch_to_current(exact_width, temp, false)?;
        self.emit_mov_reg(dst, source, OpWidth::W32)?;
        let done_source = self.code.position();
        self.emit(0x1400_0000);

        self.patch_compare_branch_to_current(zero_count, count, false)?;
        self.emit_mov_reg(dst, original, OpWidth::W32)?;

        self.patch_branch_to_current(done_main)?;
        self.patch_branch_to_current(done_undefined)?;
        self.patch_branch_to_current(done_source)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn lower_double_shift(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        left: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if width == OpWidth::W16 {
            if let Some((dst_reg, result)) =
                Self::x86_partial_write_scratch(dst, width, &[src], &[amount])?
            {
                let scratches = [result];
                self.emit_scratch_save(&scratches);
                self.emit_bitfield(result, dst_reg, 0b10, 0, 15, OpWidth::W32)?;
                self.lower_double_shift(
                    Self::arm_x_reg(result),
                    src,
                    amount,
                    left,
                    set_flags,
                    width,
                )?;
                self.emit_bitfield(dst_reg, result, 0b01, 0, 15, OpWidth::W64)?;
                self.emit_scratch_restore(&scratches);
                return Ok(());
            }
        }

        if set_flags {
            let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
            let src_reg = Self::gpr_arm_or_x86(src)?;
            return match amount {
                SrcOperand::Reg(amount) => self.lower_double_shift_reg_with_flags(
                    dst_reg,
                    src_reg,
                    Self::gpr_arm_or_x86(*amount)?,
                    left,
                    width,
                ),
                SrcOperand::Imm(amount) | SrcOperand::Imm64(amount) => {
                    self.lower_double_shift_imm_with_flags(dst_reg, src_reg, *amount, left, width)
                }
                other => Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native double shift amount {other:?}"),
                }),
            };
        }
        if let SrcOperand::Reg(amount) = amount {
            return self.lower_double_shift_reg(
                dst,
                src,
                Self::gpr_arm_or_x86(*amount)?,
                left,
                width,
            );
        }
        let Some(amount) = Self::src_imm(amount) else {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native register-count double shift".into(),
            });
        };

        let bits = width.bits();
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            let amount = (amount as u64 & 0x1f) as u32;
            let top_bit = bits - 1;
            let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
            let rn = Self::gpr_arm_or_x86(dst)?;
            if amount == 0 {
                return self.emit_bitfield(dst_reg, rn, 0b10, 0, top_bit, OpWidth::W32);
            }
            if let VReg::Imm(value) = src {
                let value = (value as u64) & width.mask();
                if amount > bits {
                    return self.emit_bitfield(dst_reg, rn, 0b10, 0, top_bit, OpWidth::W32);
                }
                if amount == bits {
                    return self.emit_mov_imm(dst_reg, value as i64, OpWidth::W32);
                }
                let injected = if left {
                    value >> (bits - amount)
                } else {
                    (value << (bits - amount)) & width.mask()
                };
                let shift = if left { ShiftOp::Lsl } else { ShiftOp::Lsr };
                if injected == 0 {
                    let shift_src = if left {
                        rn
                    } else {
                        self.emit_bitfield(dst_reg, rn, 0b10, 0, top_bit, OpWidth::W32)?;
                        dst_reg
                    };
                    self.lower_shift_imm(
                        dst_reg,
                        shift_src,
                        i64::from(amount),
                        shift,
                        OpWidth::W32,
                    )?;
                    return self.emit_bitfield(dst_reg, dst_reg, 0b10, 0, top_bit, OpWidth::W32);
                }
                if let Ok((n, immr, imms)) =
                    Self::logical_bitmask_imm(injected as i64, OpWidth::W32)
                {
                    let shift_src = if left {
                        rn
                    } else {
                        self.emit_bitfield(dst_reg, rn, 0b10, 0, top_bit, OpWidth::W32)?;
                        dst_reg
                    };
                    self.lower_shift_imm(
                        dst_reg,
                        shift_src,
                        i64::from(amount),
                        shift,
                        OpWidth::W32,
                    )?;
                    self.emit_logic_imm(dst_reg, dst_reg, 0b01, n, immr, imms, OpWidth::W32)?;
                    return self.emit_bitfield(dst_reg, dst_reg, 0b10, 0, top_bit, OpWidth::W32);
                }
            }
            let src = Self::gpr_arm_or_x86(src)?;
            if amount > bits {
                return self.emit_bitfield(dst_reg, rn, 0b10, 0, top_bit, OpWidth::W32);
            }
            if amount == bits {
                return self.emit_bitfield(dst_reg, src, 0b10, 0, top_bit, OpWidth::W32);
            }
            let scratches = if dst_reg == src {
                Self::scratch_regs(&[dst_reg, src], 1)?
            } else {
                Vec::new()
            };
            let insert_src = scratches.first().copied().unwrap_or(src);
            self.emit_scratch_save(&scratches);
            if dst_reg == src {
                self.emit_mov_reg(insert_src, src, OpWidth::W32)?;
            }
            if left {
                self.lower_shift_imm(dst_reg, rn, i64::from(amount), ShiftOp::Lsl, OpWidth::W32)?;
                self.emit_bitfield(
                    dst_reg,
                    insert_src,
                    0b01,
                    bits - amount,
                    top_bit,
                    OpWidth::W32,
                )?;
            } else {
                self.lower_shift_imm(dst_reg, rn, i64::from(amount), ShiftOp::Lsr, OpWidth::W32)?;
                if dst_reg == src {
                    self.emit_logic_shifted(
                        dst_reg,
                        dst_reg,
                        insert_src,
                        0b01,
                        false,
                        0b00,
                        bits - amount,
                        OpWidth::W32,
                    )?;
                } else {
                    let lsb = bits - amount;
                    let immr = if lsb == 0 {
                        0
                    } else {
                        OpWidth::W32.bits() - lsb
                    };
                    self.emit_bitfield(dst_reg, insert_src, 0b01, immr, amount - 1, OpWidth::W32)?;
                }
            }
            self.emit_bitfield(dst_reg, dst_reg, 0b10, 0, top_bit, OpWidth::W32)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        let mask = match width {
            OpWidth::W32 => 0x1f,
            OpWidth::W64 => 0x3f,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native double shift width {other:?}"),
                });
            }
        };
        let amount = (amount as u64 & mask) as u32;
        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let rn = Self::gpr_arm_or_x86(dst)?;
        if amount == 0 {
            if width == OpWidth::W64 {
                return Ok(());
            }
            return self.emit_mov_reg(dst_reg, rn, width);
        }

        if let VReg::Imm(value) = src {
            let value = (value as u64) & width.mask();
            let injected = if left {
                value >> (bits - amount)
            } else {
                (value << (bits - amount)) & width.mask()
            };
            let shift = if left { ShiftOp::Lsl } else { ShiftOp::Lsr };
            if injected == 0 {
                return self.lower_shift_imm(dst_reg, rn, i64::from(amount), shift, width);
            }
            if let Ok((n, immr, imms)) = Self::logical_bitmask_imm(injected as i64, width) {
                self.lower_shift_imm(dst_reg, rn, i64::from(amount), shift, width)?;
                return self.emit_logic_imm(dst_reg, dst_reg, 0b01, n, immr, imms, width);
            }
        }

        let src = Self::gpr_arm_or_x86(src)?;
        let (rn, rm, lsb) = if left {
            (rn, src, bits - amount)
        } else {
            (src, rn, amount)
        };
        self.emit_extract(dst_reg, rn, rm, lsb, width)
    }


    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_x86_ndd_double_shift(
        &mut self,
        dst: VReg,
        base: VReg,
        fill: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        left: bool,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native APX NDD double shift width {width:?}"),
            });
        }
        if width == OpWidth::W16 && flags.updates_any() && matches!(amount, SrcOperand::Reg(_)) {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native flag-setting W16 APX NDD double shift with register count"
                    .into(),
            });
        }

        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let base_reg = Self::gpr_arm_or_x86(base)?;
        let fill_reg = Self::gpr_arm_or_x86(fill)?;
        let amount_reg = match amount {
            SrcOperand::Imm(_) => None,
            SrcOperand::Reg(amount) => {
                let amount = Self::gpr_arm_or_x86(*amount)?;
                if amount != 1 {
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native APX NDD double shift register count must be CL".into(),
                    });
                }
                Some(amount)
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native APX NDD double shift amount {other:?}"),
                });
            }
        };
        let mut avoid = vec![dst_reg, base_reg, fill_reg];
        if let Some(amount) = amount_reg {
            avoid.push(amount);
        }
        let result = Self::scratch_regs(&avoid, 1)?[0];
        let scratches = [result];

        self.emit_scratch_save(&scratches);
        if width == OpWidth::W16 {
            self.emit_bitfield(result, base_reg, 0b10, 0, 15, OpWidth::W32)?;
        } else {
            self.emit_mov_reg(result, base_reg, width)?;
        }
        self.lower_double_shift(
            Self::arm_x_reg(result),
            fill,
            amount,
            left,
            flags.updates_any(),
            width,
        )?;
        if width == OpWidth::W16 {
            self.emit_bitfield(dst_reg, result, 0b01, 0, 15, OpWidth::W64)?;
        } else {
            self.emit_mov_reg(dst_reg, result, width)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }


    pub(crate) fn lower_fused_select(
        &mut self,
        dst: VReg,
        cond: Condition,
        src_true: VReg,
        src_false_base: VReg,
        false_op: CondSelectFalseOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let cond = Self::arm_cond_code(cond)?;
        self.lower_fused_select_cond(dst, cond, src_true, src_false_base, false_op, width)
    }


    pub(crate) fn lower_fused_select_cond(
        &mut self,
        dst: VReg,
        cond: u32,
        src_true: VReg,
        src_false_base: VReg,
        false_op: CondSelectFalseOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let (op, op2) = match false_op {
            CondSelectFalseOp::Identity => (0, 0),
            CondSelectFalseOp::Increment => (0, 1),
            CondSelectFalseOp::Invert => (1, 0),
            CondSelectFalseOp::Negate => (1, 1),
        };
        self.emit_cond_select(
            Self::dst_gpr_arm_or_x86(dst)?,
            Self::gpr_arm_or_x86(src_true)?,
            Self::gpr_arm_or_x86(src_false_base)?,
            cond,
            op,
            op2,
            width,
        )
    }


    pub(crate) fn try_lower_fused_flagm(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        if let Some(SmirOp {
            kind:
                OpKind::Xor {
                    dst,
                    src1,
                    src2,
                    width: OpWidth::W32,
                    flags,
                },
            ..
        }) = ops.first()
        {
            if Self::is_nzcv(*dst)
                && Self::is_nzcv(*src1)
                && Self::src_masked_imm_eq(src2, NZCV_C, OpWidth::W32)
                && !flags.updates_any()
            {
                self.lower_cfinv()?;
                return Ok(Some(1));
            }
        }

        if Self::matches_axflag_ops(ops) {
            self.lower_axflag()?;
            return Ok(Some(8));
        }

        if Self::matches_xaflag_ops(ops) {
            self.lower_xaflag()?;
            return Ok(Some(16));
        }

        Ok(None)
    }


    pub(crate) fn try_lower_fused_cls(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        let [
            SmirOp {
                kind:
                    OpKind::Sar {
                        dst: sign_mask,
                        src,
                        amount,
                        width,
                        flags,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Xor {
                        dst: normalized,
                        src1: xor_src,
                        src2,
                        width: xor_width,
                        flags: xor_flags,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Clz {
                        dst: leading,
                        src: clz_src,
                        width: clz_width,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Sub {
                        dst,
                        src1: sub_src,
                        src2: sub_amount,
                        width: sub_width,
                        flags: sub_flags,
                    },
                ..
            },
            ..,
        ] = ops
        else {
            return Ok(None);
        };

        if flags.updates_any()
            || xor_flags.updates_any()
            || sub_flags.updates_any()
            || !matches!(width, OpWidth::W32 | OpWidth::W64)
            || xor_width != width
            || clz_width != width
            || sub_width != width
            || !Self::src_shift_count_eq(amount, width.bits() - 1)
            || xor_src != src
            || !Self::src_reg_eq(src2, *sign_mask)
            || clz_src != normalized
            || sub_src != leading
            || !Self::src_masked_imm_eq(sub_amount, 1, *width)
            // The three intermediates must be dead virtual scratch: fusing to a
            // single CLS writes only `dst` and never sign_mask/normalized/leading.
            // If any is an architectural register (a real guest SAR/EOR/CLZ
            // sequence), dropping its write would leave that register stale. (#8)
            || !matches!(sign_mask, VReg::Virtual(_))
            || !matches!(normalized, VReg::Virtual(_))
            || !matches!(leading, VReg::Virtual(_))
        {
            return Ok(None);
        }

        self.lower_cls(*dst, *src, *width)?;
        Ok(Some(4))
    }


    pub(crate) fn try_lower_fused_signed_load_w(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        if let [writeback, load, extend, ..] = ops {
            if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                if let Some((rt, addr, size, opc)) =
                    Self::signed_load_w_parts(&load.kind, &extend.kind)?
                {
                    if Self::direct_addr_reg(addr) == Some(base)
                        && !Self::transfer_reg_aliases_base(rt, base)
                        && (-256..=255).contains(&offset)
                    {
                        self.lower_mem_indexed_access(rt, base, size, opc, offset, 0b11)?;
                        return Ok(Some(3));
                    }
                }
            }
        }

        if let [load, extend, writeback, ..] = ops {
            if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                if let Some((rt, addr, size, opc)) =
                    Self::signed_load_w_parts(&load.kind, &extend.kind)?
                {
                    if Self::direct_addr_reg(addr) == Some(base)
                        && !Self::transfer_reg_aliases_base(rt, base)
                        && (-256..=255).contains(&offset)
                    {
                        self.lower_mem_indexed_access(rt, base, size, opc, offset, 0b01)?;
                        return Ok(Some(3));
                    }
                }
            }
        }

        if let [load, extend, ..] = ops {
            if let Some((rt, addr, size, opc)) =
                Self::signed_load_w_parts(&load.kind, &extend.kind)?
            {
                self.lower_mem_access(rt, addr, size, opc)?;
                return Ok(Some(2));
            }
        }

        Ok(None)
    }


    pub(crate) fn try_lower_fused_mem_indexed(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        if let [writeback, access, ..] = ops {
            if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                if let Some((rt, addr, size, opc)) = Self::mem_access_parts(&access.kind)? {
                    if Self::direct_addr_reg(addr) == Some(base)
                        && !Self::transfer_reg_aliases_base(rt, base)
                        && (-256..=255).contains(&offset)
                    {
                        self.lower_mem_indexed_access(rt, base, size, opc, offset, 0b11)?;
                        return Ok(Some(2));
                    }
                }
            }
        }

        if let [access, writeback, ..] = ops {
            if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                if let Some((rt, addr, size, opc)) = Self::mem_access_parts(&access.kind)? {
                    if Self::direct_addr_reg(addr) == Some(base)
                        && !Self::transfer_reg_aliases_base(rt, base)
                        && (-256..=255).contains(&offset)
                    {
                        self.lower_mem_indexed_access(rt, base, size, opc, offset, 0b01)?;
                        return Ok(Some(2));
                    }
                }
            }
        }

        Ok(None)
    }


    pub(crate) fn try_lower_fused_pair_indexed(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        if let [writeback, access, ..] = ops {
            if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                if let Some((rt, rt2, addr, width, load)) = Self::pair_access_parts(&access.kind)? {
                    if Self::direct_addr_reg(addr) == Some(base)
                        && !Self::transfer_reg_aliases_base(rt, base)
                        && !Self::transfer_reg_aliases_base(rt2, base)
                        && Self::pair_scaled_imm(width, offset)?.is_some()
                    {
                        self.lower_pair_indexed_access(rt, rt2, base, width, load, offset, 0b11)?;
                        return Ok(Some(2));
                    }
                }
            }
        }

        if let [access, writeback, ..] = ops {
            if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                if let Some((rt, rt2, addr, width, load)) = Self::pair_access_parts(&access.kind)? {
                    if Self::direct_addr_reg(addr) == Some(base)
                        && !Self::transfer_reg_aliases_base(rt, base)
                        && !Self::transfer_reg_aliases_base(rt2, base)
                        && Self::pair_scaled_imm(width, offset)?.is_some()
                    {
                        self.lower_pair_indexed_access(rt, rt2, base, width, load, offset, 0b01)?;
                        return Ok(Some(2));
                    }
                }
            }
        }

        Ok(None)
    }


    pub(crate) fn try_lower_fused_ldpsw_pair(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        if let [writeback, first, second, ..] = ops {
            if writeback.guest_pc == first.guest_pc {
                if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                    if let Some((rt, rt2, addr)) = Self::lifted_ldpsw_pair_parts(first, second)? {
                        if Self::direct_addr_reg(addr) == Some(base)
                            && !Self::transfer_reg_aliases_base(rt, base)
                            && !Self::transfer_reg_aliases_base(rt2, base)
                            && Self::ldpsw_scaled_imm(offset).is_some()
                        {
                            self.lower_ldpsw_pair_access(rt, rt2, base, offset, 0b11)?;
                            return Ok(Some(3));
                        }
                    }
                }
            }
        }

        if let [first, second, writeback, ..] = ops {
            if writeback.guest_pc == first.guest_pc {
                if let Some((base, offset)) = Self::writeback_add_parts(&writeback.kind) {
                    if let Some((rt, rt2, addr)) = Self::lifted_ldpsw_pair_parts(first, second)? {
                        if Self::direct_addr_reg(addr) == Some(base)
                            && !Self::transfer_reg_aliases_base(rt, base)
                            && !Self::transfer_reg_aliases_base(rt2, base)
                            && Self::ldpsw_scaled_imm(offset).is_some()
                        {
                            self.lower_ldpsw_pair_access(rt, rt2, base, offset, 0b01)?;
                            return Ok(Some(3));
                        }
                    }
                }
            }
        }

        if let [first, second, ..] = ops {
            if let Some((rt, rt2, addr)) = Self::lifted_ldpsw_pair_parts(first, second)? {
                if let Some((base, offset)) = Self::addr_base_offset(addr) {
                    self.lower_ldpsw_pair_access(rt, rt2, base, offset, 0b10)?;
                    return Ok(Some(2));
                }
            }
        }

        Ok(None)
    }


    pub(crate) fn try_lower_fused_extract(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        let [lo_op, hi_op, or_op, ..] = ops else {
            return Ok(None);
        };
        if lo_op.guest_pc != hi_op.guest_pc || lo_op.guest_pc != or_op.guest_pc {
            return Ok(None);
        }

        let (
            OpKind::Shr {
                dst: lo,
                src: rm,
                amount: lo_amount,
                width,
                flags: lo_flags,
            },
            OpKind::Shl {
                dst: hi,
                src: rn,
                amount: hi_amount,
                width: hi_width,
                flags: hi_flags,
            },
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: or_width,
                flags: or_flags,
            },
        ) = (&lo_op.kind, &hi_op.kind, &or_op.kind)
        else {
            return Ok(None);
        };

        if width != hi_width
            || width != or_width
            || lo_flags.updates_any()
            || hi_flags.updates_any()
            || or_flags.updates_any()
            || *src1 != *lo
            || *src2 != *hi
        {
            return Ok(None);
        }

        let bits = i64::from(width.bits());
        let (Some(lo_amount), Some(hi_amount)) =
            (Self::src_imm(lo_amount), Self::src_imm(hi_amount))
        else {
            return Ok(None);
        };
        let lo_amount = (lo_amount as u64 & 0x3f) as i64;
        let hi_amount = (hi_amount as u64 & 0x3f) as i64;
        if !(1..bits).contains(&lo_amount) || hi_amount != bits - lo_amount {
            return Ok(None);
        }

        self.emit_extract(
            Self::dst_gpr_arm_or_x86(*dst)?,
            Self::gpr_arm_or_x86(*rn)?,
            Self::gpr_arm_or_x86(*rm)?,
            lo_amount as u32,
            *width,
        )?;
        Ok(Some(3))
    }


    pub(crate) fn try_lower_fused_rev16(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        let [lo_op, hi_op, lo_shift_op, hi_shift_op, or_op, ..] = ops else {
            return Ok(None);
        };
        if lo_op.guest_pc != hi_op.guest_pc
            || lo_op.guest_pc != lo_shift_op.guest_pc
            || lo_op.guest_pc != hi_shift_op.guest_pc
            || lo_op.guest_pc != or_op.guest_pc
        {
            return Ok(None);
        }

        let (
            OpKind::And {
                dst: lo,
                src1,
                src2: lo_mask,
                width,
                flags: lo_flags,
            },
            OpKind::And {
                dst: hi,
                src1: hi_src,
                src2: hi_mask,
                width: hi_width,
                flags: hi_flags,
            },
            OpKind::Shl {
                dst: lo_shifted,
                src: lo_shift_src,
                amount: lo_amount,
                width: lo_shift_width,
                flags: lo_shift_flags,
            },
            OpKind::Shr {
                dst: hi_shifted,
                src: hi_shift_src,
                amount: hi_amount,
                width: hi_shift_width,
                flags: hi_shift_flags,
            },
            OpKind::Or {
                dst,
                src1: or_src1,
                src2: SrcOperand::Reg(or_src2),
                width: or_width,
                flags: or_flags,
            },
        ) = (
            &lo_op.kind,
            &hi_op.kind,
            &lo_shift_op.kind,
            &hi_shift_op.kind,
            &or_op.kind,
        )
        else {
            return Ok(None);
        };

        let Some((expected_lo_mask, expected_hi_mask)) = Self::rev16_masks(*width) else {
            return Ok(None);
        };
        if width != hi_width
            || width != lo_shift_width
            || width != hi_shift_width
            || width != or_width
            || lo_flags.updates_any()
            || hi_flags.updates_any()
            || lo_shift_flags.updates_any()
            || hi_shift_flags.updates_any()
            || or_flags.updates_any()
            || hi_src != src1
            || !Self::src_masked_imm_eq(lo_mask, expected_lo_mask, *width)
            || !Self::src_masked_imm_eq(hi_mask, expected_hi_mask, *width)
            || lo_shift_src != lo
            || hi_shift_src != hi
            || !Self::src_shift_count_eq(lo_amount, 8)
            || !Self::src_shift_count_eq(hi_amount, 8)
            || or_src1 != lo_shifted
            || or_src2 != hi_shifted
        {
            return Ok(None);
        }

        self.lower_rev16(*dst, *src1, *width)?;
        Ok(Some(5))
    }


    pub(crate) fn try_lower_fused_rev32(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        let [lo_rev_op, hi_op, hi_rev_op, hi_shift_op, or_op, ..] = ops else {
            return Ok(None);
        };
        if lo_rev_op.guest_pc != hi_op.guest_pc
            || lo_rev_op.guest_pc != hi_rev_op.guest_pc
            || lo_rev_op.guest_pc != hi_shift_op.guest_pc
            || lo_rev_op.guest_pc != or_op.guest_pc
        {
            return Ok(None);
        }

        let (
            OpKind::Bswap {
                dst: lo_rev,
                src,
                width: OpWidth::W32,
            },
            OpKind::Shr {
                dst: hi,
                src: hi_src,
                amount,
                width: OpWidth::W64,
                flags: hi_flags,
            },
            OpKind::Bswap {
                dst: hi_rev,
                src: hi_rev_src,
                width: OpWidth::W32,
            },
            OpKind::Shl {
                dst: hi_shifted,
                src: hi_shift_src,
                amount: hi_shift_amount,
                width: OpWidth::W64,
                flags: hi_shift_flags,
            },
            OpKind::Or {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W64,
                flags: or_flags,
            },
        ) = (
            &lo_rev_op.kind,
            &hi_op.kind,
            &hi_rev_op.kind,
            &hi_shift_op.kind,
            &or_op.kind,
        )
        else {
            return Ok(None);
        };

        if hi_flags.updates_any()
            || hi_shift_flags.updates_any()
            || or_flags.updates_any()
            || hi_src != src
            || !Self::src_shift_count_eq(amount, 32)
            || hi_rev_src != hi
            || hi_shift_src != hi_rev
            || !Self::src_shift_count_eq(hi_shift_amount, 32)
            || src1 != hi_shifted
            || src2 != lo_rev
        {
            return Ok(None);
        }

        self.lower_rev32(*dst, *src, OpWidth::W64)?;
        Ok(Some(5))
    }


    pub(crate) fn try_lower_fused_bitfield_insert_zero(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let [bfx_op, shl_op, ..] = ops else {
            return Ok(None);
        };
        if bfx_op.guest_pc != shl_op.guest_pc {
            return Ok(None);
        }

        let (
            OpKind::Bfx {
                dst: extracted,
                src,
                lsb: 0,
                width_bits,
                sign_extend,
                op_width,
            },
            OpKind::Shl {
                dst,
                src: shl_src,
                amount,
                width,
                flags,
            },
        ) = (&bfx_op.kind, &shl_op.kind)
        else {
            return Ok(None);
        };

        let Some(amount) = Self::src_imm(amount) else {
            return Ok(None);
        };
        let amount = (amount as u64 & 0x3f) as i64;
        let bits = i64::from(op_width.bits());
        if flags.updates_any()
            || shl_src != extracted
            // The Bfx result must be a dead virtual scratch: fusing emits only the
            // final SBFIZ/UBFIZ and never writes `extracted`. If it is an
            // architectural register (a real guest Bfx), dropping its write would
            // leave that register stale. (#11)
            || !matches!(extracted, VReg::Virtual(_))
            || width != op_width
            || !(1..bits).contains(&amount)
            || i64::from(*width_bits) + amount > bits
        {
            return Ok(None);
        }

        self.lower_bitfield_insert_zero(
            *dst,
            *src,
            amount as u8,
            *width_bits,
            *sign_extend,
            *op_width,
        )?;
        Ok(Some(2))
    }


    pub(crate) fn try_lower_fused_bitfield_insert_low(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let [bfx_op, bfi_op, ..] = ops else {
            return Ok(None);
        };
        if bfx_op.guest_pc != bfi_op.guest_pc {
            return Ok(None);
        }

        let (
            OpKind::Bfx {
                dst: extracted,
                src,
                lsb,
                width_bits,
                sign_extend: false,
                op_width,
            },
            OpKind::Bfi {
                dst,
                dst_in,
                src: bfi_src,
                lsb: 0,
                width_bits: bfi_width_bits,
                op_width: bfi_width,
            },
        ) = (&bfx_op.kind, &bfi_op.kind)
        else {
            return Ok(None);
        };

        if bfi_src != extracted
            // The Bfx result must be a dead virtual scratch: fusing to a single
            // BFXIL never writes `extracted`, so an architectural register there
            // (a real guest Bfx) would be left stale. (#12)
            || !matches!(extracted, VReg::Virtual(_))
            || width_bits != bfi_width_bits
            || op_width != bfi_width
        {
            return Ok(None);
        }

        self.lower_bitfield_insert_low(*dst, *dst_in, *src, *lsb, *width_bits, *op_width)?;
        Ok(Some(2))
    }


    pub(crate) fn try_lower_fused_mem_reg_offset(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        if let [
            extend,
            SmirOp {
                kind:
                    OpKind::Shl {
                        dst: shifted,
                        src: shift_src,
                        amount,
                        width: OpWidth::W64,
                        flags: shift_flags,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Add {
                        dst: addr_tmp,
                        src1: base,
                        src2,
                        width: OpWidth::W64,
                        flags: add_flags,
                    },
                ..
            },
            ..,
        ] = ops
        {
            if !shift_flags.updates_any()
                && !add_flags.updates_any()
                && Self::src_reg_eq(src2, *shifted)
            {
                if let Some((extended, index, option)) = Self::mem_extend_parts(&extend.kind) {
                    if shift_src == &extended {
                        if let Some((rt, addr, size, opc, access_consumed)) =
                            Self::mem_access_sequence_parts(&ops[3..])?
                        {
                            if Self::direct_addr_reg(addr) == Some(*addr_tmp)
                                // Intermediates must be dead virtual scratch: a real
                                // guest extend/shift/add writing architectural regs
                                // must not be fused away (its writes would vanish). (#9)
                                && matches!(addr_tmp, VReg::Virtual(_))
                                && matches!(shifted, VReg::Virtual(_))
                                && matches!(extended, VReg::Virtual(_))
                            {
                                if let Some(s) = Self::mem_shift_bit(amount, size) {
                                    self.lower_mem_reg_offset_access(
                                        rt, *base, index, size, opc, option, s,
                                    )?;
                                    return Ok(Some(3 + access_consumed));
                                }
                            }
                        }
                    }
                }
            }
        }

        if let [
            SmirOp {
                kind:
                    OpKind::Shl {
                        dst: shifted,
                        src: index,
                        amount,
                        width: OpWidth::W64,
                        flags: shift_flags,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Add {
                        dst: addr_tmp,
                        src1: base,
                        src2,
                        width: OpWidth::W64,
                        flags: add_flags,
                    },
                ..
            },
            ..,
        ] = ops
        {
            if !shift_flags.updates_any()
                && !add_flags.updates_any()
                && Self::src_reg_eq(src2, *shifted)
            {
                if let Some((rt, addr, size, opc, access_consumed)) =
                    Self::mem_access_sequence_parts(&ops[2..])?
                {
                    if Self::direct_addr_reg(addr) == Some(*addr_tmp)
                        // Intermediates must be dead virtual scratch (see #9).
                        && matches!(addr_tmp, VReg::Virtual(_))
                        && matches!(shifted, VReg::Virtual(_))
                    {
                        if let Some(s) = Self::mem_shift_bit(amount, size) {
                            self.lower_mem_reg_offset_access(
                                rt, *base, *index, size, opc, 0b011, s,
                            )?;
                            return Ok(Some(2 + access_consumed));
                        }
                    }
                }
            }
        }

        if let [
            extend,
            SmirOp {
                kind:
                    OpKind::Add {
                        dst: addr_tmp,
                        src1: base,
                        src2,
                        width: OpWidth::W64,
                        flags,
                    },
                ..
            },
            ..,
        ] = ops
        {
            if !flags.updates_any() {
                if let Some((extended, index, option)) = Self::mem_extend_parts(&extend.kind) {
                    if Self::src_reg_eq(src2, extended) {
                        if let Some((rt, addr, size, opc, access_consumed)) =
                            Self::mem_access_sequence_parts(&ops[2..])?
                        {
                            if Self::direct_addr_reg(addr) == Some(*addr_tmp)
                                // Intermediates must be dead virtual scratch (see #9).
                                && matches!(addr_tmp, VReg::Virtual(_))
                                && matches!(extended, VReg::Virtual(_))
                            {
                                self.lower_mem_reg_offset_access(
                                    rt, *base, index, size, opc, option, 0,
                                )?;
                                return Ok(Some(2 + access_consumed));
                            }
                        }
                    }
                }
            }
        }

        if let [
            SmirOp {
                kind:
                    OpKind::Add {
                        dst: addr_tmp,
                        src1: base,
                        src2,
                        width: OpWidth::W64,
                        flags,
                    },
                ..
            },
            ..,
        ] = ops
        {
            if !flags.updates_any() {
                if let SrcOperand::Reg(index) = src2 {
                    if let Some((rt, addr, size, opc, access_consumed)) =
                        Self::mem_access_sequence_parts(&ops[1..])?
                    {
                        if Self::direct_addr_reg(addr) == Some(*addr_tmp)
                            // The Add result must be a dead virtual scratch: a real
                            // guest `add xN, ...; ldr/str [xN]` must not drop the
                            // architectural Add write. (#9)
                            && matches!(addr_tmp, VReg::Virtual(_))
                        {
                            self.lower_mem_reg_offset_access(
                                rt, *base, *index, size, opc, 0b011, 0,
                            )?;
                            return Ok(Some(1 + access_consumed));
                        }
                    }
                }
            }
        }

        Ok(None)
    }


    pub(crate) fn try_lower_fused_ldclr(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        let [
            SmirOp {
                guest_pc,
                kind:
                    OpKind::Not {
                        dst: inverted,
                        src,
                        width: not_width,
                    },
                ..
            },
            SmirOp {
                guest_pc: atomic_pc,
                kind:
                    OpKind::AtomicRmw {
                        dst,
                        addr,
                        src: atomic_src,
                        op: AtomicOp::And,
                        width,
                        order,
                    },
                ..
            },
            ..,
        ] = ops
        else {
            return Ok(None);
        };

        if guest_pc != atomic_pc
            || atomic_src != inverted
            || !matches!(inverted, VReg::Virtual(_))
            || *not_width != OpWidth::W64
        {
            return Ok(None);
        }

        self.lower_ldclr(*dst, addr, *src, *width, *order)?;
        Ok(Some(2))
    }


    pub(crate) fn try_lower_fused_inverted_shifted_logic(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let [
            SmirOp {
                kind:
                    OpKind::Mov {
                        dst: shifted,
                        src: shifted_src @ SrcOperand::Shifted { .. },
                        width: mov_width,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Not {
                        dst: inverted,
                        src: not_src,
                        width: not_width,
                    },
                ..
            },
            ..,
        ] = ops
        else {
            return Ok(None);
        };

        if not_src != shifted
            || !matches!(shifted, VReg::Virtual(_))
            || mov_width != not_width
            || !matches!(mov_width, OpWidth::W32 | OpWidth::W64)
        {
            return Ok(None);
        }

        let (rm, shift, amount) = Self::logical_src2(shifted_src, *mov_width)?;

        if let Some(op) = ops.get(2) {
            let Some((dst, src1, src2, width, flags, opc)) = (match &op.kind {
                OpKind::Or {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                } => Some((dst, src1, src2, width, flags, 0b01)),
                OpKind::Xor {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                } => Some((dst, src1, src2, width, flags, 0b10)),
                _ => None,
            }) else {
                let dst = Self::dst_gpr_arm_or_x86(*inverted)?;
                self.emit_logic_shifted(dst, 31, rm, 0b01, true, shift, amount, *mov_width)?;
                return Ok(Some(2));
            };

            if !flags.updates_any()
                && width == mov_width
                && matches!(src2, SrcOperand::Reg(reg) if reg == inverted)
            {
                let dst = Self::dst_gpr_arm_or_x86(*dst)?;
                let rn = Self::gpr_arm_or_x86(*src1)?;
                self.emit_logic_shifted(dst, rn, rm, opc, true, shift, amount, *mov_width)?;
                return Ok(Some(3));
            }
        }

        let dst = Self::dst_gpr_arm_or_x86(*inverted)?;
        self.emit_logic_shifted(dst, 31, rm, 0b01, true, shift, amount, *mov_width)?;
        Ok(Some(2))
    }


    pub(crate) fn try_lower_fused_inverted_reg_logic(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let [
            SmirOp {
                kind:
                    OpKind::Not {
                        dst: inverted,
                        src,
                        width: not_width,
                    },
                ..
            },
            next,
            ..,
        ] = ops
        else {
            return Ok(None);
        };

        if !matches!(inverted, VReg::Virtual(_))
            || !matches!(not_width, OpWidth::W32 | OpWidth::W64)
        {
            return Ok(None);
        }

        let Some((dst, src1, src2, width, flags, opc)) = (match &next.kind {
            OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } => Some((dst, src1, src2, width, flags, 0b01)),
            OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            } => Some((dst, src1, src2, width, flags, 0b10)),
            _ => None,
        }) else {
            return Ok(None);
        };

        if flags.updates_any()
            || width != not_width
            || !matches!(src2, SrcOperand::Reg(reg) if reg == inverted)
        {
            return Ok(None);
        }

        let dst = Self::dst_gpr_arm_or_x86(*dst)?;
        let rn = Self::gpr_arm_or_x86(*src1)?;
        let rm = Self::gpr_arm_or_x86(*src)?;
        self.emit_logic_shifted(dst, rn, rm, opc, true, 0, 0, *not_width)?;
        Ok(Some(2))
    }


    pub(crate) fn try_lower_fused_sysreg_access(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let [
            SmirOp {
                kind:
                    OpKind::And {
                        dst: masked,
                        src1,
                        src2,
                        width,
                        flags,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Mov {
                        dst,
                        src: SrcOperand::Reg(mov_src),
                        width: mov_width,
                    },
                ..
            },
            ..,
        ] = ops
        else {
            return Ok(None);
        };

        if flags.updates_any() || mov_src != masked {
            return Ok(None);
        }

        if let Some(reg) = Self::sysreg_vreg(*src1) {
            let Some(info) = Self::sysreg_info(reg) else {
                return Ok(None);
            };
            if *width != info.read_width
                || *mov_width != OpWidth::W64
                || !Self::src_masked_imm_eq(src2, info.mask, info.read_width)
            {
                return Ok(None);
            }
            self.emit_sysreg(Self::dst_gpr(*dst)?, reg, true)?;
            return Ok(Some(2));
        }

        let Some(reg) = Self::sysreg_vreg(*dst) else {
            return Ok(None);
        };
        let Some(info) = Self::sysreg_info(reg) else {
            return Ok(None);
        };
        if *width != OpWidth::W64
            || *mov_width != info.write_width
            || !Self::src_masked_imm_eq(src2, info.mask, info.write_width)
        {
            return Ok(None);
        }
        self.emit_sysreg(Self::gpr(*src1)?, reg, false)?;
        Ok(Some(2))
    }


    pub(crate) fn try_lower_fused_select(&mut self, ops: &[SmirOp]) -> Result<Option<usize>, LowerError> {
        let Some(SmirOp {
            kind:
                OpKind::TestCondition {
                    dst: cond_vreg,
                    cond,
                },
            ..
        }) = ops.first()
        else {
            return Ok(None);
        };
        let Some(next) = ops.get(1) else {
            return Ok(None);
        };

        if let OpKind::Select {
            dst,
            cond: select_cond,
            src_true,
            src_false,
            width,
        } = &next.kind
        {
            if select_cond == cond_vreg {
                self.lower_fused_select(
                    *dst,
                    *cond,
                    *src_true,
                    *src_false,
                    CondSelectFalseOp::Identity,
                    *width,
                )?;
                return Ok(Some(2));
            }
        }

        let Some(select) = ops.get(2) else {
            return Ok(None);
        };
        let OpKind::Select {
            dst,
            cond: select_cond,
            src_true,
            src_false,
            width,
        } = &select.kind
        else {
            return Ok(None);
        };
        if select_cond != cond_vreg {
            return Ok(None);
        }

        if let Some((false_tmp, false_base, false_op, op_width)) =
            Self::cond_select_false_transform(&next.kind)
        {
            if src_false == &false_tmp && width == &op_width {
                self.lower_fused_select(*dst, *cond, *src_true, false_base, false_op, *width)?;
                return Ok(Some(3));
            }
        }

        if let Some((true_tmp, true_base, true_op, op_width)) =
            Self::cond_select_false_transform(&next.kind)
        {
            if src_true == &true_tmp && width == &op_width {
                let cond = match Self::inverted_arm_cond_code(*cond) {
                    Ok(cond) => cond,
                    Err(_) => return Ok(None),
                };
                self.lower_fused_select_cond(*dst, cond, *src_false, true_base, true_op, *width)?;
                return Ok(Some(3));
            }
        }

        Ok(None)
    }


    pub(crate) fn try_lower_fused_cond_compare(
        &mut self,
        ops: &[SmirOp],
    ) -> Result<Option<usize>, LowerError> {
        let [
            SmirOp {
                kind:
                    OpKind::TestCondition {
                        dst: cond_vreg,
                        cond,
                    },
                ..
            },
            cmp_op,
            SmirOp {
                kind:
                    OpKind::Mov {
                        dst: cmp_nzcv,
                        src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::Nzcv))),
                        width: OpWidth::W32,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Select {
                        dst: final_nzcv,
                        cond: select_cond,
                        src_true,
                        src_false,
                        width: OpWidth::W32,
                    },
                ..
            },
            SmirOp {
                kind:
                    OpKind::Mov {
                        dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
                        src: SrcOperand::Reg(writeback_nzcv),
                        width: OpWidth::W32,
                    },
                ..
            },
            ..,
        ] = ops
        else {
            return Ok(None);
        };

        if select_cond != cond_vreg || writeback_nzcv != final_nzcv {
            return Ok(None);
        }

        let Some((discarded_dst, rn, src2, subtract, width)) =
            Self::cond_compare_op_args(&cmp_op.kind)
        else {
            return Ok(None);
        };
        if !matches!(discarded_dst, VReg::Virtual(_)) {
            return Ok(None);
        }

        let src2 = Self::cond_compare_src2(src2)?;
        let (fallback_nzcv, cond) = if src_true == cmp_nzcv {
            let VReg::Imm(fallback_nzcv) = src_false else {
                return Ok(None);
            };
            (*fallback_nzcv, Self::arm_cond_code(*cond)?)
        } else if src_false == cmp_nzcv {
            let VReg::Imm(fallback_nzcv) = src_true else {
                return Ok(None);
            };
            let cond = match Self::inverted_arm_cond_code(*cond) {
                Ok(cond) => cond,
                Err(_) => return Ok(None),
            };
            (*fallback_nzcv, cond)
        } else {
            return Ok(None);
        };
        let nzcv = Self::cond_compare_nzcv(fallback_nzcv)?;
        let rn = Self::gpr_arm_or_x86(rn)?;
        match src2 {
            CondCompareSource::Encoded { rm_imm5, immediate } => {
                self.emit_cond_compare(rn, rm_imm5, cond, nzcv, subtract, immediate, width)?;
            }
            CondCompareSource::Immediate(imm) => {
                let scratches = Self::scratch_regs(&[rn], 1)?;
                let rm = scratches[0];
                self.emit_scratch_save(&scratches);
                self.emit_mov_imm(rm, imm, width)?;
                self.emit_cond_compare(rn, rm, cond, nzcv, subtract, false, width)?;
                self.emit_scratch_restore(&scratches);
            }
        }
        Ok(Some(5))
    }
}
