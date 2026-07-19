//! Shift and rotate lowering

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
    pub(crate) fn emit_extract(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        lsb: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b100111 << 23)
                | (sf << 22)
                | ((rm as u32) << 16)
                | (lsb << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn mem_shift_bit(amount: &SrcOperand, size: u32) -> Option<u32> {
        if Self::src_imm_eq(amount, 0) {
            Some(0)
        } else if size != 0 && Self::src_imm_eq(amount, i64::from(size)) {
            Some(1)
        } else {
            None
        }
    }

    pub(crate) fn lea_scale_shift(scale: u8) -> Result<u32, LowerError> {
        match scale {
            1 => Ok(0),
            2 => Ok(1),
            4 => Ok(2),
            8 => Ok(3),
            _ => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native LEA scale {scale}"),
            }),
        }
    }

    pub(crate) fn emit_prepare_rotate_carry_value(
        &mut self,
        dst: u8,
        src: u8,
        width: OpWidth,
    ) -> Result<OpWidth, LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                let top_bit = width.bits() - 1;
                self.emit_bitfield(dst, src, 0b10, 0, top_bit, OpWidth::W32)?;
                Ok(OpWidth::W32)
            }
            OpWidth::W32 => {
                self.emit_mov_reg(dst, src, OpWidth::W32)?;
                Ok(OpWidth::W32)
            }
            OpWidth::W64 => {
                self.emit_mov_reg(dst, src, OpWidth::W64)?;
                Ok(OpWidth::W64)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native RCL/RCR width {other:?}"),
            }),
        }
    }

    pub(crate) fn emit_finish_rotate_carry_value(
        &mut self,
        dst: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                self.emit_bitfield(dst, dst, 0b10, 0, width.bits() - 1, OpWidth::W32)
            }
            OpWidth::W32 | OpWidth::W64 => Ok(()),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native RCL/RCR width {other:?}"),
            }),
        }
    }

    pub(crate) fn emit_rotate_carry_step(
        &mut self,
        dst: u8,
        flags_base: u8,
        carry: u8,
        width: OpWidth,
        emit_width: OpWidth,
        right: bool,
    ) -> Result<(), LowerError> {
        let top_bit = width.bits() - 1;
        if right {
            self.emit_bfxil_bit_to_low(flags_base, dst, 0, emit_width)?;
            self.emit_extract(dst, 31, dst, 1, emit_width)?;
            self.emit_logic_shifted(dst, dst, carry, 0b01, false, 0, top_bit, emit_width)?;
            self.emit_ubfx_bit_to_low(carry, flags_base, 0, OpWidth::W32)
        } else {
            self.emit_restore_c_from_low_bit(flags_base, carry)?;
            self.emit_bfxil_bit_to_low(flags_base, dst, top_bit, emit_width)?;
            self.emit_addsub_carry(dst, dst, dst, false, true, emit_width)?;
            self.emit_finish_rotate_carry_value(dst, width)?;
            self.emit_ubfx_bit_to_low(carry, flags_base, 0, OpWidth::W32)
        }
    }

    pub(crate) fn emit_finalize_rotate_carry_flags(
        &mut self,
        dst: u8,
        flags_base: u8,
        carry: u8,
        width: OpWidth,
        emit_width: OpWidth,
        effective_one: bool,
        right: bool,
    ) -> Result<(), LowerError> {
        self.emit_logic_shifted(
            flags_base,
            flags_base,
            flags_base,
            0b01,
            false,
            0,
            29,
            OpWidth::W32,
        )?;

        if effective_one {
            let top_bit = width.bits() - 1;
            if right {
                self.emit_logic_shifted(carry, dst, dst, 0b10, false, 0, 1, emit_width)?;
                self.emit_ubfx_bit_to_low(carry, carry, top_bit, emit_width)?;
            } else {
                self.emit_logic_shifted(carry, carry, dst, 0b10, false, 1, top_bit, emit_width)?;
                self.emit_ubfx_bit_to_low(carry, carry, 0, OpWidth::W32)?;
            }
            self.emit_logic_shifted(
                flags_base,
                flags_base,
                carry,
                0b01,
                false,
                0,
                28,
                OpWidth::W32,
            )?;
        }

        self.emit_sysreg(flags_base, ArmReg::Nzcv, false)
    }

    pub(crate) fn lower_rotate_carry(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        right: bool,
    ) -> Result<(), LowerError> {
        if let Some((dst_reg, result)) =
            Self::x86_partial_write_scratch(dst, width, &[src], &[amount])?
        {
            let scratches = [result];
            self.emit_scratch_save(&scratches);
            self.lower_rotate_carry(Self::arm_x_reg(result), src, amount, width, flags, right)?;
            self.emit_bitfield(dst_reg, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let src_reg = Self::gpr_arm_or_x86(src)?;
        let amount_reg = match amount {
            SrcOperand::Reg(reg) => Some(Self::gpr_arm_or_x86(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native RCL/RCR amount {other:?}"),
                });
            }
        };
        let bits = match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64 => width.bits(),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native RCL/RCR width {other:?}"),
                });
            }
        };

        let cmask = if width == OpWidth::W64 { 0x3f } else { 0x1f };
        if let SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) = amount {
            let effective = ((*imm as u64) & cmask) % (u64::from(bits) + 1);
            if effective == 0 {
                self.emit_prepare_rotate_carry_value(dst_reg, src_reg, width)?;
                return Ok(());
            }

            let scratches = Self::scratch_regs(&[dst_reg, src_reg], 3)?;
            let saved_flags = scratches[0];
            let flags_base = scratches[1];
            let carry = scratches[2];
            self.emit_scratch_save(&scratches);

            self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
            self.emit_keep_nz_flags(flags_base, saved_flags)?;
            self.emit_ubfx_bit_to_low(carry, saved_flags, 29, OpWidth::W32)?;
            let emit_width = self.emit_prepare_rotate_carry_value(dst_reg, src_reg, width)?;
            for _ in 0..effective {
                self.emit_rotate_carry_step(dst_reg, flags_base, carry, width, emit_width, right)?;
            }

            if flags.updates_any() {
                self.emit_finalize_rotate_carry_flags(
                    dst_reg,
                    flags_base,
                    carry,
                    width,
                    emit_width,
                    effective == 1,
                    right,
                )?;
            } else {
                self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
            }

            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        let amount_reg = amount_reg.unwrap();
        let scratches = Self::scratch_regs(&[dst_reg, src_reg, amount_reg], 4)?;
        let saved_flags = scratches[0];
        let flags_base = scratches[1];
        let carry = scratches[2];
        let count = scratches[3];
        self.emit_scratch_save(&scratches);

        self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
        self.emit_keep_nz_flags(flags_base, saved_flags)?;
        self.emit_ubfx_bit_to_low(carry, saved_flags, 29, OpWidth::W32)?;
        self.emit_normalize_rcl_rcr_count(count, amount_reg, width)?;
        let emit_width = self.emit_prepare_rotate_carry_value(dst_reg, src_reg, width)?;

        let zero_count = self.code.position();
        self.emit(0xb400_0000 | (count as u32));

        self.emit_addsub_imm(31, count, 1, true, true, OpWidth::W64)?;
        let not_one_count = self.code.position();
        self.emit(0x5400_0000 | Self::inverted_arm_cond_code(Condition::Eq)?);
        self.emit_orr_imm_one(saved_flags, saved_flags, OpWidth::W32)?;
        self.patch_cond_branch_to_current(
            not_one_count,
            Self::inverted_arm_cond_code(Condition::Eq)?,
        )?;

        let loop_start = self.code.position();
        self.emit_rotate_carry_step(dst_reg, flags_base, carry, width, emit_width, right)?;
        self.emit_addsub_imm(count, count, 1, true, false, OpWidth::W64)?;
        self.emit_compare_branch_to_offset(count, true, loop_start)?;

        if flags.updates_any() {
            let not_one = self.code.position();
            self.emit_test_branch(saved_flags, 0, false, 0)?;
            self.emit_finalize_rotate_carry_flags(
                dst_reg, flags_base, carry, width, emit_width, true, right,
            )?;
            let final_done = self.code.position();
            self.emit(0x1400_0000);
            self.patch_test_branch_to_current(not_one, saved_flags, 0, false)?;
            self.emit_finalize_rotate_carry_flags(
                dst_reg, flags_base, carry, width, emit_width, false, right,
            )?;
            self.patch_branch_to_current(final_done)?;
        } else {
            self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
        }
        let restore_done = self.code.position();
        self.emit(0x1400_0000);

        self.patch_compare_branch_to_current(zero_count, count, false)?;
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
        self.patch_branch_to_current(restore_done)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_shift_imm(
        &mut self,
        dst: u8,
        src: u8,
        amount: i64,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            return self.lower_subword_shift_imm(dst, src, amount, shift, width);
        }

        let bits = width.bits();
        let amount = match shift {
            ShiftOp::Ror | ShiftOp::Rrx => (amount as u64 & u64::from(bits - 1)) as u32,
            ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr => (amount as u64 & 0x3f) as u32,
        };
        if width == OpWidth::W64 && amount == 0 && dst == src {
            return Ok(());
        }

        match shift {
            ShiftOp::Lsl => {
                if amount == 0 {
                    self.emit_mov_reg(dst, src, width)
                } else if amount >= bits {
                    self.emit_mov_imm(dst, 0, width)
                } else {
                    self.emit_bitfield(dst, src, 0b10, bits - amount, bits - 1 - amount, width)
                }
            }
            ShiftOp::Lsr => {
                if amount == 0 {
                    self.emit_mov_reg(dst, src, width)
                } else if amount >= bits {
                    self.emit_mov_imm(dst, 0, width)
                } else {
                    self.emit_bitfield(dst, src, 0b10, amount, bits - 1, width)
                }
            }
            ShiftOp::Asr => {
                if amount == 0 {
                    self.emit_mov_reg(dst, src, width)
                } else {
                    let amount = amount.min(bits - 1);
                    self.emit_bitfield(dst, src, 0b00, amount, bits - 1, width)
                }
            }
            ShiftOp::Ror | ShiftOp::Rrx => {
                if amount == 0 {
                    self.emit_mov_reg(dst, src, width)
                } else {
                    self.emit_extract(dst, src, src, amount, width)
                }
            }
        }
    }

    pub(crate) fn lower_shift_reg(
        &mut self,
        dst: u8,
        src: u8,
        amount: u8,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            return self.lower_subword_shift_reg(dst, src, amount, shift, width);
        }

        if width == OpWidth::W32 {
            match shift {
                ShiftOp::Lsl | ShiftOp::Lsr => {
                    let opcode2 = match shift {
                        ShiftOp::Lsl => 0b1000,
                        ShiftOp::Lsr => 0b1001,
                        _ => unreachable!(),
                    };
                    if dst == amount {
                        let oob_branch = self.code.position();
                        self.emit_test_branch(amount, 5, true, 0)?;
                        self.emit_dp2(dst, src, amount, opcode2, width)?;
                        let end_branch = self.code.position();
                        self.emit(0x1400_0000);
                        self.patch_test_branch_to_current(oob_branch, amount, 5, true)?;
                        self.emit_mov_reg(dst, 31, width)?;
                        return self.patch_branch_to_current(end_branch);
                    }
                    self.emit_dp2(dst, src, amount, opcode2, width)?;
                    self.emit_test_branch(amount, 5, false, 8)?;
                    return self.emit_mov_reg(dst, 31, width);
                }
                ShiftOp::Asr => {
                    if dst == amount {
                        let oob_branch = self.code.position();
                        self.emit_test_branch(amount, 5, true, 0)?;
                        self.emit_dp2(dst, src, amount, 0b1010, width)?;
                        let end_branch = self.code.position();
                        self.emit(0x1400_0000);
                        self.patch_test_branch_to_current(oob_branch, amount, 5, true)?;
                        self.emit_bitfield(dst, src, 0b00, 31, 31, width)?;
                        return self.patch_branch_to_current(end_branch);
                    }
                    self.emit_dp2(dst, src, amount, 0b1010, width)?;
                    self.emit_test_branch(amount, 5, false, 8)?;
                    return self.emit_bitfield(dst, dst, 0b00, 31, 31, width);
                }
                ShiftOp::Ror => {}
                ShiftOp::Rrx => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!(
                            "AArch64 native W32 variable {shift:?} count semantics differ from SMIR"
                        ),
                    });
                }
            }
        }

        let opcode2 = match shift {
            ShiftOp::Lsl => 0b1000,
            ShiftOp::Lsr => 0b1001,
            ShiftOp::Asr => 0b1010,
            ShiftOp::Ror => 0b1011,
            ShiftOp::Rrx => {
                return Err(LowerError::UnsupportedOp {
                    op: "AArch64 native RRX variable shift".into(),
                });
            }
        };
        self.emit_dp2(dst, src, amount, opcode2, width)
    }

    pub(crate) fn src_shift_count_eq(src: &SrcOperand, value: u32) -> bool {
        let Some(imm) = Self::src_imm(src) else {
            return false;
        };
        (imm as u64 & 0x3f) == u64::from(value & 0x3f)
    }

    pub(crate) fn shift_emit_width(width: OpWidth) -> Result<OpWidth, LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => Ok(OpWidth::W32),
            OpWidth::W64 => Ok(OpWidth::W64),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native flag-setting shift width {other:?}"),
            }),
        }
    }

    pub(crate) fn emit_prepare_shift_flag_source(
        &mut self,
        dst: u8,
        src: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => {
                self.emit_bitfield(dst, src, 0b00, 0, width.bits() - 1, OpWidth::W64)
            }
            OpWidth::W64 => self.emit_mov_reg(dst, src, OpWidth::W64),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native flag-setting shift width {other:?}"),
            }),
        }
    }

    pub(crate) fn emit_init_shift_nz_flags(
        &mut self,
        flags: u8,
        temp: u8,
        result: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let emit_width = Self::shift_emit_width(width)?;
        let top_bit = width.bits() - 1;

        self.emit_mov_imm(flags, 0, OpWidth::W32)?;

        let sign_clear = self.code.position();
        self.emit_test_branch(result, top_bit, false, 0)?;
        self.emit_or_nzcv_const(flags, temp, NZCV_N)?;
        self.patch_test_branch_to_current(sign_clear, result, top_bit, false)?;

        let zero_reg = if matches!(width, OpWidth::W8 | OpWidth::W16) {
            self.emit_bitfield(temp, result, 0b10, 0, top_bit, emit_width)?;
            temp
        } else {
            result
        };
        let nonzero = self.code.position();
        self.emit(0xb500_0000 | u32::from(zero_reg));
        self.emit_or_nzcv_const(flags, temp, NZCV_Z)?;
        self.patch_compare_branch_to_current(nonzero, zero_reg, true)
    }

    pub(crate) fn emit_shift_carry_imm(
        &mut self,
        flags: u8,
        temp: u8,
        original: u8,
        count: u32,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let bits = width.bits();
        let carry_bit = match shift {
            ShiftOp::Lsl => (count <= bits).then_some(bits - count),
            ShiftOp::Lsr => (count <= bits).then_some(count - 1),
            ShiftOp::Asr => Some(count - 1),
            ShiftOp::Ror | ShiftOp::Rrx => None,
        };
        let Some(carry_bit) = carry_bit else {
            return Ok(());
        };

        let no_carry = self.code.position();
        self.emit_test_branch(original, carry_bit, false, 0)?;
        self.emit_or_nzcv_const(flags, temp, NZCV_C)?;
        self.patch_test_branch_to_current(no_carry, original, carry_bit, false)
    }

    pub(crate) fn emit_shift_carry_reg(
        &mut self,
        flags: u8,
        temp: u8,
        original: u8,
        count: u8,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let bits = width.bits();
        let too_large = if !matches!(shift, ShiftOp::Asr) && bits < OpWidth::W64.bits() {
            self.emit_addsub_imm(31, count, i64::from(bits), true, true, OpWidth::W64)?;
            let offset = self.code.position();
            self.emit(0x5400_0000 | Self::arm_cond_code(Condition::Ugt)?);
            Some(offset)
        } else {
            None
        };

        match shift {
            ShiftOp::Lsl => {
                self.emit_mov_imm(temp, i64::from(bits), OpWidth::W64)?;
                self.emit_addsub_reg(temp, temp, count, true, false, OpWidth::W64)?;
            }
            ShiftOp::Lsr | ShiftOp::Asr => {
                self.emit_addsub_imm(temp, count, 1, true, false, OpWidth::W64)?;
            }
            ShiftOp::Ror | ShiftOp::Rrx => unreachable!(),
        }
        self.emit_dp2(temp, original, temp, 0b1001, OpWidth::W64)?;
        self.emit_bitfield(temp, temp, 0b10, 0, 0, OpWidth::W32)?;
        self.emit_logic_shifted(flags, flags, temp, 0b01, false, 0, 29, OpWidth::W32)?;

        if let Some(offset) = too_large {
            self.patch_cond_branch_to_current(offset, Self::arm_cond_code(Condition::Ugt)?)?;
        }
        Ok(())
    }

    pub(crate) fn emit_shift_overflow_imm(
        &mut self,
        flags: u8,
        temp: u8,
        result: u8,
        original: u8,
        count: u32,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if count != 1 {
            return Ok(());
        }

        let top_bit = width.bits() - 1;
        match shift {
            ShiftOp::Lsl => {
                self.emit_logic_shifted(temp, original, result, 0b10, false, 0, 0, OpWidth::W64)?;
                let no_overflow = self.code.position();
                self.emit_test_branch(temp, top_bit, false, 0)?;
                self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
                self.patch_test_branch_to_current(no_overflow, temp, top_bit, false)
            }
            ShiftOp::Lsr => {
                let no_overflow = self.code.position();
                self.emit_test_branch(original, top_bit, false, 0)?;
                self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
                self.patch_test_branch_to_current(no_overflow, original, top_bit, false)
            }
            ShiftOp::Asr => Ok(()),
            ShiftOp::Ror | ShiftOp::Rrx => unreachable!(),
        }
    }

    pub(crate) fn emit_shift_overflow_reg(
        &mut self,
        flags: u8,
        temp: u8,
        result: u8,
        original: u8,
        count: u8,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if matches!(shift, ShiftOp::Asr) {
            return Ok(());
        }

        self.emit_addsub_imm(31, count, 1, true, true, OpWidth::W64)?;
        let not_one = self.code.position();
        self.emit(0x5400_0000 | Self::arm_cond_code(Condition::Ne)?);

        let top_bit = width.bits() - 1;
        match shift {
            ShiftOp::Lsl => {
                self.emit_logic_shifted(temp, original, result, 0b10, false, 0, 0, OpWidth::W64)?;
                let no_overflow = self.code.position();
                self.emit_test_branch(temp, top_bit, false, 0)?;
                self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
                self.patch_test_branch_to_current(no_overflow, temp, top_bit, false)?;
            }
            ShiftOp::Lsr => {
                let no_overflow = self.code.position();
                self.emit_test_branch(original, top_bit, false, 0)?;
                self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
                self.patch_test_branch_to_current(no_overflow, original, top_bit, false)?;
            }
            ShiftOp::Asr | ShiftOp::Ror | ShiftOp::Rrx => unreachable!(),
        }

        self.patch_cond_branch_to_current(not_one, Self::arm_cond_code(Condition::Ne)?)
    }

    pub(crate) fn emit_finalize_shift_flags(
        &mut self,
        result: u8,
        original: u8,
        count_reg: Option<u8>,
        imm_count: Option<u32>,
        shift: ShiftOp,
        width: OpWidth,
        flags: u8,
        temp: u8,
    ) -> Result<(), LowerError> {
        self.emit_init_shift_nz_flags(flags, temp, result, width)?;
        if let Some(count) = imm_count {
            self.emit_shift_carry_imm(flags, temp, original, count, shift, width)?;
            self.emit_shift_overflow_imm(flags, temp, result, original, count, shift, width)?;
        } else {
            let count = count_reg.expect("register-count shift flags need a count register");
            self.emit_shift_carry_reg(flags, temp, original, count, shift, width)?;
            self.emit_shift_overflow_reg(flags, temp, result, original, count, shift, width)?;
        }
        self.emit_sysreg(flags, ArmReg::Nzcv, false)
    }

    pub(crate) fn rotate_count_mask(width: OpWidth) -> Result<u64, LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => Ok(0x1f),
            OpWidth::W64 => Ok(0x3f),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native flag-setting rotate width {other:?}"),
            }),
        }
    }

    pub(crate) fn emit_rotate_overflow_from_result(
        &mut self,
        flags: u8,
        temp: u8,
        result: u8,
        width: OpWidth,
        right: bool,
    ) -> Result<(), LowerError> {
        let emit_width = Self::shift_emit_width(width)?;
        let top_bit = width.bits() - 1;
        if right {
            self.emit_logic_shifted(temp, result, result, 0b10, false, 0, 1, emit_width)?;
            let no_overflow = self.code.position();
            self.emit_test_branch(temp, top_bit, false, 0)?;
            self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
            self.patch_test_branch_to_current(no_overflow, temp, top_bit, false)
        } else {
            self.emit_logic_shifted(temp, result, result, 0b10, false, 1, top_bit, emit_width)?;
            let no_overflow = self.code.position();
            self.emit_test_branch(temp, 0, false, 0)?;
            self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
            self.patch_test_branch_to_current(no_overflow, temp, 0, false)
        }
    }

    pub(crate) fn emit_finalize_rotate_flags(
        &mut self,
        saved_flags: u8,
        flags: u8,
        temp: u8,
        result: u8,
        count_reg: Option<u8>,
        imm_count: Option<u32>,
        width: OpWidth,
        right: bool,
    ) -> Result<(), LowerError> {
        let top_bit = width.bits() - 1;
        self.emit_keep_nz_flags(flags, saved_flags)?;

        let carry_bit = if right { top_bit } else { 0 };
        let no_carry = self.code.position();
        self.emit_test_branch(result, carry_bit, false, 0)?;
        self.emit_or_nzcv_const(flags, temp, NZCV_C)?;
        self.patch_test_branch_to_current(no_carry, result, carry_bit, false)?;

        if let Some(count) = imm_count {
            if count == 1 {
                self.emit_rotate_overflow_from_result(flags, temp, result, width, right)?;
            }
        } else {
            let count = count_reg.expect("register-count rotate flags need a count register");
            self.emit_addsub_imm(31, count, 1, true, true, OpWidth::W64)?;
            let not_one = self.code.position();
            self.emit(0x5400_0000 | Self::arm_cond_code(Condition::Ne)?);
            self.emit_rotate_overflow_from_result(flags, temp, result, width, right)?;
            self.patch_cond_branch_to_current(not_one, Self::arm_cond_code(Condition::Ne)?)?;
        }

        self.emit_sysreg(flags, ArmReg::Nzcv, false)
    }

    pub(crate) fn lower_rotate_with_flags(
        &mut self,
        dst: u8,
        src: u8,
        amount: &SrcOperand,
        width: OpWidth,
        right: bool,
    ) -> Result<(), LowerError> {
        Self::shift_emit_width(width)?;
        let mask = Self::rotate_count_mask(width)?;
        let bits = width.bits();

        match amount {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let count = (*imm as u64 & mask) as u32;
                let rotate = count % bits;
                let ror = if right {
                    rotate
                } else if rotate == 0 {
                    0
                } else {
                    bits - rotate
                };
                if count == 0 {
                    return self.lower_shift_imm(dst, src, i64::from(ror), ShiftOp::Ror, width);
                }

                let scratches = Self::scratch_regs(&[dst, src], 3)?;
                let saved_flags = scratches[0];
                let flags = scratches[1];
                let temp = scratches[2];
                self.emit_scratch_save(&scratches);
                self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
                self.lower_shift_imm(dst, src, i64::from(ror), ShiftOp::Ror, width)?;
                self.emit_finalize_rotate_flags(
                    saved_flags,
                    flags,
                    temp,
                    dst,
                    None,
                    Some(count),
                    width,
                    right,
                )?;
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            SrcOperand::Reg(reg) => {
                let amount = Self::gpr_arm_or_x86(*reg)?;
                let scratch_count = if right { 4 } else { 5 };
                let scratches = Self::scratch_regs(&[dst, src, amount], scratch_count)?;
                let saved_flags = scratches[0];
                let flags = scratches[1];
                let temp = scratches[2];
                let count = scratches[3];
                self.emit_scratch_save(&scratches);
                self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
                self.emit_mov_reg(count, amount, OpWidth::W64)?;
                let (imm_n, immr, imms) = Self::logical_bitmask_imm(mask as i64, OpWidth::W64)?;
                self.emit_logic_imm(count, count, 0b00, imm_n, immr, imms, OpWidth::W64)?;

                let zero_count = self.code.position();
                self.emit(0xb400_0000 | u32::from(count));
                let rotate_count = if right {
                    count
                } else {
                    let rotate_count = scratches[4];
                    self.emit_addsub_reg(rotate_count, 31, count, true, false, OpWidth::W64)?;
                    rotate_count
                };
                self.lower_shift_reg(dst, src, rotate_count, ShiftOp::Ror, width)?;
                self.emit_finalize_rotate_flags(
                    saved_flags,
                    flags,
                    temp,
                    dst,
                    Some(count),
                    None,
                    width,
                    right,
                )?;
                self.emit_scratch_restore(&scratches);
                let done = self.code.position();
                self.emit(0x1400_0000);

                self.patch_compare_branch_to_current(zero_count, count, false)?;
                self.lower_shift_imm(dst, src, 0, ShiftOp::Ror, width)?;
                self.emit_scratch_restore(&scratches);
                self.patch_branch_to_current(done)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native rotate amount {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_shift_with_flags(
        &mut self,
        dst: u8,
        src: u8,
        amount: &SrcOperand,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if shift == ShiftOp::Ror {
            return self.lower_rotate_with_flags(dst, src, amount, width, true);
        }
        if shift == ShiftOp::Rrx {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native flag-setting {shift:?}"),
            });
        }
        Self::shift_emit_width(width)?;

        match amount {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let count = (*imm as u64 & 0x3f) as u32;
                if count == 0 {
                    return self.lower_shift_imm(dst, src, *imm, shift, width);
                }

                let scratches = Self::scratch_regs(&[dst, src], 3)?;
                let original = scratches[0];
                let flags = scratches[1];
                let temp = scratches[2];
                self.emit_scratch_save(&scratches);
                self.emit_prepare_shift_flag_source(original, src, width)?;
                self.lower_shift_imm(dst, original, *imm, shift, width)?;
                self.emit_finalize_shift_flags(
                    dst,
                    original,
                    None,
                    Some(count),
                    shift,
                    width,
                    flags,
                    temp,
                )?;
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            SrcOperand::Reg(reg) => {
                let amount = Self::gpr_arm_or_x86(*reg)?;
                let scratches = Self::scratch_regs(&[dst, src, amount], 4)?;
                let original = scratches[0];
                let count = scratches[1];
                let flags = scratches[2];
                let temp = scratches[3];
                self.emit_scratch_save(&scratches);
                self.emit_prepare_shift_flag_source(original, src, width)?;
                self.emit_mov_reg(count, amount, OpWidth::W64)?;
                let (imm_n, immr, imms) = Self::logical_bitmask_imm(0x3f, OpWidth::W64)?;
                self.emit_logic_imm(count, count, 0b00, imm_n, immr, imms, OpWidth::W64)?;

                let zero_count = self.code.position();
                self.emit(0xb400_0000 | u32::from(count));
                self.lower_shift_reg(dst, original, count, shift, width)?;
                self.emit_finalize_shift_flags(
                    dst,
                    original,
                    Some(count),
                    None,
                    shift,
                    width,
                    flags,
                    temp,
                )?;
                self.emit_scratch_restore(&scratches);
                let done = self.code.position();
                self.emit(0x1400_0000);

                self.patch_compare_branch_to_current(zero_count, count, false)?;
                self.lower_shift_imm(dst, original, 0, shift, width)?;
                self.emit_scratch_restore(&scratches);
                self.patch_branch_to_current(done)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native shift amount {other:?}"),
            }),
        }
    }

    /// Emit the exact AArch32 W32 register-controlled shift result. The caller
    /// has already truncated `count` to the architectural low eight bits.
    pub(crate) fn emit_arm_w32_reg_shift_result(
        &mut self,
        result: u8,
        original: u8,
        count: u8,
        shift: ShiftOp,
    ) -> Result<(), LowerError> {
        // A64 variable shifts mask their count modulo 32. Branch around that
        // behavior for AArch32's saturating logical/arithmetic cases; ROR is
        // intentionally modulo 32 for every nonzero low-byte count.
        match shift {
            ShiftOp::Lsl | ShiftOp::Lsr => {
                self.emit_addsub_imm(31, count, 32, true, true, OpWidth::W32)?;
                let large = self.code.position();
                let uge = Self::arm_cond_code(Condition::Uge)?;
                self.emit(0x5400_0000 | uge);
                self.lower_shift_reg(result, original, count, shift, OpWidth::W32)?;
                let done = self.code.position();
                self.emit(0x1400_0000);
                self.patch_cond_branch_to_current(large, uge)?;
                self.emit_mov_imm(result, 0, OpWidth::W32)?;
                self.patch_branch_to_current(done)
            }
            ShiftOp::Asr => {
                self.emit_addsub_imm(31, count, 32, true, true, OpWidth::W32)?;
                let large = self.code.position();
                let uge = Self::arm_cond_code(Condition::Uge)?;
                self.emit(0x5400_0000 | uge);
                self.lower_shift_reg(result, original, count, shift, OpWidth::W32)?;
                let done = self.code.position();
                self.emit(0x1400_0000);
                self.patch_cond_branch_to_current(large, uge)?;
                self.lower_shift_imm(result, original, 31, ShiftOp::Asr, OpWidth::W32)?;
                self.patch_branch_to_current(done)
            }
            ShiftOp::Ror => {
                self.lower_shift_reg(result, original, count, ShiftOp::Ror, OpWidth::W32)
            }
            ShiftOp::Rrx => Err(LowerError::InvalidOperand {
                op: "AArch64 native AArch32 register shift".into(),
                operand: "RRX is not a register-specified shift".into(),
            }),
        }
    }

    /// Add the AArch32 register-shifter carry output to an already initialized
    /// NZCV word. Count zero passes through the incoming architectural carry.
    pub(crate) fn emit_arm_w32_reg_shift_carry(
        &mut self,
        produced: u8,
        temp: u8,
        saved_flags: u8,
        original: u8,
        count: u8,
        result: u8,
        shift: ShiftOp,
    ) -> Result<(), LowerError> {
        let nonzero = self.code.position();
        self.emit(0xb500_0000 | u32::from(count));
        let old_c_clear = self.code.position();
        self.emit_test_branch(saved_flags, 29, false, 0)?;
        self.emit_or_nzcv_const(produced, temp, NZCV_C)?;
        self.patch_test_branch_to_current(old_c_clear, saved_flags, 29, false)?;
        let carry_done = self.code.position();
        self.emit(0x1400_0000);

        self.patch_compare_branch_to_current(nonzero, count, true)?;
        match shift {
            ShiftOp::Lsl | ShiftOp::Lsr => {
                self.emit_shift_carry_reg(produced, temp, original, count, shift, OpWidth::W32)?
            }
            ShiftOp::Asr => {
                self.emit_addsub_imm(31, count, 32, true, true, OpWidth::W32)?;
                let small = self.code.position();
                let ult = Self::arm_cond_code(Condition::Ult)?;
                self.emit(0x5400_0000 | ult);
                let sign_clear = self.code.position();
                self.emit_test_branch(original, 31, false, 0)?;
                self.emit_or_nzcv_const(produced, temp, NZCV_C)?;
                self.patch_test_branch_to_current(sign_clear, original, 31, false)?;
                let asr_done = self.code.position();
                self.emit(0x1400_0000);
                self.patch_cond_branch_to_current(small, ult)?;
                self.emit_shift_carry_reg(
                    produced,
                    temp,
                    original,
                    count,
                    ShiftOp::Asr,
                    OpWidth::W32,
                )?;
                self.patch_branch_to_current(asr_done)?;
            }
            ShiftOp::Ror => {
                let carry_clear = self.code.position();
                self.emit_test_branch(result, 31, false, 0)?;
                self.emit_or_nzcv_const(produced, temp, NZCV_C)?;
                self.patch_test_branch_to_current(carry_clear, result, 31, false)?;
            }
            ShiftOp::Rrx => {
                return Err(LowerError::InvalidOperand {
                    op: "AArch64 native AArch32 register shift carry".into(),
                    operand: "RRX is not a register-specified shift".into(),
                });
            }
        }
        self.patch_branch_to_current(carry_done)
    }

    pub(crate) fn emit_arm_dp_reg_shift_result(
        &mut self,
        kind: ArmDpRegShiftKind,
        dst: u8,
        rn: u8,
        shifted: u8,
        set_flags: bool,
    ) -> Result<(), LowerError> {
        use ArmDpRegShiftKind as Kind;

        match kind {
            Kind::And | Kind::Tst => {
                self.emit_logic_shifted(dst, rn, shifted, 0b00, false, 0, 0, OpWidth::W32)
            }
            Kind::Eor | Kind::Teq => {
                self.emit_logic_shifted(dst, rn, shifted, 0b10, false, 0, 0, OpWidth::W32)
            }
            Kind::Sub | Kind::Cmp => {
                self.emit_addsub_reg(dst, rn, shifted, true, set_flags, OpWidth::W32)
            }
            Kind::Rsb => self.emit_addsub_reg(dst, shifted, rn, true, set_flags, OpWidth::W32),
            Kind::Add | Kind::Cmn => {
                self.emit_addsub_reg(dst, rn, shifted, false, set_flags, OpWidth::W32)
            }
            Kind::Adc => self.emit_addsub_carry(dst, rn, shifted, false, set_flags, OpWidth::W32),
            Kind::Sbc => self.emit_addsub_carry(dst, rn, shifted, true, set_flags, OpWidth::W32),
            Kind::Rsc => self.emit_addsub_carry(dst, shifted, rn, true, set_flags, OpWidth::W32),
            Kind::Orr => self.emit_logic_shifted(dst, rn, shifted, 0b01, false, 0, 0, OpWidth::W32),
            Kind::Mov => self.emit_logic_shifted(dst, 31, shifted, 0b01, false, 0, 0, OpWidth::W32),
            Kind::Bic => self.emit_logic_shifted(dst, rn, shifted, 0b00, true, 0, 0, OpWidth::W32),
            Kind::Mvn => self.emit_logic_shifted(dst, 31, shifted, 0b01, true, 0, 0, OpWidth::W32),
        }
    }

    /// Lower AArch32's register-controlled W32 shift contract. Unlike generic
    /// SMIR shifts, the count is `amount[7:0]`; N/Z are always produced, C is
    /// either the last bit shifted out or the incoming C for count zero, and V
    /// is preserved.
    pub(crate) fn lower_arm_reg_shift(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        shift: ShiftOp,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let nzc = FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF);
        if width != OpWidth::W32
            || (flags != FlagUpdate::None && flags != FlagUpdate::Specific(nzc))
            || !matches!(
                shift,
                ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
            )
        {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native AArch32 register shift".into(),
                operand: format!("width={width:?}, flags={flags:?}, shift={shift:?}"),
            });
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let src = Self::gpr_arm_or_x86(src)?;
        let amount_reg = match amount {
            SrcOperand::Reg(reg) => Some(Self::gpr_arm_or_x86(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            other => {
                return Err(LowerError::InvalidOperand {
                    op: "AArch64 native AArch32 register shift".into(),
                    operand: format!("count {other:?}"),
                });
            }
        };

        let mut avoid = vec![dst, src];
        if let Some(reg) = amount_reg {
            avoid.push(reg);
        }
        let scratches = Self::scratch_regs(&avoid, 5)?;
        let original = scratches[0];
        let count = scratches[1];
        let saved_flags = scratches[2];
        let produced_flags = scratches[3];
        let temp = scratches[4];
        self.emit_scratch_save(&scratches);

        // Snapshot both operands before writing the destination. This covers
        // the destructive T16 form and every legal T32 destination/value/count
        // alias, including all three architectural registers being identical.
        self.emit_mov_reg(original, src, OpWidth::W32)?;
        match amount {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                self.emit_mov_imm(count, (*imm as u64 & 0xff) as i64, OpWidth::W32)?;
            }
            SrcOperand::Reg(_) => {
                self.emit_mov_reg(count, amount_reg.expect("register count"), OpWidth::W32)?;
                self.emit_logic_imm_mask(count, count, 0b00, 0xff, OpWidth::W32)?;
            }
            SrcOperand::Shifted { .. } | SrcOperand::Extended { .. } => unreachable!(),
        }
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
        self.emit_arm_w32_reg_shift_result(dst, original, count, shift)?;

        if flags == FlagUpdate::None {
            self.emit_merge_requested_nzcv(saved_flags, produced_flags, FlagSet::EMPTY)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        self.emit_init_shift_nz_flags(produced_flags, temp, dst, OpWidth::W32)?;
        self.emit_arm_w32_reg_shift_carry(
            produced_flags,
            temp,
            saved_flags,
            original,
            count,
            dst,
            shift,
        )?;
        self.emit_merge_requested_nzcv(saved_flags, produced_flags, nzc)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    /// Lower the complete A32 data-processing register-shifted-register
    /// opcode space as one alias-safe compound operation.
    pub(crate) fn lower_arm_dp_reg_shift(
        &mut self,
        kind: ArmDpRegShiftKind,
        dst: Option<VReg>,
        rn: Option<VReg>,
        rm: VReg,
        rs: VReg,
        shift: ShiftOp,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let nzc = FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF);
        let expected_flags = if kind.is_logical() {
            nzc
        } else {
            FlagSet::NZCV
        };
        if (dst.is_some() != kind.writes_result())
            || (rn.is_some() != kind.uses_rn())
            || (flags != FlagUpdate::None && flags != FlagUpdate::Specific(expected_flags))
            || !matches!(
                shift,
                ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
            )
        {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native A32 data-processing register shift".into(),
                operand: format!(
                    "kind={kind:?}, dst={dst:?}, rn={rn:?}, flags={flags:?}, shift={shift:?}"
                ),
            });
        }

        let dst_reg = dst.map(Self::dst_gpr_arm_or_x86).transpose()?;
        let rn_reg = rn.map(Self::gpr_arm_or_x86).transpose()?.unwrap_or(31);
        let rm_reg = Self::gpr_arm_or_x86(rm)?;
        let rs_reg = Self::gpr_arm_or_x86(rs)?;
        let mut avoid = vec![rn_reg, rm_reg, rs_reg];
        if let Some(reg) = dst_reg {
            avoid.push(reg);
        }
        let scratches = Self::scratch_regs(&avoid, 7)?;
        let original = scratches[0];
        let count = scratches[1];
        let shifted = scratches[2];
        let saved_flags = scratches[3];
        let produced = scratches[4];
        let temp = scratches[5];
        let discarded = scratches[6];
        let result = dst_reg.unwrap_or(discarded);
        self.emit_scratch_save(&scratches);

        // Snapshot both shifter operands before any guest destination write;
        // this covers every Rd/Rn/Rm/Rs equality partition.
        self.emit_mov_reg(original, rm_reg, OpWidth::W32)?;
        self.emit_mov_reg(count, rs_reg, OpWidth::W32)?;
        self.emit_logic_imm_mask(count, count, 0b00, 0xff, OpWidth::W32)?;
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
        self.emit_arm_w32_reg_shift_result(shifted, original, count, shift)?;

        // ADC/SBC/RSC consume the incoming A32 C bit. Internal shifter
        // comparisons have changed host PSTATE, so restore it immediately
        // before the carry-consuming final instruction.
        if kind.reads_carry() {
            self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
        }
        self.emit_arm_dp_reg_shift_result(
            kind,
            result,
            rn_reg,
            shifted,
            !kind.is_logical() && flags.updates_any(),
        )?;

        if flags == FlagUpdate::None {
            self.emit_merge_requested_nzcv(saved_flags, produced, FlagSet::EMPTY)?;
        } else if kind.is_logical() {
            self.emit_init_shift_nz_flags(produced, temp, result, OpWidth::W32)?;
            self.emit_arm_w32_reg_shift_carry(
                produced,
                temp,
                saved_flags,
                original,
                count,
                shifted,
                shift,
            )?;
            self.emit_merge_requested_nzcv(saved_flags, produced, nzc)?;
        } else {
            self.emit_sysreg(produced, ArmReg::Nzcv, true)?;
            self.emit_merge_requested_nzcv(saved_flags, produced, FlagSet::NZCV)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_shift(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        shift: ShiftOp,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let Some((dst_reg, result)) =
            Self::x86_partial_write_scratch(dst, width, &[src], &[amount])?
        {
            let scratches = [result];
            self.emit_scratch_save(&scratches);
            self.lower_shift(
                Self::arm_x_reg(result),
                src,
                amount,
                shift,
                set_flags,
                width,
            )?;
            self.emit_bitfield(dst_reg, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        if set_flags {
            let dst = Self::dst_gpr_arm_or_x86(dst)?;
            let src = Self::gpr_arm_or_x86(src)?;
            return self.lower_shift_with_flags(dst, src, amount, shift, width);
        }

        if matches!(
            shift,
            ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
        ) && src == VReg::Imm(0)
        {
            let emit_width = match width {
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                OpWidth::W64 => OpWidth::W64,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native zero-source shift width {other:?}"),
                    });
                }
            };
            return self.emit_mov_imm_best(Self::dst_gpr(dst)?, 0, emit_width);
        }

        if matches!(shift, ShiftOp::Asr | ShiftOp::Ror) {
            if let VReg::Imm(value) = src {
                let emit_width = match width {
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                    OpWidth::W64 => OpWidth::W64,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native all-ones-source shift width {other:?}"),
                        });
                    }
                };
                let result = width.mask();
                if (value as u64 & result) == result {
                    return self.emit_mov_imm_best(Self::dst_gpr(dst)?, result as i64, emit_width);
                }
            }
        }

        if shift == ShiftOp::Lsl {
            if let (VReg::Imm(value), Some(amount)) = (src, Self::src_imm(amount)) {
                let emit_width = match width {
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                    OpWidth::W64 => OpWidth::W64,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native immediate-source Shl width {other:?}"),
                        });
                    }
                };
                let value = (value as u64) & width.mask();
                let amount = (amount as u64 & 0x3f) as u32;
                let result = if amount >= width.bits() {
                    0
                } else {
                    (value << amount) & width.mask()
                };
                let dst = Self::dst_gpr(dst)?;
                if self.try_emit_movn_single(dst, result, emit_width)? {
                    return Ok(());
                }
                return self.emit_mov_imm(dst, result as i64, emit_width);
            }
        }

        if shift == ShiftOp::Lsr {
            if let (VReg::Imm(value), Some(amount)) = (src, Self::src_imm(amount)) {
                let emit_width = match width {
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                    OpWidth::W64 => OpWidth::W64,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native immediate-source Shr width {other:?}"),
                        });
                    }
                };
                let value = (value as u64) & width.mask();
                let amount = (amount as u64 & 0x3f) as u32;
                let result = if amount >= width.bits() {
                    0
                } else {
                    (value >> amount) & width.mask()
                };
                return self.emit_mov_imm_best(Self::dst_gpr(dst)?, result as i64, emit_width);
            }
        }

        if shift == ShiftOp::Asr {
            if let (VReg::Imm(value), Some(amount)) = (src, Self::src_imm(amount)) {
                let emit_width = match width {
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                    OpWidth::W64 => OpWidth::W64,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native immediate-source Sar width {other:?}"),
                        });
                    }
                };
                let mask = width.mask();
                let value = (value as u64) & mask;
                let signed = if (value & width.sign_bit()) != 0 {
                    value | !mask
                } else {
                    value
                };
                let amount = (amount as u64 & 0x3f) as u32;
                let result = if amount >= width.bits() {
                    if (signed as i64) < 0 { mask } else { 0 }
                } else {
                    ((signed as i64 >> amount) as u64) & mask
                };
                let dst = Self::dst_gpr(dst)?;
                if self.try_emit_movn_single(dst, result, emit_width)? {
                    return Ok(());
                }
                return self.emit_mov_imm(dst, result as i64, emit_width);
            }
        }

        if shift == ShiftOp::Ror {
            if let (VReg::Imm(value), Some(amount)) = (src, Self::src_imm(amount)) {
                let emit_width = match width {
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                    OpWidth::W64 => OpWidth::W64,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native immediate-source Ror width {other:?}"),
                        });
                    }
                };
                let mask = width.mask();
                let value = (value as u64) & mask;
                let bits = width.bits() as u64;
                let cmask = if bits == 64 { 0x3f } else { 0x1f };
                let amount = ((amount as u64) & cmask) % bits;
                let result = if amount == 0 {
                    value
                } else {
                    ((value >> amount) | (value << (bits - amount))) & mask
                };
                let dst = Self::dst_gpr(dst)?;
                if self.try_emit_movn_single(dst, result, emit_width)? {
                    return Ok(());
                }
                return self.emit_mov_imm(dst, result as i64, emit_width);
            }
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let src = Self::gpr_arm_or_x86(src)?;
        if matches!(amount, SrcOperand::Reg(VReg::Imm(0)))
            && matches!(
                shift,
                ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr | ShiftOp::Ror
            )
        {
            return self.lower_shift_imm(dst, src, 0, shift, width);
        }

        match amount {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                self.lower_shift_imm(dst, src, *imm, shift, width)
            }
            SrcOperand::Reg(reg) => {
                self.lower_shift_reg(dst, src, Self::gpr_arm_or_x86(*reg)?, shift, width)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native shift amount {other:?}"),
            }),
        }
    }

    pub(crate) fn bidir_shift_op(kind: u8, negative_count: bool) -> ShiftOp {
        match kind {
            0 => {
                if negative_count {
                    ShiftOp::Asr
                } else {
                    ShiftOp::Lsl
                }
            }
            1 => {
                if negative_count {
                    ShiftOp::Lsl
                } else {
                    ShiftOp::Asr
                }
            }
            2 => {
                if negative_count {
                    ShiftOp::Lsr
                } else {
                    ShiftOp::Lsl
                }
            }
            _ => {
                if negative_count {
                    ShiftOp::Lsl
                } else {
                    ShiftOp::Lsr
                }
            }
        }
    }

    pub(crate) fn lower_bidir_shift_imm(
        &mut self,
        dst: u8,
        src: u8,
        count: i64,
        kind: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let negative_count = count < 0;
        let shift = Self::bidir_shift_op(kind, negative_count);
        let magnitude = count.abs();
        if magnitude == 64 {
            return self.lower_bidir_full_count(dst, src, shift, width);
        }
        self.lower_shift_imm(dst, src, magnitude, shift, width)
    }

    pub(crate) fn lower_bidir_shift_reg_path(
        &mut self,
        dst: u8,
        src: u8,
        count: u8,
        kind: u8,
        negative_count: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let shift = Self::bidir_shift_op(kind, negative_count);
        self.lower_shift_reg(dst, src, count, shift, width)
    }

    pub(crate) fn lower_bidir_shift(
        &mut self,
        dst: VReg,
        src: &SrcOperand,
        amount: &SrcOperand,
        kind: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match width {
            OpWidth::W32 | OpWidth::W64 => {}
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native BidirShift width {other:?}"),
                });
            }
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        if let Some(amount) = Self::src_imm(amount) {
            let count = Self::bidir_count_imm(amount);
            return match src {
                SrcOperand::Reg(src) => {
                    self.lower_bidir_shift_imm(dst, Self::gpr_arm_or_x86(*src)?, count, kind, width)
                }
                SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                    let scratches = Self::scratch_regs(&[dst], 1)?;
                    let src = scratches[0];
                    self.emit_scratch_save(&scratches);
                    self.emit_mov_imm(src, *imm, width)?;
                    self.lower_bidir_shift_imm(dst, src, count, kind, width)?;
                    self.emit_scratch_restore(&scratches);
                    Ok(())
                }
                other => Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native BidirShift source {other:?}"),
                }),
            };
        }

        let SrcOperand::Reg(amount) = amount else {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native BidirShift amount {amount:?}"),
            });
        };
        let amount = Self::gpr_arm_or_x86(*amount)?;
        let mut avoid = vec![dst, amount];
        if let SrcOperand::Reg(src) = src {
            avoid.push(Self::gpr_arm_or_x86(*src)?);
        }
        let src_needs_scratch = matches!(src, SrcOperand::Imm(_) | SrcOperand::Imm64(_));
        let scratches = Self::scratch_regs(&avoid, if src_needs_scratch { 2 } else { 1 })?;
        let count = scratches[0];

        self.emit_scratch_save(&scratches);
        let src = match src {
            SrcOperand::Reg(src) => Self::gpr_arm_or_x86(*src)?,
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let src = scratches[1];
                self.emit_mov_imm(src, *imm, width)?;
                src
            }
            other => {
                self.emit_scratch_restore(&scratches);
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native BidirShift source {other:?}"),
                });
            }
        };

        self.emit_bitfield(count, amount, 0b00, 0, 6, OpWidth::W64)?;
        let negative = self.code.position();
        self.emit_test_branch(count, 63, true, 0)?;
        self.lower_bidir_shift_reg_path(dst, src, count, kind, false, width)?;
        let done_positive = self.code.position();
        self.emit(0x1400_0000);

        self.patch_test_branch_to_current(negative, count, 63, true)?;
        self.emit_addsub_reg(count, 31, count, true, false, OpWidth::W64)?;
        let full_count = self.code.position();
        self.emit_test_branch(count, 6, true, 0)?;
        self.lower_bidir_shift_reg_path(dst, src, count, kind, true, width)?;
        let done_negative = self.code.position();
        self.emit(0x1400_0000);

        self.patch_test_branch_to_current(full_count, count, 6, true)?;
        let shift = Self::bidir_shift_op(kind, true);
        self.lower_bidir_full_count(dst, src, shift, width)?;
        self.patch_branch_to_current(done_positive)?;
        self.patch_branch_to_current(done_negative)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn carry_rotate_effective_count(
        amount: i64,
        width: OpWidth,
    ) -> Result<u32, LowerError> {
        let bits = width.bits();
        if !matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native carry rotate width {width:?}"),
            });
        }
        let mask = if bits == 64 { 0x3f } else { 0x1f };
        Ok(((amount as u64 & mask) % (u64::from(bits) + 1)) as u32)
    }

    pub(crate) fn lower_carry_rotate(
        &mut self,
        op: &str,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let amount = match amount {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => *imm,
            SrcOperand::Reg(VReg::Imm(imm)) => *imm,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native {op} amount {other:?}"),
                });
            }
        };
        if Self::carry_rotate_effective_count(amount, width)? != 0 {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native nonzero-count {op}"),
            });
        }

        let dst = Self::dst_gpr(dst)?;
        if let VReg::Imm(value) = src {
            let emit_width = match width {
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                OpWidth::W64 => OpWidth::W64,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native {op} width {other:?}"),
                    });
                }
            };
            let result = (value as u64) & width.mask();
            return self.emit_mov_imm_best(dst, result as i64, emit_width);
        }

        let src = Self::gpr(src)?;
        self.lower_shift_imm(dst, src, 0, ShiftOp::Lsl, width)
    }

    pub(crate) fn lower_shift_flag_contract(
        &mut self,
        dst: VReg,
        src: VReg,
        amount: &SrcOperand,
        shift: ShiftOp,
        flags: FlagUpdate,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let partial_nzc = FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF);
        if flags == FlagUpdate::Specific(partial_nzc) {
            return self.lower_with_selected_nzcv(partial_nzc, |lowerer| {
                lowerer.lower_shift(dst, src, amount, shift, true, width)
            });
        }
        self.lower_shift(dst, src, amount, shift, flags.updates_any(), width)
    }
}
