//! Carry-rotate lowering and shift flag-contract dispatch.

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::types::{ArmReg, Condition, OpWidth, ShiftOp, SrcOperand, VReg};
use crate::smir::lower::{LowerError, aarch64::Aarch64Lowerer};

impl Aarch64Lowerer {
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
