//! Integer add/sub/multiply lowering

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
    pub(crate) fn emit_addsub_shifted(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        subtract: bool,
        set_flags: bool,
        shift: u32,
        amount: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | ((subtract as u32) << 30)
                | ((set_flags as u32) << 29)
                | (0b01011 << 24)
                | (shift << 22)
                | ((rm as u32) << 16)
                | (amount << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn emit_addsub_reg(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        self.emit_addsub_shifted(dst, rn, rm, subtract, set_flags, 0, 0, width)
    }

    pub(crate) fn emit_addsub_extended(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        subtract: bool,
        set_flags: bool,
        option: u32,
        amount: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | ((subtract as u32) << 30)
                | ((set_flags as u32) << 29)
                | (0b01011 << 24)
                | (1 << 21)
                | ((rm as u32) << 16)
                | (option << 13)
                | (amount << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn emit_addsub_carry(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | ((subtract as u32) << 30)
                | ((set_flags as u32) << 29)
                | (0b11010000 << 21)
                | ((rm as u32) << 16)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn emit_addsub_imm(
        &mut self,
        dst: u8,
        rn: u8,
        imm: i64,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if imm < 0 {
            return Err(LowerError::InvalidOperand {
                op: if subtract { "SUB" } else { "ADD" }.into(),
                operand: format!("negative immediate {imm}"),
            });
        }
        let imm = imm as u64;
        let (shift, imm12) = if imm <= 0xfff {
            (0, imm as u32)
        } else if imm & 0xfff == 0 && (imm >> 12) <= 0xfff {
            (1, (imm >> 12) as u32)
        } else {
            return Err(LowerError::InvalidOperand {
                op: if subtract { "SUB" } else { "ADD" }.into(),
                operand: format!("immediate {imm:#x} does not fit AArch64 add/sub immediate"),
            });
        };
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | ((subtract as u32) << 30)
                | ((set_flags as u32) << 29)
                | (0b10001 << 24)
                | (shift << 22)
                | (imm12 << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn addsub_imm_fits(imm: u64) -> bool {
        imm <= 0xfff || (imm & 0xfff == 0 && (imm >> 12) <= 0xfff)
    }

    pub(crate) fn canonical_addsub_imm(
        imm: i64,
        subtract: bool,
        width: OpWidth,
    ) -> Option<(bool, i64)> {
        let value = match width {
            OpWidth::W32 => u64::from(imm as u32),
            OpWidth::W64 => imm as u64,
            _ => return None,
        };
        if Self::addsub_imm_fits(value) {
            return i64::try_from(value).ok().map(|imm| (subtract, imm));
        }

        let negated = value.wrapping_neg() & width.mask();
        if negated != 0 && Self::addsub_imm_fits(negated) {
            return i64::try_from(negated).ok().map(|imm| (!subtract, imm));
        }

        None
    }

    pub(crate) fn canonical_subword_addsub_imm(
        imm: i64,
        subtract: bool,
        width: OpWidth,
    ) -> Option<(bool, u64)> {
        if !matches!(width, OpWidth::W8 | OpWidth::W16) {
            return None;
        }

        let value = (imm as u64) & width.mask();
        if Self::addsub_imm_fits(value) {
            return Some((subtract, value));
        }

        let negated = value.wrapping_neg() & width.mask();
        if negated != 0 && Self::addsub_imm_fits(negated) {
            return Some((!subtract, negated));
        }

        None
    }

    pub(crate) fn direct_addr_reg(addr: &Address) -> Option<VReg> {
        match addr {
            Address::Direct(reg) => Some(*reg),
            _ => None,
        }
    }

    pub(crate) fn writeback_add_parts(kind: &OpKind) -> Option<(VReg, i64)> {
        match kind {
            OpKind::Add {
                dst,
                src1,
                src2,
                width: OpWidth::W64,
                flags,
            } if *dst == *src1 && !flags.updates_any() => Some((*dst, Self::src_imm(src2)?)),
            _ => None,
        }
    }

    pub(crate) fn signed_addsub_imm_fits(offset: i64) -> bool {
        let imm = if offset < 0 {
            match offset.checked_neg() {
                Some(value) => value,
                None => return false,
            }
        } else {
            offset
        } as u64;

        imm <= 0xfff || (imm & 0xfff == 0 && (imm >> 12) <= 0xfff)
    }

    pub(crate) fn emit_add_signed_imm(
        &mut self,
        dst: u8,
        rn: u8,
        offset: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let (subtract, imm) = if offset < 0 {
            (
                true,
                offset
                    .checked_neg()
                    .ok_or_else(|| LowerError::InvalidOperand {
                        op: "AArch64 native signed immediate".into(),
                        operand: format!("{offset:#x}"),
                    })?,
            )
        } else {
            (false, offset)
        };
        self.emit_addsub_imm(dst, rn, imm, subtract, false, width)
    }

    pub(crate) fn lower_addsub(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let Some((dst_reg, result)) =
            Self::x86_partial_write_scratch(dst, width, &[src1], &[src2])?
        {
            let scratches = [result];
            self.emit_scratch_save(&scratches);
            self.lower_addsub(
                Self::arm_x_reg(result),
                src1,
                src2,
                subtract,
                set_flags,
                width,
            )?;
            self.emit_bitfield(dst_reg, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            if set_flags {
                return self.lower_subword_addsub_with_flags(dst, src1, src2, subtract, width);
            }
            return self.lower_subword_addsub(dst, src1, src2, subtract, width);
        }

        let dst = Self::dst_or_zero_for_flags_arm_or_x86(dst, set_flags)?;
        if src1 == VReg::Imm(0) {
            if !subtract && !set_flags {
                if let SrcOperand::Reg(reg) = src2 {
                    let rm = Self::gpr(*reg)?;
                    if width == OpWidth::W64 && dst == rm {
                        return Ok(());
                    }
                    return self.emit_mov_reg(dst, rm, width);
                }
            }

            match src2 {
                SrcOperand::Shifted {
                    reg,
                    shift: ShiftOp::Ror,
                    amount,
                } => {
                    if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native zero-base add/sub width {width:?}"),
                        });
                    }
                    return self.lower_addsub_materialized_ror_src2(
                        dst,
                        31,
                        Self::gpr(*reg)?,
                        *amount,
                        subtract,
                        set_flags,
                        width,
                    );
                }
                SrcOperand::Reg(_) | SrcOperand::Shifted { .. } => {
                    if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native zero-base add/sub width {width:?}"),
                        });
                    }
                    let (rm, shift, amount) = Self::addsub_src2(src2, width)?;
                    return self.emit_addsub_shifted(
                        dst, 31, rm, subtract, set_flags, shift, amount, width,
                    );
                }
                SrcOperand::Extended { .. } => {
                    if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native zero-base add/sub width {width:?}"),
                        });
                    }
                    let (rm, option, amount) = Self::addsub_ext_src2(src2)?;
                    if dst == 31 {
                        return self
                            .emit_zero_base_extended_flags(rm, option, amount, subtract, width);
                    }
                    // dst = extend(rm) << amount (no SP). Fold in the subtract
                    // sign and/or flag update with an XZR-based shifted add/sub
                    // (Rn = 31 is XZR in the shifted-register encoding).
                    self.emit_zero_base_extended(dst, rm, option, amount, width)?;
                    if subtract || set_flags {
                        return self.emit_addsub_reg(dst, 31, dst, subtract, set_flags, width);
                    }
                    return Ok(());
                }
                _ => {}
            }

            if let SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) = src2 {
                let is_zero = match width {
                    OpWidth::W32 => *imm as u32 == 0,
                    OpWidth::W64 => *imm == 0,
                    _ => false,
                };
                if set_flags {
                    if is_zero {
                        return self.emit_addsub_reg(dst, 31, 31, subtract, true, width);
                    }
                    let bits = match width {
                        OpWidth::W32 => u64::from(*imm as u32),
                        OpWidth::W64 => *imm as u64,
                        _ => unreachable!(),
                    };
                    if dst == 31 {
                        let sign_bit = width.sign_bit();
                        let clears_flags = if subtract {
                            (bits & sign_bit) != 0 && bits != sign_bit
                        } else {
                            (bits & sign_bit) == 0
                        };
                        if clears_flags {
                            return self.emit_sysreg(31, ArmReg::Nzcv, false);
                        }
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "AArch64 native flag-setting zero-base {} with nonzero immediate needs a destination scratch",
                                if subtract { "Sub" } else { "Add" }
                            ),
                        });
                    }
                    if !self.try_emit_movn_single(dst, bits, width)? {
                        self.emit_mov_imm(dst, *imm, width)?;
                    }
                    return self.emit_addsub_reg(dst, 31, dst, subtract, true, width);
                }

                let value = if subtract {
                    (*imm).wrapping_neg()
                } else {
                    *imm
                };
                let bits = match width {
                    OpWidth::W32 => u64::from(value as u32),
                    OpWidth::W64 => value as u64,
                    _ => unreachable!(),
                };
                if self.try_emit_movn_single(dst, bits, width)? {
                    return Ok(());
                }
                return self.emit_mov_imm(dst, value, width);
            }
        }

        let rn = Self::gpr_arm_or_x86(src1)?;
        let is_zero_imm = |imm: i64| match width {
            OpWidth::W32 => imm as u32 == 0,
            OpWidth::W64 => imm == 0,
            _ => false,
        };
        if !set_flags {
            match src2 {
                SrcOperand::Reg(reg) if *reg == VReg::Imm(0) => {
                    if width == OpWidth::W64 && dst == rn {
                        return Ok(());
                    }
                    return self.emit_mov_reg(dst, rn, width);
                }
                SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) if is_zero_imm(*imm) => {
                    if width == OpWidth::W64 && dst == rn {
                        return Ok(());
                    }
                    return self.emit_mov_reg(dst, rn, width);
                }
                _ => {}
            }
        }
        match src2 {
            SrcOperand::Shifted {
                reg,
                shift: ShiftOp::Ror,
                amount,
            } => self.lower_addsub_materialized_ror_src2(
                dst,
                rn,
                Self::gpr(*reg)?,
                *amount,
                subtract,
                set_flags,
                width,
            ),
            SrcOperand::Reg(_) | SrcOperand::Shifted { .. } => {
                let (rm, shift, amount) = Self::addsub_src2(src2, width)?;
                self.emit_addsub_shifted(dst, rn, rm, subtract, set_flags, shift, amount, width)
            }
            SrcOperand::Extended { .. } => {
                let (rm, option, amount) = Self::addsub_ext_src2(src2)?;
                self.emit_addsub_extended(dst, rn, rm, subtract, set_flags, option, amount, width)
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let (subtract, imm) =
                    Self::canonical_addsub_imm(*imm, subtract, width).unwrap_or((subtract, *imm));
                self.emit_addsub_imm(dst, rn, imm, subtract, set_flags, width)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native add/sub source {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_addsub_materialized_ror_src2(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        amount: u8,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let amount = u32::from(amount) & (width.bits() - 1);
        if amount == 0 {
            return self.emit_addsub_reg(dst, rn, rm, subtract, set_flags, width);
        }
        if dst == 31 {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native add/sub ROR source needs a writable destination scratch".into(),
            });
        }
        if dst == rn {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native add/sub ROR source with destination aliased to base".into(),
            });
        }
        self.lower_shift_imm(dst, rm, i64::from(amount), ShiftOp::Ror, width)?;
        self.emit_addsub_reg(dst, rn, dst, subtract, set_flags, width)
    }

    pub(crate) fn lower_subword_addsub(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        subtract: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let top_bit = width.bits() - 1;
        if let SrcOperand::Reg(reg) = src2 {
            if *reg == VReg::Imm(0) {
                if src1 == VReg::Imm(0) {
                    return self.emit_mov_imm(dst, 0, OpWidth::W32);
                }
                return self.emit_bitfield(
                    dst,
                    Self::gpr_arm_or_x86(src1)?,
                    0b10,
                    0,
                    top_bit,
                    OpWidth::W32,
                );
            }
            if !subtract && src1 == VReg::Imm(0) {
                return self.emit_bitfield(
                    dst,
                    Self::gpr_arm_or_x86(*reg)?,
                    0b10,
                    0,
                    top_bit,
                    OpWidth::W32,
                );
            }
        }

        if let SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) = src2 {
            if src1 == VReg::Imm(0) {
                let value = (*imm as u64) & width.mask();
                let result = if subtract {
                    value.wrapping_neg() & width.mask()
                } else {
                    value
                };
                return self.emit_mov_imm(dst, result as i64, OpWidth::W32);
            }
        }

        if src1 == VReg::Imm(0) {
            match src2 {
                SrcOperand::Reg(_) => {
                    let (rm, shift, amount) = Self::addsub_src2(src2, OpWidth::W32)?;
                    self.emit_addsub_shifted(
                        dst,
                        31,
                        rm,
                        subtract,
                        false,
                        shift,
                        amount,
                        OpWidth::W32,
                    )?;
                    return self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32);
                }
                SrcOperand::Shifted {
                    reg,
                    shift: ShiftOp::Ror,
                    amount,
                } => {
                    self.lower_addsub_materialized_ror_src2(
                        dst,
                        31,
                        Self::gpr_arm_or_x86(*reg)?,
                        *amount,
                        subtract,
                        false,
                        OpWidth::W64,
                    )?;
                    return self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32);
                }
                SrcOperand::Shifted { .. } => {
                    let (rm, shift, amount) = Self::addsub_src2(src2, OpWidth::W64)?;
                    self.emit_addsub_shifted(
                        dst,
                        31,
                        rm,
                        subtract,
                        false,
                        shift,
                        amount,
                        OpWidth::W64,
                    )?;
                    return self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32);
                }
                SrcOperand::Extended { .. } => {
                    let (rm, option, amount) = Self::addsub_ext_src2(src2)?;
                    // dst = extend(rm) << amount (no SP); negate via an
                    // XZR-based reg sub when subtracting, then truncate to the
                    // subword width. The old `emit_addsub_extended(dst, 31, ..)`
                    // used SP (Rn = 31) as the base.
                    self.emit_zero_base_extended(dst, rm, option, amount, OpWidth::W64)?;
                    if subtract {
                        self.emit_addsub_reg(dst, 31, dst, true, false, OpWidth::W64)?;
                    }
                    return self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32);
                }
                _ => {}
            }
        }

        let rn = Self::gpr_arm_or_x86(src1)?;

        match src2 {
            SrcOperand::Reg(reg) => {
                self.emit_addsub_reg(
                    dst,
                    rn,
                    Self::gpr_arm_or_x86(*reg)?,
                    subtract,
                    false,
                    OpWidth::W32,
                )?;
            }
            SrcOperand::Shifted {
                reg,
                shift: ShiftOp::Ror,
                amount,
            } => {
                self.lower_addsub_materialized_ror_src2(
                    dst,
                    rn,
                    Self::gpr_arm_or_x86(*reg)?,
                    *amount,
                    subtract,
                    false,
                    OpWidth::W64,
                )?;
            }
            SrcOperand::Shifted { .. } => {
                let (rm, shift, amount) = Self::addsub_src2(src2, OpWidth::W64)?;
                self.emit_addsub_shifted(
                    dst,
                    rn,
                    rm,
                    subtract,
                    false,
                    shift,
                    amount,
                    OpWidth::W64,
                )?;
            }
            SrcOperand::Extended { .. } => {
                let (rm, option, amount) = Self::addsub_ext_src2(src2)?;
                self.emit_addsub_extended(
                    dst,
                    rn,
                    rm,
                    subtract,
                    false,
                    option,
                    amount,
                    OpWidth::W64,
                )?;
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let (subtract, imm) = Self::canonical_subword_addsub_imm(*imm, subtract, width)
                    .unwrap_or((subtract, (*imm as u64) & width.mask()));
                if imm == 0 {
                    return self.emit_bitfield(dst, rn, 0b10, 0, top_bit, OpWidth::W32);
                }

                let lo = imm & 0xfff;
                let hi = imm & !0xfff;
                let mut emitted = false;
                if lo != 0 {
                    self.emit_addsub_imm(dst, rn, lo as i64, subtract, false, OpWidth::W32)?;
                    emitted = true;
                }
                if hi != 0 {
                    self.emit_addsub_imm(
                        dst,
                        if emitted { dst } else { rn },
                        hi as i64,
                        subtract,
                        false,
                        OpWidth::W32,
                    )?;
                }
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword add/sub source {other:?}"),
                });
            }
        }

        self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)
    }

    pub(crate) fn emit_shifted_subword_addsub_operand(
        &mut self,
        dst: u8,
        src: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let top_bit = width.bits() - 1;
        let shift = OpWidth::W32.bits() - width.bits();
        self.emit_bitfield(dst, src, 0b10, 0, top_bit, OpWidth::W32)?;
        self.emit_logic_shifted(dst, 31, dst, 0b01, false, 0, shift, OpWidth::W32)
    }

    pub(crate) fn lower_subword_addsub_with_flags(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        subtract: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_or_zero_for_flags_arm_or_x86(dst, true)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let rm = match src2 {
            SrcOperand::Reg(reg) => Some(Self::gpr_arm_or_x86(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword add/sub source {other:?}"),
                });
            }
        };

        let mut avoid = vec![dst_reg, rn];
        if let Some(rm) = rm {
            avoid.push(rm);
        }
        let scratches = Self::scratch_regs(&avoid, 2)?;
        let lhs = scratches[0];
        let rhs = scratches[1];
        let shift = OpWidth::W32.bits() - width.bits();

        self.emit_scratch_save(&scratches);
        self.emit_shifted_subword_addsub_operand(lhs, rn, width)?;
        match src2 {
            SrcOperand::Reg(_) => {
                self.emit_shifted_subword_addsub_operand(rhs, rm.unwrap(), width)?;
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let imm = ((*imm as u64) & width.mask()) << shift;
                self.emit_mov_imm(rhs, imm as i64, OpWidth::W32)?;
            }
            _ => unreachable!(),
        }

        self.emit_addsub_reg(dst_reg, lhs, rhs, subtract, true, OpWidth::W32)?;
        if dst_reg != 31 {
            self.emit_logic_shifted(dst_reg, 31, dst_reg, 0b01, false, 1, shift, OpWidth::W32)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_addsub_carry(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        subtract: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let Some((dst_reg, result)) =
            Self::x86_partial_write_scratch(dst, width, &[src1], &[src2])?
        {
            let scratches = [result];
            self.emit_scratch_save(&scratches);
            self.lower_addsub_carry(
                Self::arm_x_reg(result),
                src1,
                src2,
                subtract,
                set_flags,
                width,
            )?;
            self.emit_bitfield(dst_reg, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            if set_flags {
                return self
                    .lower_subword_addsub_carry_with_flags(dst, src1, src2, subtract, width);
            }
            return self.lower_subword_addsub_carry(dst, src1, src2, subtract, width);
        }

        let dst = Self::dst_or_zero_for_flags_arm_or_x86(dst, set_flags)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        match src2 {
            SrcOperand::Reg(reg) => self.emit_addsub_carry(
                dst,
                rn,
                Self::gpr_arm_or_x86(*reg)?,
                subtract,
                set_flags,
                width,
            ),
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let value = match width {
                    OpWidth::W32 => u64::from(*imm as u32),
                    OpWidth::W64 => *imm as u64,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native add/sub carry width {width:?}"),
                        });
                    }
                };
                if value == 0 {
                    self.emit_addsub_carry(dst, rn, 31, subtract, set_flags, width)
                } else if value == width.mask() {
                    self.emit_addsub_carry(dst, rn, 31, !subtract, set_flags, width)
                } else if dst != 31 && dst != rn {
                    self.emit_mov_imm(dst, value as i64, width)?;
                    self.emit_addsub_carry(dst, rn, dst, subtract, set_flags, width)
                } else {
                    let scratches = Self::scratch_regs(&[dst, rn], 1)?;
                    let rm = scratches[0];
                    self.emit_scratch_save(&scratches);
                    self.emit_mov_imm(rm, value as i64, width)?;
                    self.emit_addsub_carry(dst, rn, rm, subtract, set_flags, width)?;
                    self.emit_scratch_restore(&scratches);
                    Ok(())
                }
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native add/sub carry source {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_subword_addsub_carry(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        subtract: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let top_bit = width.bits() - 1;

        match src2 {
            SrcOperand::Reg(reg) => {
                self.emit_addsub_carry(
                    dst,
                    rn,
                    Self::gpr_arm_or_x86(*reg)?,
                    subtract,
                    false,
                    OpWidth::W32,
                )?;
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let value = (*imm as u64) & width.mask();
                if value == 0 || value == width.mask() {
                    let op_subtract = if value == 0 { subtract } else { !subtract };
                    self.emit_addsub_carry(dst, rn, 31, op_subtract, false, OpWidth::W32)?;
                } else {
                    let scratches = Self::scratch_regs(&[dst, rn], 1)?;
                    let rm = scratches[0];
                    self.emit_scratch_save(&scratches);
                    self.emit_mov_imm(rm, value as i64, OpWidth::W32)?;
                    self.emit_addsub_carry(dst, rn, rm, subtract, false, OpWidth::W32)?;
                    self.emit_scratch_restore(&scratches);
                }
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword add/sub carry source {other:?}"),
                });
            }
        }

        self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)
    }

    pub(crate) fn emit_finalize_subword_addsub_carry_flags(
        &mut self,
        saved_flags: u8,
        flags: u8,
        lhs: u8,
        rhs: u8,
        result: u8,
        temp: u8,
        subtract: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        self.emit_init_shift_nz_flags(flags, temp, result, width)?;

        self.emit_ubfx_bit_to_low(temp, saved_flags, 29, OpWidth::W32)?;
        if subtract {
            let (imm_n, immr, imms) = Self::logical_bitmask_imm(1, OpWidth::W32)?;
            self.emit_logic_imm(temp, temp, 0b10, imm_n, immr, imms, OpWidth::W32)?;
            self.emit_addsub_reg(temp, rhs, temp, false, false, OpWidth::W32)?;
            self.emit_addsub_reg(31, lhs, temp, true, true, OpWidth::W32)?;
        } else {
            self.emit_addsub_reg(temp, temp, lhs, false, false, OpWidth::W32)?;
            self.emit_addsub_reg(temp, temp, rhs, false, false, OpWidth::W32)?;
            self.emit_addsub_imm(
                31,
                temp,
                (width.mask() + 1) as i64,
                true,
                true,
                OpWidth::W32,
            )?;
        }
        let no_carry = self.code.position();
        self.emit(0x5400_0000 | Self::arm_cond_code(Condition::Ult)?);
        self.emit_or_nzcv_const(flags, temp, NZCV_C)?;
        self.patch_cond_branch_to_current(no_carry, Self::arm_cond_code(Condition::Ult)?)?;

        if subtract {
            self.emit_logic_shifted(temp, lhs, rhs, 0b10, false, 0, 0, OpWidth::W32)?;
        } else {
            self.emit_logic_shifted(temp, lhs, rhs, 0b10, true, 0, 0, OpWidth::W32)?;
        }
        self.emit_logic_shifted(saved_flags, lhs, result, 0b10, false, 0, 0, OpWidth::W32)?;
        self.emit_logic_shifted(temp, temp, saved_flags, 0b00, false, 0, 0, OpWidth::W32)?;
        let no_overflow = self.code.position();
        self.emit_test_branch(temp, width.bits() - 1, false, 0)?;
        self.emit_or_nzcv_const(flags, temp, NZCV_V)?;
        self.patch_test_branch_to_current(no_overflow, temp, width.bits() - 1, false)?;

        self.emit_sysreg(flags, ArmReg::Nzcv, false)
    }

    pub(crate) fn lower_subword_addsub_carry_with_flags(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        subtract: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_or_zero_for_flags_arm_or_x86(dst, true)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let rm = match src2 {
            SrcOperand::Reg(reg) => Some(Self::gpr_arm_or_x86(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword add/sub carry source {other:?}"),
                });
            }
        };
        let top_bit = width.bits() - 1;
        let mut avoid = vec![dst_reg, rn];
        if let Some(rm) = rm {
            avoid.push(rm);
        }
        let scratches = Self::scratch_regs(&avoid, 6)?;
        let saved_flags = scratches[0];
        let flags = scratches[1];
        let lhs = scratches[2];
        let rhs = scratches[3];
        let result = scratches[4];
        let temp = scratches[5];

        self.emit_scratch_save(&scratches);
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
        self.emit_bitfield(lhs, rn, 0b10, 0, top_bit, OpWidth::W32)?;
        let result_src = match src2 {
            SrcOperand::Reg(_) => {
                let rm = rm.unwrap();
                self.emit_bitfield(rhs, rm, 0b10, 0, top_bit, OpWidth::W32)?;
                rm
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let imm = (i128::from(*imm) & i128::from(width.mask())) as i64;
                self.emit_mov_imm(rhs, imm, OpWidth::W32)?;
                rhs
            }
            _ => unreachable!(),
        };
        self.emit_addsub_carry(result, rn, result_src, subtract, false, OpWidth::W32)?;
        self.emit_bitfield(result, result, 0b10, 0, top_bit, OpWidth::W32)?;
        if dst_reg != 31 {
            self.emit_mov_reg(dst_reg, result, OpWidth::W32)?;
        }
        self.emit_finalize_subword_addsub_carry_flags(
            saved_flags,
            flags,
            lhs,
            rhs,
            result,
            temp,
            subtract,
            width,
        )?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_subword_logic(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        opc: u32,
        n: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let top_bit = width.bits() - 1;
        let mut subword_sign_known_clear = false;

        match src2 {
            SrcOperand::Reg(reg) => {
                self.emit_logic_reg_n(dst, rn, Self::gpr_arm_or_x86(*reg)?, opc, n, OpWidth::W32)?;
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                if n && opc != 0b00 {
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native inverted subword logical immediate".into(),
                    });
                }

                let mut imm = (*imm as u64) & width.mask();
                let opc = if n {
                    imm = (!imm) & width.mask();
                    0b00
                } else {
                    opc
                };
                if set_flags && opc == 0b00 && (imm & width.sign_bit()) == 0 {
                    subword_sign_known_clear = true;
                }

                if opc == 0b00 && imm == width.mask() {
                    self.emit_bitfield(dst, rn, 0b10, 0, top_bit, OpWidth::W32)?;
                    if set_flags {
                        return self.lower_bzhi_result_flags(
                            dst,
                            width,
                            OpWidth::W32,
                            false,
                            subword_sign_known_clear,
                        );
                    }
                    return Ok(());
                }

                if imm == 0 {
                    match opc {
                        0b00 => self.emit_mov_imm(dst, 0, OpWidth::W32)?,
                        0b01 | 0b10 => {
                            self.emit_bitfield(dst, rn, 0b10, 0, top_bit, OpWidth::W32)?;
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "AArch64 native zero subword logical immediate".into(),
                            });
                        }
                    }
                    if set_flags {
                        return self.lower_bzhi_result_flags(
                            dst,
                            width,
                            OpWidth::W32,
                            false,
                            subword_sign_known_clear,
                        );
                    }
                    return Ok(());
                } else {
                    match Self::logical_bitmask_imm(imm as i64, OpWidth::W32) {
                        Ok((imm_n, immr, imms)) => {
                            self.emit_logic_imm(dst, rn, opc, imm_n, immr, imms, OpWidth::W32)?;
                        }
                        Err(LowerError::UnsupportedOp { .. }) => {
                            if self
                                .lower_materialized_subword_logic_imm(dst, rn, opc, imm, width)
                                .is_err()
                            {
                                self.emit_logic_imm_scratch(
                                    dst,
                                    rn,
                                    opc,
                                    imm as i64,
                                    OpWidth::W32,
                                )?;
                            }
                        }
                        Err(err) => return Err(err),
                    }
                }
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword logical source {other:?}"),
                });
            }
        }

        self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)?;
        if set_flags {
            self.lower_bzhi_result_flags(
                dst,
                width,
                OpWidth::W32,
                false,
                subword_sign_known_clear,
            )?;
        }
        Ok(())
    }

    pub(crate) fn lower_materialized_subword_logic_imm(
        &mut self,
        dst: u8,
        rn: u8,
        opc: u32,
        imm: u64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if dst == rn {
            let all_ones = width.mask();
            if opc == 0b00 {
                let mut remaining = (!imm) & all_ones;
                if remaining == 0 {
                    return Ok(());
                }
                if remaining.count_ones() <= 3 {
                    while remaining != 0 {
                        let bit = remaining.trailing_zeros();
                        let chunk = !(1_i64 << bit);
                        let (imm_n, immr, imms) = Self::logical_bitmask_imm(chunk, OpWidth::W32)?;
                        self.emit_logic_imm(dst, dst, opc, imm_n, immr, imms, OpWidth::W32)?;
                        remaining &= remaining - 1;
                    }
                    return Ok(());
                }
            }
            if matches!(opc, 0b01 | 0b10) && imm.count_ones() <= 3 {
                let mut remaining = imm;
                while remaining != 0 {
                    let bit = remaining.trailing_zeros();
                    let chunk = 1_i64 << bit;
                    let (imm_n, immr, imms) = Self::logical_bitmask_imm(chunk, OpWidth::W32)?;
                    self.emit_logic_imm(dst, dst, opc, imm_n, immr, imms, OpWidth::W32)?;
                    remaining &= remaining - 1;
                }
                return Ok(());
            }
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native materialized subword logical immediate needs dst != src1"
                    .into(),
            });
        }
        self.emit_mov_imm_best(dst, imm as i64, OpWidth::W32)?;
        self.emit_logic_reg_n(dst, rn, dst, opc, false, OpWidth::W32)
    }

    pub(crate) fn lower_subword_logic_with_flags(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        opc: u32,
        n: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_or_zero_for_flags_arm_or_x86(dst, true)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let rm = match src2 {
            SrcOperand::Reg(reg) => Some(Self::gpr_arm_or_x86(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword logical source {other:?}"),
                });
            }
        };
        let mut avoid = vec![rn];
        if dst != 31 {
            avoid.push(dst);
        }
        if let Some(rm) = rm {
            avoid.push(rm);
        }
        let scratches = Self::scratch_regs(&avoid, 3)?;
        let result = scratches[0];
        let flags = scratches[1];
        let temp = scratches[2];
        let top_bit = width.bits() - 1;

        self.emit_scratch_save(&scratches);
        match (src2, rm) {
            (SrcOperand::Reg(_), Some(rm)) => {
                self.emit_logic_reg_n(result, rn, rm, opc, n, OpWidth::W32)?;
            }
            (SrcOperand::Imm(imm) | SrcOperand::Imm64(imm), None) => {
                let mut imm = (*imm as u64) & width.mask();
                if n {
                    imm = (!imm) & width.mask();
                }
                self.emit_mov_imm(temp, imm as i64, OpWidth::W32)?;
                self.emit_logic_shifted(result, rn, temp, opc, false, 0, 0, OpWidth::W32)?;
            }
            _ => unreachable!("subword logical source already classified"),
        }
        self.emit_bitfield(result, result, 0b10, 0, top_bit, OpWidth::W32)?;
        if dst != 31 {
            self.emit_mov_reg(dst, result, OpWidth::W32)?;
        }
        self.emit_init_shift_nz_flags(flags, temp, result, width)?;
        self.emit_sysreg(flags, ArmReg::Nzcv, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_neg(
        &mut self,
        dst: VReg,
        src: VReg,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let Some((dst_reg, result)) = Self::x86_partial_write_scratch(dst, width, &[src], &[])? {
            let scratches = [result];
            self.emit_scratch_save(&scratches);
            self.lower_neg(Self::arm_x_reg(result), src, set_flags, width)?;
            self.emit_bitfield(dst_reg, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        if !set_flags {
            if let VReg::Imm(value) = src {
                let emit_width = match width {
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                    OpWidth::W64 => OpWidth::W64,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native Neg width {other:?}"),
                        });
                    }
                };
                let value = 0_u64.wrapping_sub((value as u64) & width.mask()) & width.mask();
                let dst = Self::dst_gpr(dst)?;
                if self.try_emit_movn_single(dst, value, emit_width)? {
                    return Ok(());
                }
                return self.emit_mov_imm(dst, value as i64, emit_width);
            }
        }

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            if set_flags {
                return self.lower_subword_neg_with_flags(dst, src, width);
            }

            let dst = Self::dst_gpr_arm_or_x86(dst)?;
            self.emit_addsub_reg(
                dst,
                31,
                Self::gpr_arm_or_x86(src)?,
                true,
                false,
                OpWidth::W32,
            )?;
            let imms = if width == OpWidth::W8 { 7 } else { 15 };
            return self.emit_bitfield(dst, dst, 0b10, 0, imms, OpWidth::W32);
        }

        self.emit_addsub_reg(
            Self::dst_gpr_arm_or_x86(dst)?,
            31,
            Self::gpr_arm_or_x86(src)?,
            true,
            set_flags,
            width,
        )
    }

    pub(crate) fn lower_subword_neg_with_flags(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_or_zero_for_flags_arm_or_x86(dst, true)?;
        let rn = Self::gpr_arm_or_x86(src)?;
        let scratches = Self::scratch_regs(&[dst_reg, rn], 1)?;
        let rhs = scratches[0];
        let shift = OpWidth::W32.bits() - width.bits();

        self.emit_scratch_save(&scratches);
        self.emit_shifted_subword_addsub_operand(rhs, rn, width)?;
        self.emit_addsub_reg(dst_reg, 31, rhs, true, true, OpWidth::W32)?;
        if dst_reg != 31 {
            self.emit_logic_shifted(dst_reg, 31, dst_reg, 0b01, false, 1, shift, OpWidth::W32)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_subword_inc_dec_with_flags(
        &mut self,
        dst: VReg,
        src: VReg,
        decrement: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_reg = Self::dst_or_zero_for_flags_arm_or_x86(dst, true)?;
        let rn = Self::gpr_arm_or_x86(src)?;
        let scratches = Self::scratch_regs(&[dst_reg, rn], 4)?;
        let saved_flags = scratches[0];
        let flags = scratches[1];
        let lhs = scratches[2];
        let rhs = scratches[3];
        let shift = OpWidth::W32.bits() - width.bits();

        self.emit_scratch_save(&scratches);
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
        self.emit_shifted_subword_addsub_operand(lhs, rn, width)?;
        self.emit_mov_imm(rhs, 1_i64 << shift, OpWidth::W32)?;
        self.emit_addsub_reg(dst_reg, lhs, rhs, decrement, true, OpWidth::W32)?;
        if dst_reg != 31 {
            self.emit_logic_shifted(dst_reg, 31, dst_reg, 0b01, false, 1, shift, OpWidth::W32)?;
        }
        self.emit_preserve_saved_c_flag(saved_flags, flags)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn addsub_src2(
        src2: &SrcOperand,
        width: OpWidth,
    ) -> Result<(u8, u32, u32), LowerError> {
        let bits = width.bits();
        match src2 {
            SrcOperand::Reg(reg) => Ok((Self::gpr_arm_or_x86(*reg)?, 0, 0)),
            SrcOperand::Shifted { reg, shift, amount } => {
                let shift = match shift {
                    ShiftOp::Lsl => 0,
                    ShiftOp::Lsr => 1,
                    ShiftOp::Asr => 2,
                    ShiftOp::Ror | ShiftOp::Rrx => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native add/sub {shift:?} source"),
                        });
                    }
                };
                if u32::from(*amount) >= bits {
                    return Err(LowerError::InvalidOperand {
                        op: "AArch64 add/sub shifted register".into(),
                        operand: format!("amount={amount}, width={width:?}"),
                    });
                }
                Ok((Self::gpr_arm_or_x86(*reg)?, shift, u32::from(*amount)))
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native add/sub source {other:?}"),
            }),
        }
    }

    pub(crate) fn addsub_ext_src2(src2: &SrcOperand) -> Result<(u8, u32, u32), LowerError> {
        match src2 {
            SrcOperand::Extended { reg, extend, shift } => {
                let option = match extend {
                    ExtendOp::Uxtb => 0b000,
                    ExtendOp::Uxth => 0b001,
                    ExtendOp::Uxtw => 0b010,
                    ExtendOp::Uxtx => 0b011,
                    ExtendOp::Sxtb => 0b100,
                    ExtendOp::Sxth => 0b101,
                    ExtendOp::Sxtw => 0b110,
                    ExtendOp::Sxtx => 0b111,
                };
                if *shift > 4 {
                    return Err(LowerError::InvalidOperand {
                        op: "AArch64 add/sub extended register".into(),
                        operand: format!("shift={shift}"),
                    });
                }
                Ok((Self::gpr_arm_or_x86(*reg)?, option, u32::from(*shift)))
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native add/sub extended source {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_mul(
        &mut self,
        dst_lo: VReg,
        dst_hi: Option<VReg>,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        set_flags: bool,
        signed: bool,
    ) -> Result<(), LowerError> {
        if dst_hi.is_none() && width == OpWidth::W16 {
            if let Some((dst, result)) =
                Self::x86_partial_write_scratch(dst_lo, width, &[src1], &[src2])?
            {
                let scratches = [result];
                self.emit_scratch_save(&scratches);
                self.lower_mul(
                    Self::arm_x_reg(result),
                    None,
                    src1,
                    src2,
                    width,
                    set_flags,
                    signed,
                )?;
                self.emit_bitfield(dst, result, 0b01, 0, 15, OpWidth::W64)?;
                self.emit_scratch_restore(&scratches);
                return Ok(());
            }
        }

        if set_flags {
            if matches!(width, OpWidth::W8 | OpWidth::W16) && dst_hi.is_none() {
                return self.lower_subword_mul_with_flags(dst_lo, src1, src2, width, signed);
            }
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native flag-setting multiply".into(),
            });
        }
        if let Some(dst_hi) = dst_hi {
            if matches!(width, OpWidth::W16 | OpWidth::W32) {
                return self.lower_mul_full_sub64(dst_lo, dst_hi, src1, src2, width, signed);
            }
        }
        if dst_hi.is_none() {
            if let (VReg::Imm(imm), SrcOperand::Reg(reg)) = (src1, src2) {
                if !matches!(*reg, VReg::Imm(_)) {
                    let src2 = SrcOperand::Imm64(imm);
                    return self.lower_mul(dst_lo, None, *reg, &src2, width, false, signed);
                }
            }
        }
        if dst_hi.is_none() {
            if let (VReg::Imm(lhs), Some(rhs)) = (src1, Self::src_imm(src2)) {
                let emit_width = match width {
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                    OpWidth::W64 => OpWidth::W64,
                    other => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native multiply width {other:?}"),
                        });
                    }
                };
                let product = ((lhs as u64) & width.mask())
                    .wrapping_mul((rhs as u64) & width.mask())
                    & width.mask();
                let dst = Self::dst_gpr(dst_lo)?;
                if self.try_emit_movn_single(dst, product, emit_width)? {
                    return Ok(());
                }
                return self.emit_mov_imm(dst, product as i64, emit_width);
            }
        }
        if dst_hi.is_none() && matches!(src2, SrcOperand::Reg(VReg::Imm(0))) {
            let emit_width = match width {
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                OpWidth::W64 => OpWidth::W64,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native multiply width {other:?}"),
                    });
                }
            };
            return self.emit_mov_imm(Self::dst_gpr_arm_or_x86(dst_lo)?, 0, emit_width);
        }
        if dst_hi.is_none() && Self::src_imm(src2).map(|imm| (imm as u64) & width.mask()) == Some(0)
        {
            let emit_width = match width {
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                OpWidth::W64 => OpWidth::W64,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native multiply width {other:?}"),
                    });
                }
            };
            return self.emit_mov_imm(Self::dst_gpr_arm_or_x86(dst_lo)?, 0, emit_width);
        }
        if let Some(dst_hi) = dst_hi {
            let src1_zero = matches!(src1, VReg::Imm(imm) if ((imm as u64) & width.mask()) == 0);
            let src2_zero = Self::src_imm(src2)
                .map(|imm| ((imm as u64) & width.mask()) == 0)
                .unwrap_or_else(|| Self::src_operand_is_zero(src2));
            if src1_zero || src2_zero {
                if width != OpWidth::W64 {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native high-half multiply width {width:?}"),
                    });
                }
                if !matches!(dst_lo, VReg::Virtual(_)) {
                    self.emit_mov_imm(Self::dst_gpr_arm_or_x86(dst_lo)?, 0, OpWidth::W64)?;
                }
                return self.emit_mov_imm(Self::dst_gpr_arm_or_x86(dst_hi)?, 0, OpWidth::W64);
            }
        }
        if let Some(dst_hi) = dst_hi {
            if let (VReg::Imm(lhs), Some(rhs)) = (src1, Self::src_imm(src2)) {
                if width != OpWidth::W64 {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native high-half multiply width {width:?}"),
                    });
                }
                let (lo, hi) = if signed {
                    let product = (lhs as i64 as i128) * (rhs as i64 as i128);
                    (product as u64, (product >> 64) as u64)
                } else {
                    let product = (lhs as u64 as u128) * (rhs as u64 as u128);
                    (product as u64, (product >> 64) as u64)
                };
                if !matches!(dst_lo, VReg::Virtual(_)) {
                    self.emit_mov_imm_best(
                        Self::dst_gpr_arm_or_x86(dst_lo)?,
                        lo as i64,
                        OpWidth::W64,
                    )?;
                }
                return self.emit_mov_imm_best(
                    Self::dst_gpr_arm_or_x86(dst_hi)?,
                    hi as i64,
                    OpWidth::W64,
                );
            }
        }
        if dst_hi.is_none() && Self::src_imm(src2).map(|imm| (imm as u64) & width.mask()) == Some(1)
        {
            let dst = Self::dst_gpr_arm_or_x86(dst_lo)?;
            let rn = Self::gpr_arm_or_x86(src1)?;
            if width == OpWidth::W64 && dst == rn {
                return Ok(());
            }
            return match width {
                OpWidth::W8 | OpWidth::W16 => {
                    self.emit_mov_reg(dst, rn, OpWidth::W32)?;
                    self.emit_bitfield(dst, dst, 0b10, 0, width.bits() - 1, OpWidth::W32)
                }
                OpWidth::W32 | OpWidth::W64 => self.emit_mov_reg(dst, rn, width),
                other => Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native multiply width {other:?}"),
                }),
            };
        }
        if dst_hi.is_none()
            && Self::src_imm(src2).map(|imm| (imm as u64) & width.mask()) == Some(width.mask())
        {
            return self.lower_neg(dst_lo, src1, false, width);
        }
        if dst_hi.is_none() {
            if let Some(imm) = Self::src_imm(src2) {
                let multiplier = (imm as u64) & width.mask();
                if multiplier.is_power_of_two() && multiplier > 1 {
                    match width {
                        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64 => {
                            return self.lower_shift_imm(
                                Self::dst_gpr_arm_or_x86(dst_lo)?,
                                Self::gpr_arm_or_x86(src1)?,
                                i64::from(multiplier.trailing_zeros()),
                                ShiftOp::Lsl,
                                width,
                            );
                        }
                        other => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("AArch64 native multiply width {other:?}"),
                            });
                        }
                    }
                }
                return self.lower_mul_imm(dst_lo, src1, imm, width);
            }
        }
        if let Some(dst_hi) = dst_hi {
            if let Some(imm) = Self::src_imm(src2) {
                return self.lower_mul_full_imm(dst_lo, dst_hi, src1, imm, width, signed);
            }
        }
        let SrcOperand::Reg(src2) = src2 else {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native multiply source {src2:?}"),
            });
        };
        let rn = Self::gpr_arm_or_x86(src1)?;
        let rm = Self::gpr_arm_or_x86(*src2)?;

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            if dst_hi.is_some() {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native high-half multiply width {width:?}"),
                });
            }
            let dst_lo = Self::dst_gpr_arm_or_x86(dst_lo)?;
            self.emit_dp3(dst_lo, rn, rm, 31, 0b000, 0, OpWidth::W32)?;
            return self.emit_bitfield(dst_lo, dst_lo, 0b10, 0, width.bits() - 1, OpWidth::W32);
        }

        if let Some(dst_hi) = dst_hi {
            if width != OpWidth::W64 {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native high-half multiply width {width:?}"),
                });
            }
            let dst_hi = Self::dst_gpr_arm_or_x86(dst_hi)?;
            let op31 = if signed { 0b010 } else { 0b110 };
            if matches!(dst_lo, VReg::Virtual(_)) {
                return self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width);
            }

            let dst_lo = Self::dst_gpr_arm_or_x86(dst_lo)?;
            let lo_aliases_source = dst_lo == rn || dst_lo == rm;
            let hi_aliases_source = dst_hi == rn || dst_hi == rm;
            if dst_lo == dst_hi {
                // SMIR writes the low destination first and the high destination
                // second. MULX permits both architectural destinations to name
                // the same register, so only the final high half is observable.
                return self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width);
            }
            if lo_aliases_source && hi_aliases_source {
                let scratches = Self::scratch_regs(&[dst_lo, dst_hi, rn, rm], 1)?;
                let scratch = scratches[0];
                let copy_source = if dst_hi == rn {
                    rn
                } else if dst_hi == rm {
                    rm
                } else {
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native full-width multiply alias topology".into(),
                    });
                };
                let rn = if copy_source == rn { scratch } else { rn };
                let rm = if copy_source == rm { scratch } else { rm };

                self.emit_scratch_save(&scratches);
                self.emit_mov_reg(scratch, copy_source, width)?;
                self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width)?;
                self.emit_dp3(dst_lo, rn, rm, 31, 0b000, 0, width)?;
                self.emit_scratch_restore(&scratches);
                return Ok(());
            }
            if lo_aliases_source {
                self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width)?;
                return self.emit_dp3(dst_lo, rn, rm, 31, 0b000, 0, width);
            }
            self.emit_dp3(dst_lo, rn, rm, 31, 0b000, 0, width)?;
            return self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width);
        }

        self.emit_dp3(
            Self::dst_gpr_arm_or_x86(dst_lo)?,
            rn,
            rm,
            31,
            0b000,
            0,
            width,
        )
    }

    pub(crate) fn lower_mul_full_sub64(
        &mut self,
        dst_lo: VReg,
        dst_hi: VReg,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        signed: bool,
    ) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W16 | OpWidth::W32) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native sub-64 full multiply width {width:?}"),
            });
        }

        let dst_lo = Self::dst_gpr_arm_or_x86(dst_lo)?;
        let dst_hi = Self::dst_gpr_arm_or_x86(dst_hi)?;
        let src1_reg = match src1 {
            VReg::Imm(_) => None,
            reg => Some(Self::gpr_arm_or_x86(reg)?),
        };
        let (src2_reg, src2_imm) = match src2 {
            SrcOperand::Reg(VReg::Imm(imm)) => (None, Some(*imm)),
            SrcOperand::Reg(reg) => (Some(Self::gpr_arm_or_x86(*reg)?), None),
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => (None, Some(*imm)),
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native sub-64 full multiply source {other:?}"),
                });
            }
        };

        let mut avoid = vec![dst_lo, dst_hi];
        if let Some(reg) = src1_reg {
            avoid.push(reg);
        }
        if let Some(reg) = src2_reg {
            avoid.push(reg);
        }
        let scratches = Self::scratch_regs(&avoid, 4)?;
        let lhs = scratches[0];
        let rhs = scratches[1];
        let product = scratches[2];
        let high = scratches[3];

        self.emit_scratch_save(&scratches);
        match (src1, src1_reg) {
            (VReg::Imm(imm), None) => {
                self.emit_mov_imm(lhs, ((imm as u64) & width.mask()) as i64, OpWidth::W32)?;
            }
            (_, Some(reg)) => self.emit_mov_reg(lhs, reg, OpWidth::W32)?,
            _ => unreachable!("sub-64 full multiply lhs already classified"),
        }
        match (src2_reg, src2_imm) {
            (Some(reg), None) => self.emit_mov_reg(rhs, reg, OpWidth::W32)?,
            (None, Some(imm)) => {
                self.emit_mov_imm(rhs, ((imm as u64) & width.mask()) as i64, OpWidth::W32)?;
            }
            _ => unreachable!("sub-64 full multiply rhs already classified"),
        }

        if width == OpWidth::W16 {
            let opc = if signed { 0b00 } else { 0b10 };
            self.emit_bitfield(lhs, lhs, opc, 0, 15, OpWidth::W32)?;
            self.emit_bitfield(rhs, rhs, opc, 0, 15, OpWidth::W32)?;
        }

        // SMADDL/UMADDL with XZR as the accumulator are the signed/unsigned
        // widening multiplies. They retain the complete W32 product so both
        // architectural halves can be committed after every source is consumed.
        let op31 = if signed { 0b001 } else { 0b101 };
        self.emit_dp3(product, lhs, rhs, 31, op31, 0, OpWidth::W64)?;
        if width == OpWidth::W16 {
            self.emit_bitfield(high, product, 0b10, 16, 31, OpWidth::W32)?;
            if dst_lo != dst_hi {
                self.emit_bitfield(dst_lo, product, 0b01, 0, 15, OpWidth::W64)?;
            }
            self.emit_bitfield(dst_hi, high, 0b01, 0, 15, OpWidth::W64)?;
        } else {
            self.emit_bitfield(high, product, 0b10, 32, 63, OpWidth::W64)?;
            if dst_lo != dst_hi {
                self.emit_mov_reg(dst_lo, product, OpWidth::W32)?;
            }
            self.emit_mov_reg(dst_hi, high, OpWidth::W32)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_mul_imm(
        &mut self,
        dst_lo: VReg,
        src1: VReg,
        imm: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let emit_width = match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
            OpWidth::W64 => OpWidth::W64,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native multiply width {other:?}"),
                });
            }
        };
        let dst_lo = Self::dst_gpr_arm_or_x86(dst_lo)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let scratches = Self::scratch_regs(&[dst_lo, rn], 1)?;
        let rm = scratches[0];

        self.emit_scratch_save(&scratches);
        self.emit_mov_imm(rm, ((imm as u64) & width.mask()) as i64, emit_width)?;
        self.emit_dp3(dst_lo, rn, rm, 31, 0b000, 0, emit_width)?;
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            self.emit_bitfield(dst_lo, dst_lo, 0b10, 0, width.bits() - 1, OpWidth::W32)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_mul_full_imm(
        &mut self,
        dst_lo: VReg,
        dst_hi: VReg,
        src1: VReg,
        imm: i64,
        width: OpWidth,
        signed: bool,
    ) -> Result<(), LowerError> {
        if width != OpWidth::W64 {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native high-half multiply width {width:?}"),
            });
        }

        let dst_hi = Self::dst_gpr_arm_or_x86(dst_hi)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let dst_lo = if matches!(dst_lo, VReg::Virtual(_)) {
            None
        } else {
            Some(Self::dst_gpr_arm_or_x86(dst_lo)?)
        };
        // SMIR commits the high output after the low output. When both name the
        // same architectural register (valid for MULX), the low half is dead.
        let dst_lo = dst_lo.filter(|dst_lo| *dst_lo != dst_hi);

        let mut avoid = vec![dst_hi, rn];
        if let Some(dst_lo) = dst_lo {
            avoid.push(dst_lo);
        }
        let scratches = Self::scratch_regs(&avoid, 1)?;
        let rm = scratches[0];
        let op31 = if signed { 0b010 } else { 0b110 };

        self.emit_scratch_save(&scratches);
        self.emit_mov_imm(rm, imm, width)?;
        match dst_lo {
            Some(dst_lo) if dst_lo == rn => {
                self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width)?;
                self.emit_dp3(dst_lo, rn, rm, 31, 0b000, 0, width)?;
            }
            Some(dst_lo) => {
                self.emit_dp3(dst_lo, rn, rm, 31, 0b000, 0, width)?;
                self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width)?;
            }
            None => {
                self.emit_dp3(dst_hi, rn, rm, 31, op31, 0, width)?;
            }
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_subword_mul_with_flags(
        &mut self,
        dst_lo: VReg,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        signed: bool,
    ) -> Result<(), LowerError> {
        let dst = Self::dst_gpr_arm_or_x86(dst_lo)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let rm = match src2 {
            SrcOperand::Reg(reg) => Some(Self::gpr_arm_or_x86(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword multiply source {other:?}"),
                });
            }
        };
        let mut avoid = vec![dst, rn];
        if let Some(rm) = rm {
            avoid.push(rm);
        }
        let scratches = Self::scratch_regs(&avoid, 5)?;
        let flags = scratches[0];
        let lhs = scratches[1];
        let rhs = scratches[2];
        let product = scratches[3];
        let temp = scratches[4];
        let top_bit = width.bits() - 1;

        self.emit_scratch_save(&scratches);
        if signed {
            self.emit_bitfield(lhs, rn, 0b00, 0, top_bit, OpWidth::W32)?;
            match (src2, rm) {
                (SrcOperand::Reg(_), Some(rm)) => {
                    self.emit_bitfield(rhs, rm, 0b00, 0, top_bit, OpWidth::W32)?;
                }
                (SrcOperand::Imm(imm) | SrcOperand::Imm64(imm), None) => {
                    let mask = width.mask() as i64;
                    let sign = 1_i64 << top_bit;
                    let imm = (*imm & mask) ^ sign;
                    self.emit_mov_imm(rhs, imm - sign, OpWidth::W32)?;
                }
                _ => unreachable!("subword multiply source already classified"),
            }
        } else {
            self.emit_bitfield(lhs, rn, 0b10, 0, top_bit, OpWidth::W32)?;
            match (src2, rm) {
                (SrcOperand::Reg(_), Some(rm)) => {
                    self.emit_bitfield(rhs, rm, 0b10, 0, top_bit, OpWidth::W32)?;
                }
                (SrcOperand::Imm(imm) | SrcOperand::Imm64(imm), None) => {
                    self.emit_mov_imm(rhs, *imm & width.mask() as i64, OpWidth::W32)?;
                }
                _ => unreachable!("subword multiply source already classified"),
            }
        }
        self.emit_dp3(product, lhs, rhs, 31, 0b000, 0, OpWidth::W32)?;
        self.emit_bitfield(temp, product, 0b10, 0, top_bit, OpWidth::W32)?;
        self.emit_mov_reg(dst, temp, OpWidth::W32)?;

        self.emit_init_shift_nz_flags(flags, rhs, temp, width)?;
        if signed {
            self.emit_bitfield(lhs, temp, 0b00, 0, top_bit, OpWidth::W32)?;
            self.emit_addsub_reg(31, product, lhs, true, true, OpWidth::W32)?;
            let no_overflow = self.code.position();
            self.emit(0x5400_0000 | Self::arm_cond_code(Condition::Eq)?);
            self.emit_or_nzcv_const(flags, rhs, NZCV_C | NZCV_V)?;
            self.patch_cond_branch_to_current(no_overflow, Self::arm_cond_code(Condition::Eq)?)?;
        } else {
            self.emit_addsub_imm(
                31,
                product,
                (width.mask() + 1) as i64,
                true,
                true,
                OpWidth::W32,
            )?;
            let no_overflow = self.code.position();
            self.emit(0x5400_0000 | Self::arm_cond_code(Condition::Ult)?);
            self.emit_or_nzcv_const(flags, rhs, NZCV_C | NZCV_V)?;
            self.patch_cond_branch_to_current(no_overflow, Self::arm_cond_code(Condition::Ult)?)?;
        }
        self.emit_sysreg(flags, ArmReg::Nzcv, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_mul_acc(
        &mut self,
        dst: VReg,
        acc: VReg,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
        subtract: bool,
    ) -> Result<(), LowerError> {
        let as_src_operand = |reg| match reg {
            VReg::Imm(imm) => SrcOperand::Imm64(imm),
            reg => SrcOperand::Reg(reg),
        };
        let masked_imm = |reg| match reg {
            VReg::Imm(imm) => Some((imm as u64) & width.mask()),
            _ => None,
        };
        let is_masked_zero = |reg| masked_imm(reg) == Some(0);
        if is_masked_zero(src1) || is_masked_zero(src2) {
            return self.lower_addsub(dst, acc, &SrcOperand::Imm64(0), false, false, width);
        }
        if masked_imm(src1) == Some(1) {
            return self.lower_addsub(dst, acc, &as_src_operand(src2), subtract, false, width);
        }
        if masked_imm(src2) == Some(1) {
            return self.lower_addsub(dst, acc, &as_src_operand(src1), subtract, false, width);
        }
        if masked_imm(src1) == Some(width.mask()) {
            return self.lower_addsub(dst, acc, &as_src_operand(src2), !subtract, false, width);
        }
        if masked_imm(src2) == Some(width.mask()) {
            return self.lower_addsub(dst, acc, &as_src_operand(src1), !subtract, false, width);
        }
        if let (VReg::Imm(lhs), VReg::Imm(rhs)) = (src1, src2) {
            if matches!(
                width,
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
            ) {
                let product = ((lhs as u64) & width.mask())
                    .wrapping_mul((rhs as u64) & width.mask())
                    & width.mask();
                let product = product as i64;
                let encodable = match width {
                    OpWidth::W8 | OpWidth::W16 => true,
                    OpWidth::W32 | OpWidth::W64 => {
                        acc == VReg::Imm(0)
                            || Self::canonical_addsub_imm(product, subtract, width).is_some()
                    }
                    _ => false,
                };
                if encodable {
                    return self.lower_addsub(
                        dst,
                        acc,
                        &SrcOperand::Imm64(product),
                        subtract,
                        false,
                        width,
                    );
                }
            }
        }
        let shifted_factor = |factor, other| {
            let VReg::Imm(imm) = factor else {
                return None;
            };
            if matches!(other, VReg::Imm(_)) {
                return None;
            }
            let multiplier = (imm as u64) & width.mask();
            if multiplier.is_power_of_two() && multiplier > 1 {
                Some((other, multiplier.trailing_zeros()))
            } else {
                None
            }
        };
        if let Some((reg, amount)) =
            shifted_factor(src1, src2).or_else(|| shifted_factor(src2, src1))
        {
            let dst = Self::dst_gpr_arm_or_x86(dst)?;
            let rn = Self::gpr_arm_or_x86(acc)?;
            let rm = Self::gpr_arm_or_x86(reg)?;
            let emit_width = match width {
                OpWidth::W8 | OpWidth::W16 | OpWidth::W32 => OpWidth::W32,
                OpWidth::W64 => OpWidth::W64,
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("AArch64 native multiply-accumulate width {other:?}"),
                    });
                }
            };
            self.emit_addsub_shifted(dst, rn, rm, subtract, false, 0, amount, emit_width)?;
            if matches!(width, OpWidth::W8 | OpWidth::W16) {
                return self.emit_bitfield(dst, dst, 0b10, 0, width.bits() - 1, OpWidth::W32);
            }
            return Ok(());
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let rm = Self::gpr_arm_or_x86(src2)?;
        let ra = Self::gpr_arm_or_x86(acc)?;
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            self.emit_dp3(dst, rn, rm, ra, 0b000, subtract as u32, OpWidth::W32)?;
            return self.emit_bitfield(dst, dst, 0b10, 0, width.bits() - 1, OpWidth::W32);
        }
        self.emit_dp3(dst, rn, rm, ra, 0b000, subtract as u32, width)
    }

    pub(crate) fn lower_subword_div(
        &mut self,
        quot: VReg,
        rem: Option<VReg>,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        signed: bool,
    ) -> Result<(), LowerError> {
        let quot = Self::dst_gpr_arm_or_x86(quot)?;
        let rem = rem.map(Self::dst_gpr_arm_or_x86).transpose()?;
        let src1 = Self::gpr_arm_or_x86(src1)?;
        let src2_reg = match src2 {
            SrcOperand::Reg(reg) => Some(Self::gpr_arm_or_x86(*reg)?),
            SrcOperand::Imm(_) | SrcOperand::Imm64(_) => None,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native divide source {other:?}"),
                });
            }
        };

        let mut avoid = vec![quot, src1];
        if let Some(rem) = rem {
            avoid.push(rem);
        }
        if let Some(src2_reg) = src2_reg {
            avoid.push(src2_reg);
        }
        let scratches = Self::scratch_regs(&avoid, 2)?;
        let rn = scratches[0];
        let rm = scratches[1];
        let top_bit = width.bits() - 1;
        let opc = if signed { 0b00 } else { 0b10 };

        self.emit_scratch_save(&scratches);
        self.emit_bitfield(rn, src1, opc, 0, top_bit, OpWidth::W32)?;
        match (src2, src2_reg) {
            (SrcOperand::Reg(_), Some(src2_reg)) => {
                self.emit_bitfield(rm, src2_reg, opc, 0, top_bit, OpWidth::W32)?;
            }
            (SrcOperand::Imm(imm) | SrcOperand::Imm64(imm), None) => {
                let divisor = if signed {
                    let mask = width.mask() as i64;
                    let sign = 1_i64 << top_bit;
                    ((*imm & mask) ^ sign) - sign
                } else {
                    *imm & width.mask() as i64
                };
                self.emit_mov_imm(rm, divisor, OpWidth::W32)?;
            }
            _ => unreachable!("subword divide source already classified"),
        }

        let opcode2 = if signed { 0b0011 } else { 0b0010 };
        self.emit_dp2(quot, rn, rm, opcode2, OpWidth::W32)?;
        if let Some(rem) = rem {
            self.emit_dp3(rem, quot, rm, rn, 0b000, 1, OpWidth::W32)?;
        }
        if rem != Some(quot) {
            self.emit_bitfield(quot, quot, 0b10, 0, top_bit, OpWidth::W32)?;
        }
        if let Some(rem) = rem {
            self.emit_bitfield(rem, rem, 0b10, 0, top_bit, OpWidth::W32)?;
        }
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_subword_shift_imm(
        &mut self,
        dst: u8,
        src: u8,
        amount: i64,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let bits = width.bits();
        let top_bit = bits - 1;
        let amount = match shift {
            ShiftOp::Ror | ShiftOp::Rrx => (amount as u64 & u64::from(bits - 1)) as u32,
            ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr => (amount as u64 & 0x3f) as u32,
        };

        match shift {
            ShiftOp::Lsl => {
                if amount == 0 {
                    self.emit_bitfield(dst, src, 0b10, 0, top_bit, OpWidth::W32)
                } else if amount >= bits {
                    self.emit_mov_imm(dst, 0, OpWidth::W32)
                } else {
                    self.emit_bitfield(
                        dst,
                        src,
                        0b10,
                        OpWidth::W32.bits() - amount,
                        top_bit - amount,
                        OpWidth::W32,
                    )
                }
            }
            ShiftOp::Lsr => {
                if amount == 0 {
                    self.emit_bitfield(dst, src, 0b10, 0, top_bit, OpWidth::W32)
                } else if amount >= bits {
                    self.emit_mov_imm(dst, 0, OpWidth::W32)
                } else {
                    self.emit_bitfield(dst, src, 0b10, amount, top_bit, OpWidth::W32)
                }
            }
            ShiftOp::Asr => {
                self.emit_bitfield(dst, src, 0b00, amount.min(top_bit), top_bit, OpWidth::W32)?;
                self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)
            }
            ShiftOp::Ror => {
                if amount == 0 {
                    return self.emit_bitfield(dst, src, 0b10, 0, top_bit, OpWidth::W32);
                }
                self.emit_bitfield(dst, src, 0b10, 0, top_bit, OpWidth::W32)?;
                self.emit_bitfield(
                    dst,
                    dst,
                    0b01,
                    OpWidth::W32.bits() - bits,
                    top_bit,
                    OpWidth::W32,
                )?;
                self.emit_bitfield(dst, dst, 0b10, amount, amount + top_bit, OpWidth::W32)
            }
            ShiftOp::Rrx => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native {width:?} immediate {shift:?}"),
            }),
        }
    }

    pub(crate) fn emit_subword_shift_oob_guards(
        &mut self,
        amount: u8,
        width: OpWidth,
    ) -> Result<Vec<(usize, u32)>, LowerError> {
        let guard_bits: &[u32] = match width {
            OpWidth::W8 => &[3, 4, 5],
            OpWidth::W16 => &[4, 5],
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native subword shift guard width {width:?}"),
                });
            }
        };

        let mut guards = Vec::with_capacity(guard_bits.len());
        for &bit in guard_bits {
            let offset = self.code.position();
            self.emit_test_branch(amount, bit, true, 0)?;
            guards.push((offset, bit));
        }
        Ok(guards)
    }

    pub(crate) fn patch_subword_shift_oob_guards(
        &mut self,
        amount: u8,
        guards: &[(usize, u32)],
    ) -> Result<(), LowerError> {
        for &(offset, bit) in guards {
            self.patch_test_branch_to_current(offset, amount, bit, true)?;
        }
        Ok(())
    }

    pub(crate) fn lower_subword_shift_reg(
        &mut self,
        dst: u8,
        src: u8,
        amount: u8,
        shift: ShiftOp,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match shift {
            ShiftOp::Rrx => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native {width:?} variable {shift:?}"),
                });
            }
            _ => {}
        }

        let needs_temp = match shift {
            ShiftOp::Lsr | ShiftOp::Asr => dst == amount,
            ShiftOp::Ror => dst == amount,
            ShiftOp::Lsl | ShiftOp::Rrx => false,
        };
        let scratches = if needs_temp {
            Self::scratch_regs(&[dst, src, amount], 1)?
        } else {
            Vec::new()
        };
        let temp = scratches.first().copied().unwrap_or(dst);
        self.emit_scratch_save(&scratches);

        let top_bit = width.bits() - 1;
        let guards = if shift == ShiftOp::Ror {
            Vec::new()
        } else {
            self.emit_subword_shift_oob_guards(amount, width)?
        };

        match shift {
            ShiftOp::Lsl => {
                self.emit_dp2(dst, src, amount, 0b1000, OpWidth::W32)?;
                self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)?;
                let end_branch = self.code.position();
                self.emit(0x1400_0000);
                self.patch_subword_shift_oob_guards(amount, &guards)?;
                self.emit_mov_imm(dst, 0, OpWidth::W32)?;
                self.patch_branch_to_current(end_branch)?;
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            ShiftOp::Lsr => {
                self.emit_bitfield(temp, src, 0b10, 0, top_bit, OpWidth::W32)?;
                self.emit_dp2(dst, temp, amount, 0b1001, OpWidth::W32)?;
                let end_branch = self.code.position();
                self.emit(0x1400_0000);
                self.patch_subword_shift_oob_guards(amount, &guards)?;
                self.emit_mov_imm(dst, 0, OpWidth::W32)?;
                self.patch_branch_to_current(end_branch)?;
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            ShiftOp::Asr => {
                let align_sign_shift = OpWidth::W32.bits() - width.bits();
                self.emit_bitfield(
                    temp,
                    src,
                    0b10,
                    OpWidth::W32.bits() - align_sign_shift,
                    top_bit,
                    OpWidth::W32,
                )?;
                self.emit_dp2(dst, temp, amount, 0b1010, OpWidth::W32)?;
                self.emit_bitfield(
                    dst,
                    dst,
                    0b10,
                    align_sign_shift,
                    OpWidth::W32.bits() - 1,
                    OpWidth::W32,
                )?;
                let end_branch = self.code.position();
                self.emit(0x1400_0000);
                self.patch_subword_shift_oob_guards(amount, &guards)?;
                self.emit_bitfield(dst, src, 0b00, top_bit, top_bit, OpWidth::W32)?;
                self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)?;
                self.patch_branch_to_current(end_branch)?;
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            ShiftOp::Ror => {
                if needs_temp || temp == src {
                    self.emit_bitfield(temp, src, 0b10, 0, top_bit, OpWidth::W32)?;
                    match width {
                        OpWidth::W8 => {
                            self.emit_logic_shifted(
                                temp,
                                temp,
                                temp,
                                0b01,
                                false,
                                0,
                                8,
                                OpWidth::W32,
                            )?;
                            self.emit_logic_shifted(
                                temp,
                                temp,
                                temp,
                                0b01,
                                false,
                                0,
                                16,
                                OpWidth::W32,
                            )?;
                        }
                        OpWidth::W16 => {
                            self.emit_logic_shifted(
                                temp,
                                temp,
                                temp,
                                0b01,
                                false,
                                0,
                                16,
                                OpWidth::W32,
                            )?;
                        }
                        _ => unreachable!(),
                    }
                } else {
                    self.emit_bitfield(temp, src, 0b10, 0, top_bit, OpWidth::W32)?;
                    match width {
                        OpWidth::W8 => {
                            for immr in [24, 16, 8] {
                                self.emit_bitfield(temp, temp, 0b01, immr, top_bit, OpWidth::W32)?;
                            }
                        }
                        OpWidth::W16 => {
                            self.emit_bitfield(temp, temp, 0b01, 16, top_bit, OpWidth::W32)?;
                        }
                        _ => unreachable!(),
                    }
                }
                self.emit_dp2(dst, temp, amount, 0b1011, OpWidth::W32)?;
                self.emit_bitfield(dst, dst, 0b10, 0, top_bit, OpWidth::W32)?;
                self.emit_scratch_restore(&scratches);
                Ok(())
            }
            ShiftOp::Rrx => unreachable!(),
        }
    }

    pub(crate) fn lower_mul_flag_contract(
        &mut self,
        dst_lo: VReg,
        dst_hi: Option<VReg>,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
        signed: bool,
    ) -> Result<(), LowerError> {
        let partial_nz = FlagSet::SF.union(FlagSet::ZF);
        if flags == FlagUpdate::Specific(partial_nz) {
            if dst_hi.is_some() {
                return Err(LowerError::InvalidOperand {
                    op: "AArch64 selective-NZ multiply".into(),
                    operand: "high-half destination".into(),
                });
            }
            let result = Self::dst_gpr_arm_or_x86(dst_lo)?;
            return self.lower_with_selected_nzcv(partial_nz, |lowerer| {
                lowerer.lower_mul(dst_lo, None, src1, src2, width, false, signed)?;
                lowerer.emit_logic_reg_n(31, result, result, 0b11, false, width)
            });
        }
        self.lower_mul(
            dst_lo,
            dst_hi,
            src1,
            src2,
            width,
            flags.updates_any(),
            signed,
        )
    }
}
