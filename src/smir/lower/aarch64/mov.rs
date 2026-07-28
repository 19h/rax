//! Register move and immediate materialization

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
    pub(crate) fn fp_reg(vreg: VReg) -> Result<u8, LowerError> {
        match vreg {
            VReg::Arch(ArchReg::Arm(ArmReg::V(n))) if n < 32 => Ok(n),
            other => Err(LowerError::InvalidRegister(format!(
                "AArch64 native lowerer expected V register, got {other:?}"
            ))),
        }
    }

    pub(crate) fn emit_mov_reg(
        &mut self,
        dst: u8,
        src: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (0b01 << 29)
                | (0b01010 << 24)
                | (31 << 5)
                | ((src as u32) << 16)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn emit_mov_imm(
        &mut self,
        dst: u8,
        imm: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        let bits = match width {
            OpWidth::W32 => imm as u32 as u64,
            OpWidth::W64 => imm as u64,
            _ => unreachable!(),
        };
        let chunks = if width == OpWidth::W32 { 2 } else { 4 };
        let mut emitted = false;
        for idx in 0..chunks {
            let chunk = ((bits >> (idx * 16)) & 0xffff) as u32;
            if !emitted || chunk != 0 {
                let opc = if emitted { 0b11 } else { 0b10 };
                self.emit(
                    (sf << 31)
                        | (opc << 29)
                        | (0b100101 << 23)
                        | ((idx as u32) << 21)
                        | (chunk << 5)
                        | (dst as u32),
                );
                emitted = true;
            }
        }
        Ok(())
    }

    pub(crate) fn emit_mov_imm_best(
        &mut self,
        dst: u8,
        imm: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let bits = match width {
            OpWidth::W32 => imm as u32 as u64,
            OpWidth::W64 => imm as u64,
            _ => return self.emit_mov_imm(dst, imm, width),
        };
        if self.try_emit_movn_single(dst, bits, width)? {
            return Ok(());
        }
        self.emit_mov_imm(dst, imm, width)
    }

    pub(crate) fn emit_movn_imm16(
        &mut self,
        dst: u8,
        imm16: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit((sf << 31) | (0b100101 << 23) | ((imm16 & 0xffff) << 5) | (dst as u32));
        Ok(())
    }

    pub(crate) fn emit_movn_zero(&mut self, dst: u8, width: OpWidth) -> Result<(), LowerError> {
        self.emit_movn_imm16(dst, 0, width)
    }

    pub(crate) fn try_emit_movn_single(
        &mut self,
        dst: u8,
        bits: u64,
        width: OpWidth,
    ) -> Result<bool, LowerError> {
        let mask = width.mask();
        if (bits | 0xffff) == mask {
            self.emit_movn_imm16(dst, (!bits & 0xffff) as u32, width)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn src_imm(src: &SrcOperand) -> Option<i64> {
        match src {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => Some(*imm),
            _ => None,
        }
    }

    pub(crate) fn transfer_reg_aliases_base(rt: u8, base: VReg) -> bool {
        match Self::base_gpr(base) {
            Ok(rn) => rn == rt,
            Err(_) => false,
        }
    }

    pub(crate) fn pair_scaled_imm(
        width: MemWidth,
        offset: i64,
    ) -> Result<Option<(u32, i64)>, LowerError> {
        let (opc, scale) = Self::pair_width(width)?;
        if offset % scale != 0 {
            return Ok(None);
        }

        let imm7 = offset / scale;
        if (-64..=63).contains(&imm7) {
            Ok(Some((opc, imm7)))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn ldpsw_scaled_imm(offset: i64) -> Option<i64> {
        if offset % 4 != 0 {
            return None;
        }
        let imm7 = offset / 4;
        (-64..=63).contains(&imm7).then_some(imm7)
    }

    pub(crate) fn literal_scaled_imm19(
        op: &str,
        target: i64,
        insn_pc: i64,
    ) -> Result<i32, LowerError> {
        let delta = target.wrapping_sub(insn_pc);
        if delta % 4 != 0 {
            return Err(LowerError::InvalidOperand {
                op: op.into(),
                operand: format!("unaligned PC-relative target {target:#x} from {insn_pc:#x}"),
            });
        }

        let imm19 = delta / 4;
        if !(-(1_i64 << 18)..=(1_i64 << 18) - 1).contains(&imm19) {
            return Err(LowerError::InvalidOperand {
                op: op.into(),
                operand: format!("PC-relative target {target:#x} from {insn_pc:#x}"),
            });
        }

        Ok(imm19 as i32)
    }

    pub(crate) fn lower_mov(
        &mut self,
        dst: VReg,
        src: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let Some(reg) = Self::sysreg_vreg(dst) {
            return self.lower_sysreg_write(reg, src, width);
        }
        if let SrcOperand::Reg(src_reg) = src {
            if let Some(reg) = Self::sysreg_vreg(*src_reg) {
                return self.lower_sysreg_read(dst, reg, width);
            }
        }

        let x86_partial_dst = matches!(dst, VReg::Arch(ArchReg::X86(_)))
            && matches!(width, OpWidth::W8 | OpWidth::W16);
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        match src {
            SrcOperand::Reg(reg) => {
                let src = Self::gpr_arm_or_x86(*reg)?;
                if x86_partial_dst {
                    if dst == src {
                        return Ok(());
                    }
                    return self.emit_bitfield(dst, src, 0b01, 0, width.bits() - 1, OpWidth::W64);
                }
                if width == OpWidth::W64 && dst == src {
                    return Ok(());
                }
                self.emit_mov_reg(dst, src, width)
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                if x86_partial_dst {
                    let scratches = Self::scratch_regs(&[dst], 1)?;
                    let value = (*imm as u64) & width.mask();
                    self.emit_scratch_save(&scratches);
                    self.emit_mov_imm_best(scratches[0], value as i64, OpWidth::W32)?;
                    self.emit_bitfield(dst, scratches[0], 0b01, 0, width.bits() - 1, OpWidth::W64)?;
                    self.emit_scratch_restore(&scratches);
                    return Ok(());
                }
                self.emit_mov_imm_best(dst, *imm, width)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native Mov source {other:?}"),
            }),
        }
    }

    pub(crate) fn arm_x_reg(reg: u8) -> VReg {
        VReg::Arch(ArchReg::Arm(ArmReg::X(reg)))
    }

    pub(crate) fn cls_imm(value: u64, width: OpWidth) -> Result<u64, LowerError> {
        match width {
            OpWidth::W32 => {
                let value = value as u32;
                let normalized = if (value & 0x8000_0000) != 0 {
                    !value
                } else {
                    value
                };
                Ok(u64::from(normalized.leading_zeros() - 1))
            }
            OpWidth::W64 => {
                let normalized = if (value & 0x8000_0000_0000_0000) != 0 {
                    !value
                } else {
                    value
                };
                Ok(u64::from(normalized.leading_zeros() - 1))
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native Cls width {other:?}"),
            }),
        }
    }

    pub(crate) fn rev16_imm(value: u64, width: OpWidth) -> Result<u64, LowerError> {
        match width {
            OpWidth::W32 | OpWidth::W64 => {
                let value = value & width.mask();
                Ok((((value & 0x00ff_00ff_00ff_00ff) << 8)
                    | ((value & 0xff00_ff00_ff00_ff00) >> 8))
                    & width.mask())
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native Rev16 width {other:?}"),
            }),
        }
    }

    pub(crate) fn rev32_imm(value: u64, width: OpWidth) -> Result<u64, LowerError> {
        match width {
            OpWidth::W32 => Ok(u64::from((value as u32).swap_bytes())),
            OpWidth::W64 => {
                let lo = u64::from((value as u32).swap_bytes());
                let hi = u64::from(((value >> 32) as u32).swap_bytes()) << 32;
                Ok(hi | lo)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native Rev32 width {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_div_regs(
        &mut self,
        quot: u8,
        rem: Option<VReg>,
        rn: u8,
        rm: u8,
        width: OpWidth,
        signed: bool,
    ) -> Result<(), LowerError> {
        let opcode2 = if signed { 0b0011 } else { 0b0010 };
        if let Some(rem) = rem {
            let rem = Self::dst_gpr_arm_or_x86(rem)?;
            if quot == rn || quot == rm {
                if rem == rn || rem == rm {
                    let scratches = Self::scratch_regs(&[quot, rem, rn, rm], 1)?;
                    let scratch = scratches[0];
                    let saved_source = if quot == rn { rn } else { rm };
                    let div_rn = if quot == rn { scratch } else { rn };
                    let div_rm = if quot == rm { scratch } else { rm };

                    self.emit_scratch_save(&scratches);
                    self.emit_mov_reg(scratch, saved_source, width)?;
                    self.emit_dp2(quot, div_rn, div_rm, opcode2, width)?;
                    self.emit_dp3(rem, quot, div_rm, div_rn, 0b000, 1, width)?;
                    self.emit_scratch_restore(&scratches);
                    return Ok(());
                }
                self.emit_dp2(rem, rn, rm, opcode2, width)?;
                self.emit_dp3(rem, rem, rm, rn, 0b000, 1, width)?;
                return self.emit_dp2(quot, rn, rm, opcode2, width);
            }
            self.emit_dp2(quot, rn, rm, opcode2, width)?;
            return self.emit_dp3(rem, quot, rm, rn, 0b000, 1, width);
        }
        self.emit_dp2(quot, rn, rm, opcode2, width)
    }

    pub(crate) fn src_imm_eq(src: &SrcOperand, value: i64) -> bool {
        matches!(src, SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) if *imm == value)
    }

    pub(crate) fn src_masked_imm_eq(src: &SrcOperand, value: i64, width: OpWidth) -> bool {
        let Some(imm) = Self::src_imm(src) else {
            return false;
        };
        (imm as u64 & width.mask()) == (value as u64 & width.mask())
    }

    pub(crate) fn vreg_src(reg: VReg) -> SrcOperand {
        match reg {
            VReg::Imm(value) => SrcOperand::Imm(value),
            other => SrcOperand::Reg(other),
        }
    }

    pub(crate) fn op_dst(op: &OpKind) -> Option<VReg> {
        match op {
            OpKind::Shl { dst, .. }
            | OpKind::Shr { dst, .. }
            | OpKind::And { dst, .. }
            | OpKind::AndNot { dst, .. }
            | OpKind::Or { dst, .. }
            | OpKind::Xor { dst, .. }
            | OpKind::Mov { dst, .. } => Some(*dst),
            _ => None,
        }
    }

    pub(crate) fn src_reg_eq(src: &SrcOperand, reg: VReg) -> bool {
        matches!(src, SrcOperand::Reg(src) if *src == reg)
    }

    pub(crate) fn lower_cmove_imm(
        &mut self,
        dst: VReg,
        value: i64,
        cond: Condition,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        if cond == Condition::Always {
            let mov_width = if width == OpWidth::W64 {
                OpWidth::W64
            } else {
                OpWidth::W32
            };
            self.emit_mov_imm_best(dst, value, mov_width)?;
            return self.finish_cmove_width(dst, width);
        }

        let mov_width = if width == OpWidth::W64 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let value = (value as u64) & width.mask();
        match value {
            0 => {
                self.emit_cond_select(dst, 31, dst, Self::arm_cond_code(cond)?, 0, 0, mov_width)?;
                return self.finish_cmove_width(dst, width);
            }
            1 => {
                self.emit_cond_select(
                    dst,
                    dst,
                    31,
                    Self::inverted_arm_cond_code(cond)?,
                    0,
                    1,
                    mov_width,
                )?;
                return self.finish_cmove_width(dst, width);
            }
            value if value == width.mask() => {
                self.emit_cond_select(
                    dst,
                    dst,
                    31,
                    Self::inverted_arm_cond_code(cond)?,
                    1,
                    0,
                    mov_width,
                )?;
                return self.finish_cmove_width(dst, width);
            }
            _ => {}
        }

        let skip_mov = self.code.position();
        let inverted = Self::inverted_arm_cond_code(cond)?;
        self.emit(0x5400_0000 | inverted);

        self.emit_mov_imm_best(dst, value as i64, mov_width)?;
        self.patch_cond_branch_to_current(skip_mov, inverted)?;
        self.finish_cmove_width(dst, width)
    }

    pub(crate) fn lower_select_mov(
        &mut self,
        dst: VReg,
        src: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let mov_width = match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
            OpWidth::W64 => OpWidth::W64,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native Select width {other:?}"),
                });
            }
        };
        self.lower_mov(dst, src, mov_width)?;
        self.finish_select_width(dst, width)
    }
}
