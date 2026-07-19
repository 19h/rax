//! Bitfield, extend, reverse, and bit-count lowering

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
    pub(crate) fn emit_bitfield(
        &mut self,
        dst: u8,
        rn: u8,
        opc: u32,
        immr: u32,
        imms: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (opc << 29)
                | (0b100110 << 23)
                | (sf << 22)
                | (immr << 16)
                | (imms << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn bitfield_args(
        op: &str,
        lsb: u8,
        width_bits: u8,
        op_width: OpWidth,
    ) -> Result<u32, LowerError> {
        Self::sf(op_width)?;
        let op_bits = op_width.bits();
        if width_bits == 0
            || u32::from(lsb) >= op_bits
            || u32::from(width_bits) > op_bits
            || u32::from(lsb) + u32::from(width_bits) > op_bits
        {
            return Err(LowerError::InvalidOperand {
                op: op.into(),
                operand: format!("lsb={lsb}, width_bits={width_bits}, op_width={op_width:?}"),
            });
        }
        Ok(op_bits)
    }

    pub(crate) fn mem_index_scale_bit(scale: u8, size: u32) -> Result<u32, LowerError> {
        if scale == 1 {
            return Ok(0);
        }
        if size != 0 && u32::from(scale) == (1_u32 << size) {
            return Ok(1);
        }

        Err(LowerError::UnsupportedOp {
            op: format!("AArch64 native memory index scale {scale} for access size {size}"),
        })
    }

    pub(crate) fn bit_test_single_bit_imm(bit: u32, width: OpWidth) -> i64 {
        if width == OpWidth::W64 {
            (1_u64 << bit) as i64
        } else {
            i64::from(1_u32 << bit)
        }
    }

    pub(crate) fn finish_bit_test_result_width(
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
                op: format!("AArch64 native bit test result width {other:?}"),
            }),
        }
    }

    pub(crate) fn emit_write_c_from_low_bit(
        &mut self,
        flags: u8,
        bit: u8,
    ) -> Result<(), LowerError> {
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(!(NZCV_C as u32) as i64, OpWidth::W32)?;
        self.emit_sysreg(flags, ArmReg::Nzcv, true)?;
        self.emit_logic_imm(flags, flags, 0b00, imm_n, immr, imms, OpWidth::W32)?;
        self.emit_logic_shifted(flags, flags, bit, 0b01, false, 0, 29, OpWidth::W32)?;
        self.emit_sysreg(flags, ArmReg::Nzcv, false)
    }

    pub(crate) fn apply_bit_test_imm_action(
        &mut self,
        dst: Option<u8>,
        src: u8,
        bit: u32,
        action: BitTestAction,
        width: OpWidth,
        action_width: OpWidth,
        finish_subword: bool,
    ) -> Result<(), LowerError> {
        let Some(dst) = dst else {
            return Ok(());
        };
        let bit_mask = Self::bit_test_single_bit_imm(bit, action_width);
        let (opc, imm) = match action {
            BitTestAction::Test => return Ok(()),
            BitTestAction::Set => (0b01, bit_mask),
            BitTestAction::Reset => (0b00, Self::inverted_logical_imm(bit_mask, action_width)?),
            BitTestAction::Toggle => (0b10, bit_mask),
        };
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(imm, action_width)?;
        self.emit_logic_imm(dst, src, opc, imm_n, immr, imms, action_width)?;
        if finish_subword {
            self.finish_bit_test_result_width(dst, width)?;
        }
        Ok(())
    }

    pub(crate) fn apply_bit_test_reg_action(
        &mut self,
        dst: Option<u8>,
        src: u8,
        mask: u8,
        action: BitTestAction,
        width: OpWidth,
        action_width: OpWidth,
        finish_subword: bool,
    ) -> Result<(), LowerError> {
        let Some(dst) = dst else {
            return Ok(());
        };
        match action {
            BitTestAction::Test => return Ok(()),
            BitTestAction::Set => {
                self.emit_logic_reg_n(dst, src, mask, 0b01, false, action_width)?;
            }
            BitTestAction::Reset => {
                self.emit_logic_reg_n(dst, src, mask, 0b00, true, action_width)?;
            }
            BitTestAction::Toggle => {
                self.emit_logic_reg_n(dst, src, mask, 0b10, false, action_width)?;
            }
        }
        if finish_subword {
            self.finish_bit_test_result_width(dst, width)?;
        }
        Ok(())
    }

    pub(crate) fn lower_bit_test(
        &mut self,
        dst: Option<VReg>,
        src: VReg,
        index: &SrcOperand,
        action: BitTestAction,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        Self::bit_test_emit_width(width)?;
        let x86_partial_dst = matches!(dst, Some(VReg::Arch(ArchReg::X86(_))))
            && matches!(width, OpWidth::W8 | OpWidth::W16);
        let src = Self::gpr_arm_or_x86(src)?;
        let dst = match dst {
            Some(dst) => Some(Self::dst_gpr_arm_or_x86(dst)?),
            None => None,
        };

        match index {
            SrcOperand::Imm(value) | SrcOperand::Imm64(value) => {
                self.lower_bit_test_imm(dst, src, *value, action, width, x86_partial_dst)
            }
            SrcOperand::Reg(index) => self.lower_bit_test_reg(
                dst,
                src,
                Self::gpr_arm_or_x86(*index)?,
                action,
                width,
                x86_partial_dst,
            ),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native bit test index {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_bit_test_imm(
        &mut self,
        dst: Option<u8>,
        src: u8,
        index: i64,
        action: BitTestAction,
        width: OpWidth,
        x86_partial_dst: bool,
    ) -> Result<(), LowerError> {
        let emit_width = Self::bit_test_emit_width(width)?;
        let bit = ((index as u64) & u64::from(width.bits() - 1)) as u32;
        let mut avoid = vec![src];
        if let Some(dst) = dst {
            avoid.push(dst);
        }
        let needs_merge = x86_partial_dst && dst.is_some_and(|dst| dst != src);
        let scratches = Self::scratch_regs(&avoid, 2 + usize::from(needs_merge))?;
        let flags = scratches[0];
        let bit_reg = scratches[1];
        let result = scratches.get(2).copied();
        let action_dst = result.or(dst);
        let action_width = if x86_partial_dst && !needs_merge {
            OpWidth::W64
        } else {
            emit_width
        };

        self.emit_scratch_save(&scratches);
        self.emit_bitfield(bit_reg, src, 0b10, bit, bit, emit_width)?;
        self.apply_bit_test_imm_action(
            action_dst,
            src,
            bit,
            action,
            width,
            action_width,
            !x86_partial_dst || needs_merge,
        )?;
        if let (Some(dst), Some(result)) = (dst, result) {
            self.emit_bitfield(dst, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
        }
        self.emit_write_c_from_low_bit(flags, bit_reg)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_bit_test_reg(
        &mut self,
        dst: Option<u8>,
        src: u8,
        index: u8,
        action: BitTestAction,
        width: OpWidth,
        x86_partial_dst: bool,
    ) -> Result<(), LowerError> {
        let emit_width = Self::bit_test_emit_width(width)?;
        let amount_width = if width == OpWidth::W64 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let mut avoid = vec![src, index];
        if let Some(dst) = dst {
            avoid.push(dst);
        }
        let needs_mask = !matches!(action, BitTestAction::Test);
        let needs_merge = x86_partial_dst && dst.is_some_and(|dst| dst != src);
        let scratches = Self::scratch_regs(
            &avoid,
            3 + usize::from(needs_mask) + usize::from(needs_merge),
        )?;
        let flags = scratches[0];
        let bit_reg = scratches[1];
        let amount = scratches[2];
        let mask = scratches.get(3).copied();
        let result = scratches.get(3 + usize::from(needs_mask)).copied();
        let action_dst = result.or(dst);
        let action_width = if x86_partial_dst && !needs_merge {
            OpWidth::W64
        } else {
            emit_width
        };

        self.emit_scratch_save(&scratches);
        let (imm_n, immr, imms) =
            Self::logical_bitmask_imm(i64::from(width.bits() - 1), amount_width)?;
        self.emit_logic_imm(amount, index, 0b00, imm_n, immr, imms, amount_width)?;
        self.emit_dp2(bit_reg, src, amount, 0b1001, emit_width)?;
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(1, OpWidth::W32)?;
        self.emit_logic_imm(bit_reg, bit_reg, 0b00, imm_n, immr, imms, OpWidth::W32)?;
        if let Some(mask) = mask {
            self.emit_mov_imm(mask, 1, action_width)?;
            self.emit_dp2(mask, mask, amount, 0b1000, action_width)?;
            self.apply_bit_test_reg_action(
                action_dst,
                src,
                mask,
                action,
                width,
                action_width,
                !x86_partial_dst || needs_merge,
            )?;
        }
        if let (Some(dst), Some(result)) = (dst, result) {
            self.emit_bitfield(dst, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
        }
        self.emit_write_c_from_low_bit(flags, bit_reg)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_clz(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if width == OpWidth::W16 {
            if let Some((dst, result)) = Self::x86_partial_write_scratch(dst, width, &[src], &[])? {
                let scratches = [result];
                self.emit_scratch_save(&scratches);
                self.lower_clz(Self::arm_x_reg(result), src, width)?;
                self.emit_bitfield(dst, result, 0b01, 0, 15, OpWidth::W64)?;
                self.emit_scratch_restore(&scratches);
                return Ok(());
            }
        }

        if let VReg::Imm(value) = src {
            let emit_width = match width {
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                OpWidth::W64 => OpWidth::W64,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native Clz width {other:?}"),
                    });
                }
            };
            let value = (value as u64) & width.mask();
            let extra_bits = 64 - width.bits();
            let result = value.leading_zeros() - extra_bits;
            return self.emit_mov_imm(
                Self::dst_gpr_arm_or_x86(dst)?,
                i64::from(result),
                emit_width,
            );
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let src = Self::gpr_arm_or_x86(src)?;
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            let bits = width.bits();
            self.emit_bitfield(dst, src, 0b10, bits, bits - 1, OpWidth::W32)?;
            let sentinel = 1_i64 << (OpWidth::W32.bits() - bits - 1);
            let (imm_n, immr, imms) = Self::logical_bitmask_imm(sentinel, OpWidth::W32)?;
            self.emit_logic_imm(dst, dst, 0b01, imm_n, immr, imms, OpWidth::W32)?;
            return self.emit_dp1(dst, dst, 0b000100, OpWidth::W32);
        }
        self.emit_dp1(dst, src, 0b000100, width)
    }

    pub(crate) fn lower_bit_scan_flags(
        &mut self,
        dst: u8,
        src: VReg,
        width: OpWidth,
        emit_width: OpWidth,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        match flags {
            FlagUpdate::None => return Ok(()),
            FlagUpdate::All => {
                return if matches!(src, VReg::Imm(_)) {
                    self.lower_bit_scan_zero_flags(dst, src, width, emit_width)
                } else {
                    self.emit_logic_flags_from_source(Self::gpr_arm_or_x86(src)?, width)
                };
            }
            FlagUpdate::Specific(set) if set == FlagSet::ZF => {}
            other => {
                return Err(LowerError::InvalidOperand {
                    op: "AArch64 native bit scan".into(),
                    operand: format!("flag contract {other:?}"),
                });
            }
        }

        let mut avoid = vec![dst];
        if !matches!(src, VReg::Imm(_)) {
            avoid.push(Self::gpr_arm_or_x86(src)?);
        }
        let scratches = Self::scratch_regs(&avoid, 3)?;
        let saved = scratches[0];
        let produced = scratches[1];
        let temp = scratches[2];
        self.emit_scratch_save(&scratches);
        self.emit_sysreg(saved, ArmReg::Nzcv, true)?;
        self.lower_bit_scan_zero_flags(temp, src, width, emit_width)?;
        self.emit_sysreg(produced, ArmReg::Nzcv, true)?;
        self.emit_logic_imm_mask(saved, saved, 0b00, !(NZCV_Z as u32) as i64, OpWidth::W32)?;
        self.emit_logic_imm_mask(produced, produced, 0b00, NZCV_Z, OpWidth::W32)?;
        self.emit_logic_shifted(saved, saved, produced, 0b01, false, 0, 0, OpWidth::W32)?;
        self.emit_sysreg(saved, ArmReg::Nzcv, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_bit_scan_zero_flags(
        &mut self,
        dst: u8,
        src: VReg,
        width: OpWidth,
        emit_width: OpWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(value) = src {
            if ((value as u64) & width.mask()) == 0 {
                return self.emit_logic_reg_n(31, 31, 31, 0b11, false, emit_width);
            }
            self.emit_mov_imm(dst, 1, emit_width)?;
            return self.emit_logic_reg_n(31, dst, dst, 0b11, false, emit_width);
        }

        let src = Self::gpr_arm_or_x86(src)?;
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                let (imm_n, immr, imms) =
                    Self::logical_bitmask_imm(width.mask() as i64, OpWidth::W32)?;
                self.emit_logic_imm(31, src, 0b11, imm_n, immr, imms, OpWidth::W32)
            }
            OpWidth::W32 => self.emit_logic_reg_n(31, src, src, 0b11, false, OpWidth::W32),
            OpWidth::W64 => self.emit_logic_reg_n(31, src, src, 0b11, false, OpWidth::W64),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native bit-scan flag width {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_pdep_pext(
        &mut self,
        dst: VReg,
        src: VReg,
        mask: VReg,
        width: OpWidth,
        deposit: bool,
    ) -> Result<(), LowerError> {
        let bits = match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64 => width.bits(),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!(
                        "AArch64 native {} width {other:?}",
                        if deposit { "Pdep" } else { "Pext" }
                    ),
                });
            }
        };
        let emit_width = if width == OpWidth::W64 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let mask_imm = match mask {
            VReg::Imm(value) => Some((value as u64) & width.mask()),
            _ => None,
        };

        if let VReg::Imm(value) = src {
            if let Some(mask) = mask_imm {
                let src = (value as u64) & width.mask();
                let result = if deposit {
                    Self::eval_pdep(src, mask, bits)
                } else {
                    Self::eval_pext(src, mask, bits)
                };
                return self.emit_mov_imm(
                    Self::dst_gpr_arm_or_x86(dst)?,
                    result as i64,
                    emit_width,
                );
            }
        }

        if let Some(mask) = mask_imm {
            if mask == 0 {
                return self.emit_mov_imm(Self::dst_gpr_arm_or_x86(dst)?, 0, emit_width);
            }

            let Some((lsb, width_bits)) = Self::contiguous_bitfield(mask) else {
                let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
                let src_reg = Self::gpr_arm_or_x86(src)?;
                let scratches = if dst_reg == src_reg {
                    Self::scratch_regs(&[dst_reg, src_reg], 1)?
                } else {
                    Vec::new()
                };
                self.emit_scratch_save(&scratches);
                let src_reg = if let Some(&scratch) = scratches.first() {
                    self.emit_pdep_pext_operand(scratch, src, width, emit_width)?;
                    scratch
                } else {
                    src_reg
                };
                if deposit {
                    self.lower_pdep_const_mask(dst_reg, src_reg, mask, bits, emit_width)?;
                } else {
                    self.lower_pext_const_mask(dst_reg, src_reg, mask, bits, emit_width)?;
                }
                self.emit_scratch_restore(&scratches);
                return Ok(());
            };

            return if deposit {
                self.lower_bitfield_insert_zero(dst, src, lsb, width_bits, false, emit_width)
            } else {
                self.lower_bfx(dst, src, lsb, width_bits, false, emit_width)
            };
        }

        if deposit {
            self.lower_pdep_runtime_mask(dst, src, mask, bits, width, emit_width)
        } else {
            self.lower_pext_runtime_mask(dst, src, mask, bits, width, emit_width)
        }
    }

    pub(crate) fn emit_pdep_pext_operand(
        &mut self,
        dst: u8,
        value: VReg,
        width: OpWidth,
        emit_width: OpWidth,
    ) -> Result<(), LowerError> {
        match value {
            VReg::Imm(value) => {
                self.emit_mov_imm(dst, ((value as u64) & width.mask()) as i64, emit_width)
            }
            _ => {
                let src = Self::gpr_arm_or_x86(value)?;
                match width {
                    OpWidth::W8 | OpWidth::W16 => {
                        self.emit_bitfield(dst, src, 0b10, 0, width.bits() - 1, OpWidth::W32)
                    }
                    OpWidth::W32 | OpWidth::W64 => self.emit_mov_reg(dst, src, emit_width),
                    other => Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native PDEP/PEXT width {other:?}"),
                    }),
                }
            }
        }
    }

    pub(crate) fn emit_finish_pdep_pext_value(
        &mut self,
        dst: u8,
        src: u8,
        width: OpWidth,
        emit_width: OpWidth,
    ) -> Result<(), LowerError> {
        match width {
            OpWidth::W8 | OpWidth::W16 => {
                self.emit_bitfield(dst, src, 0b10, 0, width.bits() - 1, OpWidth::W32)
            }
            OpWidth::W32 | OpWidth::W64 => self.emit_mov_reg(dst, src, emit_width),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native PDEP/PEXT width {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_pdep_runtime_mask(
        &mut self,
        dst: VReg,
        src: VReg,
        mask: VReg,
        bits: u32,
        width: OpWidth,
        emit_width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let src_reg = match src {
            VReg::Imm(_) => None,
            _ => Some(Self::gpr_arm_or_x86(src)?),
        };
        let mask_reg = Self::gpr_arm_or_x86(mask)?;
        let mut avoid = vec![dst_reg, mask_reg];
        if let Some(src_reg) = src_reg {
            avoid.push(src_reg);
        }
        let scratches = Self::scratch_regs(&avoid, 3)?;
        let result = scratches[0];
        let src_work = scratches[1];
        let mask_work = scratches[2];
        self.emit_scratch_save(&scratches);

        self.emit_pdep_pext_operand(src_work, src, width, emit_width)?;
        self.emit_pdep_pext_operand(mask_work, mask, width, emit_width)?;
        self.emit_mov_imm(result, 0, emit_width)?;

        let result_v = Self::arm_x_reg(result);
        for out_bit in 0..bits {
            let skip_mask = self.code.position();
            self.emit_test_branch(mask_work, out_bit, false, 0)?;
            let skip_src = self.code.position();
            self.emit_test_branch(src_work, 0, false, 0)?;
            self.lower_logic(
                result_v,
                result_v,
                &Self::single_bit_operand(out_bit, emit_width),
                0b01,
                false,
                false,
                emit_width,
            )?;
            self.patch_test_branch_to_current(skip_src, src_work, 0, false)?;
            self.emit_extract(src_work, 31, src_work, 1, emit_width)?;
            self.patch_test_branch_to_current(skip_mask, mask_work, out_bit, false)?;
        }

        self.emit_finish_pdep_pext_value(dst_reg, result, width, emit_width)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_pext_runtime_mask(
        &mut self,
        dst: VReg,
        src: VReg,
        mask: VReg,
        bits: u32,
        width: OpWidth,
        emit_width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let src_reg = match src {
            VReg::Imm(_) => None,
            _ => Some(Self::gpr_arm_or_x86(src)?),
        };
        let mask_reg = Self::gpr_arm_or_x86(mask)?;
        let mut avoid = vec![dst_reg, mask_reg];
        if let Some(src_reg) = src_reg {
            avoid.push(src_reg);
        }
        let scratches = Self::scratch_regs(&avoid, 3)?;
        let result = scratches[0];
        let src_work = scratches[1];
        let mask_work = scratches[2];
        self.emit_scratch_save(&scratches);

        self.emit_pdep_pext_operand(src_work, src, width, emit_width)?;
        self.emit_pdep_pext_operand(mask_work, mask, width, emit_width)?;
        self.emit_mov_imm(result, 0, emit_width)?;

        for src_bit in (0..bits).rev() {
            let skip_mask = self.code.position();
            self.emit_test_branch(mask_work, src_bit, false, 0)?;
            self.emit_addsub_reg(result, result, result, false, false, emit_width)?;
            let skip_src = self.code.position();
            self.emit_test_branch(src_work, src_bit, false, 0)?;
            self.emit_orr_imm_one(result, result, emit_width)?;
            self.patch_test_branch_to_current(skip_src, src_work, src_bit, false)?;
            self.patch_test_branch_to_current(skip_mask, mask_work, src_bit, false)?;
        }

        self.emit_finish_pdep_pext_value(dst_reg, result, width, emit_width)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn eval_pdep(src: u64, mask: u64, bits: u32) -> u64 {
        let mut result = 0;
        let mut src_bit = 0;
        for bit in 0..bits {
            if ((mask >> bit) & 1) != 0 {
                if ((src >> src_bit) & 1) != 0 {
                    result |= 1_u64 << bit;
                }
                src_bit += 1;
            }
        }
        result
    }

    pub(crate) fn eval_pext(src: u64, mask: u64, bits: u32) -> u64 {
        let mut result = 0;
        let mut dst_bit = 0;
        for bit in 0..bits {
            if ((mask >> bit) & 1) != 0 {
                if ((src >> bit) & 1) != 0 {
                    result |= 1_u64 << dst_bit;
                }
                dst_bit += 1;
            }
        }
        result
    }

    pub(crate) fn contiguous_bitfield(mask: u64) -> Option<(u8, u8)> {
        if mask == 0 {
            return None;
        }
        let lsb = mask.trailing_zeros();
        let shifted = mask >> lsb;
        if shifted != u64::MAX && shifted & (shifted + 1) != 0 {
            return None;
        }
        Some((lsb as u8, shifted.count_ones() as u8))
    }

    pub(crate) fn single_bit_operand(bit: u32, width: OpWidth) -> SrcOperand {
        let value = 1_u64 << bit;
        if width == OpWidth::W64 {
            SrcOperand::Imm64(value as i64)
        } else {
            SrcOperand::Imm(value as i64)
        }
    }

    pub(crate) fn emit_ubfx_bit_to_low(
        &mut self,
        dst: u8,
        src: u8,
        bit: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        self.emit_bitfield(dst, src, 0b10, bit, bit, width)
    }

    pub(crate) fn emit_bfxil_bit_to_low(
        &mut self,
        dst: u8,
        src: u8,
        bit: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(!1_i64, width)?;
        self.emit_logic_imm(dst, dst, 0b00, imm_n, immr, imms, width)?;
        self.emit_bitfield(dst, src, 0b01, bit, bit, width)
    }

    pub(crate) fn emit_restore_c_from_low_bit(
        &mut self,
        flags_base: u8,
        carry: u8,
    ) -> Result<(), LowerError> {
        self.emit_logic_shifted(carry, flags_base, carry, 0b01, false, 0, 29, OpWidth::W32)?;
        self.emit_sysreg(carry, ArmReg::Nzcv, false)?;
        self.emit_ubfx_bit_to_low(carry, carry, 29, OpWidth::W32)
    }

    pub(crate) fn lower_pdep_const_mask(
        &mut self,
        dst: u8,
        src: u8,
        mask: u64,
        bits: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        self.emit_mov_imm(dst, 0, width)?;
        let dst_v = Self::arm_x_reg(dst);
        let mut src_bit = mask.count_ones();
        for out_bit in (0..bits).rev() {
            if ((mask >> out_bit) & 1) == 0 {
                continue;
            }
            src_bit -= 1;
            let skip = self.code.position();
            self.emit_test_branch(src, src_bit, false, 0)?;
            self.lower_logic(
                dst_v,
                dst_v,
                &Self::single_bit_operand(out_bit, width),
                0b01,
                false,
                false,
                width,
            )?;
            self.patch_test_branch_to_current(skip, src, src_bit, false)?;
        }
        Ok(())
    }

    pub(crate) fn lower_pext_const_mask(
        &mut self,
        dst: u8,
        src: u8,
        mask: u64,
        bits: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        self.emit_mov_imm(dst, 0, width)?;
        let mut emitted_bit = false;
        for src_bit in (0..bits).rev() {
            if ((mask >> src_bit) & 1) == 0 {
                continue;
            }
            if emitted_bit {
                self.lower_shift_imm(dst, dst, 1, ShiftOp::Lsl, width)?;
            }
            emitted_bit = true;

            let skip = self.code.position();
            self.emit_test_branch(src, src_bit, false, 0)?;
            self.emit_orr_imm_one(dst, dst, width)?;
            self.patch_test_branch_to_current(skip, src, src_bit, false)?;
        }
        Ok(())
    }

    pub(crate) fn lower_cls(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(value) = src {
            let result = Self::cls_imm(value as u64, width)?;
            return self.emit_mov_imm(Self::dst_gpr_arm_or_x86(dst)?, result as i64, width);
        }

        self.emit_dp1(
            Self::dst_gpr_arm_or_x86(dst)?,
            Self::gpr_arm_or_x86(src)?,
            0b000101,
            width,
        )
    }

    pub(crate) fn lower_rbit(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(value) = src {
            let (result, emit_width) = match width {
                OpWidth::W8 | OpWidth::W16 => (value as u64, OpWidth::W64),
                OpWidth::W32 => ((value as u32).reverse_bits() as u64, OpWidth::W32),
                OpWidth::W64 => ((value as u64).reverse_bits(), OpWidth::W64),
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native Rbit width {other:?}"),
                    });
                }
            };
            let dst = Self::dst_gpr(dst)?;
            if self.try_emit_movn_single(dst, result, emit_width)? {
                return Ok(());
            }
            return self.emit_mov_imm(dst, result as i64, emit_width);
        }

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            return self.emit_mov_reg(
                Self::dst_gpr_arm_or_x86(dst)?,
                Self::gpr_arm_or_x86(src)?,
                OpWidth::W64,
            );
        }
        self.emit_dp1(
            Self::dst_gpr_arm_or_x86(dst)?,
            Self::gpr_arm_or_x86(src)?,
            0b000000,
            width,
        )
    }

    pub(crate) fn lower_rev16(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(value) = src {
            let result = Self::rev16_imm(value as u64, width)?;
            let dst = Self::dst_gpr(dst)?;
            if self.try_emit_movn_single(dst, result, width)? {
                return Ok(());
            }
            return self.emit_mov_imm(dst, result as i64, width);
        }

        match width {
            OpWidth::W32 | OpWidth::W64 => self.emit_dp1(
                Self::dst_gpr_arm_or_x86(dst)?,
                Self::gpr_arm_or_x86(src)?,
                0b000001,
                width,
            ),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native Rev16 width {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_rev32(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(value) = src {
            let result = Self::rev32_imm(value as u64, width)?;
            let dst = Self::dst_gpr(dst)?;
            if self.try_emit_movn_single(dst, result, width)? {
                return Ok(());
            }
            return self.emit_mov_imm(dst, result as i64, width);
        }

        match width {
            OpWidth::W32 | OpWidth::W64 => self.emit_dp1(
                Self::dst_gpr_arm_or_x86(dst)?,
                Self::gpr_arm_or_x86(src)?,
                0b000010,
                width,
            ),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native Rev32 width {other:?}"),
            }),
        }
    }

    pub(crate) fn emit_bitfield_merge_from_work(
        &mut self,
        dst: u8,
        work: u8,
        src_lsb: u8,
        dst_lsb: u8,
        width_bits: u8,
        op_width: OpWidth,
    ) -> Result<(), LowerError> {
        Self::bitfield_args("Bfi merge dst", dst_lsb, width_bits, op_width)?;
        Self::bitfield_args("Bfi merge src", src_lsb, width_bits, op_width)?;

        let field_bits = if width_bits == 64 {
            u64::MAX
        } else {
            (1_u64 << width_bits) - 1
        };
        let field_mask = (field_bits << dst_lsb) & op_width.mask();
        let clear_mask = (!field_mask) & op_width.mask();
        if clear_mask == 0 {
            self.emit_mov_imm(dst, 0, op_width)?;
        } else {
            let (imm_n, immr, imms) = Self::logical_bitmask_imm(clear_mask as i64, op_width)?;
            self.emit_logic_imm(dst, dst, 0b00, imm_n, immr, imms, op_width)?;
        }
        self.emit_bitfield(
            work,
            work,
            0b10,
            u32::from(src_lsb),
            u32::from(src_lsb) + u32::from(width_bits) - 1,
            op_width,
        )?;
        self.emit_logic_shifted(
            dst,
            dst,
            work,
            0b01,
            false,
            0b00,
            u32::from(dst_lsb),
            op_width,
        )
    }

    pub(crate) fn lower_bitfield_insert_zero(
        &mut self,
        dst: VReg,
        src: VReg,
        lsb: u8,
        width_bits: u8,
        sign_extend: bool,
        op_width: OpWidth,
    ) -> Result<(), LowerError> {
        let op_bits = Self::bitfield_args("Bfiz", lsb, width_bits, op_width)?;
        if lsb == 0 {
            return self.lower_bfx(dst, src, 0, width_bits, sign_extend, op_width);
        }
        self.emit_bitfield(
            Self::dst_gpr_arm_or_x86(dst)?,
            Self::gpr_arm_or_x86(src)?,
            if sign_extend { 0b00 } else { 0b10 },
            op_bits - u32::from(lsb),
            u32::from(width_bits - 1),
            op_width,
        )
    }

    pub(crate) fn lower_bitfield_insert_low(
        &mut self,
        dst: VReg,
        dst_in: VReg,
        src: VReg,
        lsb: u8,
        width_bits: u8,
        op_width: OpWidth,
    ) -> Result<(), LowerError> {
        let op_bits = Self::bitfield_args("Bfxil", lsb, width_bits, op_width)?;
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let dst_in = Self::gpr_arm_or_x86(dst_in)?;

        if let VReg::Imm(value) = src {
            let low_mask = if width_bits == 64 {
                u64::MAX
            } else {
                (1_u64 << u32::from(width_bits)) - 1
            };
            let extracted = (((value as u64) & op_width.mask()) >> lsb) & low_mask;
            if u32::from(width_bits) == op_bits && lsb == 0 {
                if extracted == op_width.mask() {
                    return self.emit_movn_zero(dst, op_width);
                }
                return self.emit_mov_imm_best(dst, extracted as i64, op_width);
            }
            if extracted == 0 && u32::from(width_bits) < op_bits {
                let clear_mask = (!low_mask) & op_width.mask();
                if let Ok((n, immr, imms)) = Self::logical_bitmask_imm(clear_mask as i64, op_width)
                {
                    return self.emit_logic_imm(dst, dst_in, 0b00, n, immr, imms, op_width);
                }
            }
            if extracted == low_mask && u32::from(width_bits) < op_bits {
                let (n, immr, imms) = Self::logical_bitmask_imm(low_mask as i64, op_width)?;
                return self.emit_logic_imm(dst, dst_in, 0b01, n, immr, imms, op_width);
            }
            if extracted != 0 && u32::from(width_bits) < op_bits {
                let clear_mask = (!low_mask) & op_width.mask();
                if let (
                    Ok((clear_n, clear_immr, clear_imms)),
                    Ok((insert_n, insert_immr, insert_imms)),
                ) = (
                    Self::logical_bitmask_imm(clear_mask as i64, op_width),
                    Self::logical_bitmask_imm(extracted as i64, op_width),
                ) {
                    self.emit_logic_imm(
                        dst, dst_in, 0b00, clear_n, clear_immr, clear_imms, op_width,
                    )?;
                    return self.emit_logic_imm(
                        dst,
                        dst,
                        0b01,
                        insert_n,
                        insert_immr,
                        insert_imms,
                        op_width,
                    );
                }
            }
        }

        let src = Self::gpr_arm_or_x86(src)?;

        if lsb == 0 && u32::from(width_bits) == op_bits {
            if op_width == OpWidth::W64 && dst == src {
                return Ok(());
            }
            return self.emit_mov_reg(dst, src, op_width);
        }

        if dst != dst_in {
            if dst == src {
                let scratches = Self::scratch_regs(&[dst, dst_in, src], 1)?;
                let work = scratches[0];
                self.emit_scratch_save(&scratches);
                self.emit_mov_reg(work, src, op_width)?;
                self.emit_mov_reg(dst, dst_in, op_width)?;
                self.emit_bitfield(
                    dst,
                    work,
                    0b01,
                    u32::from(lsb),
                    u32::from(lsb) + u32::from(width_bits) - 1,
                    op_width,
                )?;
                self.emit_scratch_restore(&scratches);
                return Ok(());
            }
            self.emit_mov_reg(dst, dst_in, op_width)?;
        }

        self.emit_bitfield(
            dst,
            src,
            0b01,
            u32::from(lsb),
            u32::from(lsb + width_bits - 1),
            op_width,
        )
    }

    pub(crate) fn bidir_count_imm(imm: i64) -> i64 {
        let low7 = imm & 0x7f;
        (low7 << 57) >> 57
    }

    pub(crate) fn lower_bidir_full_count(
        &mut self,
        dst: u8,
        src: u8,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match shift {
            ShiftOp::Asr => {
                let top_bit = width.bits() - 1;
                self.emit_bitfield(dst, src, 0b00, top_bit, top_bit, width)
            }
            ShiftOp::Lsl | ShiftOp::Lsr => self.emit_mov_imm(dst, 0, width),
            ShiftOp::Ror | ShiftOp::Rrx => unreachable!("BidirShift never rotates"),
        }
    }

    pub(crate) fn deposit_imm_bits(value: u64, mut mask: u64) -> u64 {
        let mut result = 0;
        let mut bit = 0;
        while mask != 0 {
            let lowest = mask & mask.wrapping_neg();
            if ((value >> bit) & 1) != 0 {
                result |= lowest;
            }
            bit += 1;
            mask &= mask - 1;
        }
        result
    }

    pub(crate) fn extract_imm_bits(value: u64, mut mask: u64) -> u64 {
        let mut result = 0;
        let mut bit = 0;
        while mask != 0 {
            let lowest = mask & mask.wrapping_neg();
            if (value & lowest) != 0 {
                result |= 1_u64 << bit;
            }
            bit += 1;
            mask &= mask - 1;
        }
        result
    }

    pub(crate) fn lower_bit_permute_imm_mask(
        &mut self,
        op: &str,
        deposit: bool,
        dst: VReg,
        src: VReg,
        mask: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if !matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native {op} width {width:?}"),
            });
        }

        let VReg::Imm(mask) = mask else {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native {op}"),
            });
        };
        let mask = (mask as u64) & width.mask();
        let emit_width = match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
            OpWidth::W64 => OpWidth::W64,
            _ => unreachable!(),
        };

        if let VReg::Imm(value) = src {
            let value = (value as u64) & width.mask();
            let result = if deposit {
                Self::deposit_imm_bits(value, mask)
            } else {
                Self::extract_imm_bits(value, mask)
            } & width.mask();
            return self.emit_mov_imm_best(Self::dst_gpr(dst)?, result as i64, emit_width);
        }

        if mask == 0 {
            let dst = Self::dst_gpr(dst)?;
            return self.emit_mov_imm(dst, 0, emit_width);
        }
        if mask == width.mask() {
            let dst = Self::dst_gpr(dst)?;
            let src = Self::gpr(src)?;
            return self.lower_shift_imm(dst, src, 0, ShiftOp::Lsl, width);
        }
        if Self::is_low_contiguous_mask(mask, width) {
            let mask = SrcOperand::Imm(mask as i64);
            return self.lower_logic(dst, src, &mask, 0b00, false, false, width);
        }
        if let Some((lsb, width_bits)) = Self::contiguous_mask_field(mask) {
            if deposit {
                return self
                    .lower_bitfield_insert_zero(dst, src, lsb, width_bits, false, emit_width);
            }
            return self.lower_bfx(dst, src, lsb, width_bits, false, emit_width);
        }

        Err(LowerError::UnsupportedOp {
            op: format!("AArch64 native {op} mask"),
        })
    }
}
