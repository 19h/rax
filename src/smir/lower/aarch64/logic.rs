//! Bitwise logical lowering

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
    pub(crate) fn emit_logic_reg_n(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        opc: u32,
        n: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        self.emit_logic_shifted(dst, rn, rm, opc, n, 0, 0, width)
    }

    pub(crate) fn emit_logic_shifted(
        &mut self,
        dst: u8,
        rn: u8,
        rm: u8,
        opc: u32,
        n: bool,
        shift: u32,
        amount: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (opc << 29)
                | (0b01010 << 24)
                | (shift << 22)
                | ((n as u32) << 21)
                | ((rm as u32) << 16)
                | (amount << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn emit_logic_imm(
        &mut self,
        dst: u8,
        rn: u8,
        opc: u32,
        n: u32,
        immr: u32,
        imms: u32,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let sf = Self::sf(width)?;
        self.emit(
            (sf << 31)
                | (opc << 29)
                | (0b100100 << 23)
                | (n << 22)
                | (immr << 16)
                | (imms << 10)
                | ((rn as u32) << 5)
                | (dst as u32),
        );
        Ok(())
    }

    pub(crate) fn emit_orr_imm_one(
        &mut self,
        dst: u8,
        rn: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let n = Self::sf(width)?;
        self.emit_logic_imm(dst, rn, 0b01, n, 0, 0, width)
    }

    pub(crate) fn lower_logic(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        opc: u32,
        n: bool,
        set_flags: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let Some((dst_reg, result)) =
            Self::x86_partial_write_scratch(dst, width, &[src1], &[src2])?
        {
            let scratches = [result];
            self.emit_scratch_save(&scratches);
            self.lower_logic(
                Self::arm_x_reg(result),
                src1,
                src2,
                opc,
                n,
                set_flags,
                width,
            )?;
            self.emit_bitfield(dst_reg, result, 0b01, 0, width.bits() - 1, OpWidth::W64)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            if set_flags && matches!(dst, VReg::Virtual(_)) {
                return self.lower_subword_logic_with_flags(dst, src1, src2, opc, n, width);
            }
            let result_opc = if set_flags && opc == 0b11 { 0b00 } else { opc };
            return self.lower_subword_logic(dst, src1, src2, result_opc, n, set_flags, width);
        }

        if set_flags && opc != 0b11 {
            if matches!(dst, VReg::Virtual(_)) {
                return self.lower_logic_with_synth_flags(dst, src1, src2, opc, n, width);
            }
            let dst_reg = Self::dst_gpr(dst)?;
            self.lower_logic(dst, src1, src2, opc, n, false, width)?;
            return self.lower_bmi_result_flags(dst_reg, width, false);
        }

        if !n && (opc == 0b00 || (set_flags && opc == 0b11)) {
            if let VReg::Imm(imm) = src1 {
                let (_, value, all_ones) = Self::logical_imm_value(imm, width)?;
                if value == all_ones {
                    let dst = Self::dst_or_zero_for_flags(dst, set_flags)?;
                    match src2 {
                        SrcOperand::Reg(reg) => {
                            let src = Self::gpr(*reg)?;
                            if set_flags {
                                return self.emit_logic_reg_n(dst, src, src, 0b11, false, width);
                            }
                            if width == OpWidth::W64 && dst == src {
                                return Ok(());
                            }
                            return self.emit_mov_reg(dst, src, width);
                        }
                        SrcOperand::Shifted { .. } => {
                            if set_flags {
                                let (src, shift, amount) = Self::addsub_src2(src2, width)?;
                                return self.emit_addsub_shifted(
                                    dst, 31, src, false, true, shift, amount, width,
                                );
                            }
                            let (src, shift, amount) = Self::logical_src2(src2, width)?;
                            return self.emit_logic_shifted(
                                dst, 31, src, 0b01, false, shift, amount, width,
                            );
                        }
                        SrcOperand::Extended { .. } => {
                            let (src, option, amount) = Self::addsub_ext_src2(src2)?;
                            // `all_ones AND[S] extend(src)` == extend(src); the
                            // zero base must not be SP, so realize the value
                            // directly and derive ANDS flags (N/Z from the
                            // result, C=V=0) from it.
                            if set_flags {
                                if dst == 31 {
                                    return Err(LowerError::UnsupportedOp {
                                        op: "AArch64 native flag-only ANDS all-ones with extended source".into(),
                                    });
                                }
                                self.emit_zero_base_extended(dst, src, option, amount, width)?;
                                return self.emit_logic_reg_n(dst, dst, dst, 0b11, false, width);
                            }
                            return self.emit_zero_base_extended(dst, src, option, amount, width);
                        }
                        _ => {}
                    }
                }
            }
        }

        if n && (opc == 0b00 || (set_flags && opc == 0b11)) {
            if let VReg::Imm(imm) = src1 {
                let (_, value, all_ones) = Self::logical_imm_value(imm, width)?;
                if value == all_ones {
                    match src2 {
                        SrcOperand::Reg(reg) => {
                            let dst = Self::dst_gpr(dst)?;
                            let src = Self::gpr(*reg)?;
                            self.emit_logic_reg_n(dst, 31, src, 0b01, true, width)?;
                            if set_flags {
                                return self.lower_bmi_result_flags(dst, width, false);
                            }
                            return Ok(());
                        }
                        SrcOperand::Shifted { .. } => {
                            let dst = Self::dst_gpr(dst)?;
                            let (src, shift, amount) = Self::logical_src2(src2, width)?;
                            self.emit_logic_shifted(
                                dst, 31, src, 0b01, true, shift, amount, width,
                            )?;
                            if set_flags {
                                return self.lower_bmi_result_flags(dst, width, false);
                            }
                            return Ok(());
                        }
                        SrcOperand::Extended { .. } => {
                            let dst = Self::dst_gpr(dst)?;
                            let (src, option, amount) = Self::addsub_ext_src2(src2)?;
                            self.emit_zero_base_extended(dst, src, option, amount, width)?;
                            self.emit_logic_reg_n(dst, 31, dst, 0b01, true, width)?;
                            if set_flags {
                                return self.lower_bmi_result_flags(dst, width, false);
                            }
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }

        if !n && opc == 0b01 {
            if let VReg::Imm(imm) = src1 {
                let (_, value, all_ones) = Self::logical_imm_value(imm, width)?;
                if value == all_ones
                    && matches!(
                        src2,
                        SrcOperand::Reg(_)
                            | SrcOperand::Shifted { .. }
                            | SrcOperand::Extended { .. }
                    )
                {
                    let dst = Self::dst_gpr(dst)?;
                    return self.emit_movn_zero(dst, width);
                }
            }
        }

        if !n && opc == 0b10 {
            if let VReg::Imm(imm) = src1 {
                let (_, value, all_ones) = Self::logical_imm_value(imm, width)?;
                if value == all_ones {
                    match src2 {
                        SrcOperand::Reg(reg) => {
                            let dst = Self::dst_gpr(dst)?;
                            let src = Self::gpr(*reg)?;
                            return self.emit_logic_reg_n(dst, 31, src, 0b10, true, width);
                        }
                        SrcOperand::Shifted { .. } => {
                            let dst = Self::dst_gpr(dst)?;
                            let (src, shift, amount) = Self::logical_src2(src2, width)?;
                            return self.emit_logic_shifted(
                                dst, 31, src, 0b10, true, shift, amount, width,
                            );
                        }
                        SrcOperand::Extended { .. } => {
                            let dst = Self::dst_gpr(dst)?;
                            let (src, option, amount) = Self::addsub_ext_src2(src2)?;
                            self.emit_zero_base_extended(dst, src, option, amount, width)?;
                            return self.emit_logic_reg_n(dst, 31, dst, 0b10, true, width);
                        }
                        _ => {}
                    }
                }
            }
        }

        if let VReg::Imm(imm) = src1 {
            let (_, value, _) = Self::logical_imm_value(imm, width)?;
            if value == 0 {
                if let SrcOperand::Extended { .. } = src2 {
                    let dst = Self::dst_or_zero_for_flags(dst, set_flags)?;
                    if opc == 0b00 || (set_flags && opc == 0b11) {
                        if set_flags {
                            return self.emit_logic_reg_n(dst, 31, 31, 0b11, false, width);
                        }
                        return self.emit_mov_imm(dst, 0, width);
                    }
                    if !n && matches!(opc, 0b01 | 0b10) {
                        let (src, option, amount) = Self::addsub_ext_src2(src2)?;
                        // `0 OR/XOR extend(src)` == extend(src); realize the
                        // value directly so the zero base is not encoded as SP.
                        return self.emit_zero_base_extended(dst, src, option, amount, width);
                    }
                }
            }
        }

        if !set_flags {
            if let SrcOperand::Reg(reg) = src2 {
                let dst = Self::dst_gpr_arm_or_x86(dst)?;
                let rn = Self::gpr_arm_or_x86(src1)?;
                let rm = Self::gpr_arm_or_x86(*reg)?;
                if !n {
                    if opc == 0b00 && (rn == 31 || rm == 31) {
                        return self.emit_mov_imm(dst, 0, width);
                    }
                    if matches!(opc, 0b00 | 0b01) && rn == rm {
                        if width == OpWidth::W64 && dst == rn {
                            return Ok(());
                        }
                        return self.emit_mov_reg(dst, rn, width);
                    }
                    if opc == 0b10 && rn == rm {
                        return self.emit_mov_imm(dst, 0, width);
                    }
                    if matches!(opc, 0b01 | 0b10) && rn == 31 {
                        if width == OpWidth::W64 && dst == rm {
                            return Ok(());
                        }
                        return self.emit_mov_reg(dst, rm, width);
                    }
                    if matches!(opc, 0b01 | 0b10) && rm == 31 {
                        if width == OpWidth::W64 && dst == rn {
                            return Ok(());
                        }
                        return self.emit_mov_reg(dst, rn, width);
                    }
                }
                if n && opc == 0b00 {
                    if rn == 31 || rn == rm {
                        return self.emit_mov_imm(dst, 0, width);
                    }
                    if rm == 31 {
                        if width == OpWidth::W64 && dst == rn {
                            return Ok(());
                        }
                        return self.emit_mov_reg(dst, rn, width);
                    }
                }
            }
        }

        match src2 {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let imm = if n {
                    Self::inverted_logical_imm(*imm, width)?
                } else {
                    *imm
                };
                if self.lower_logic_special_imm(dst, src1, opc, set_flags, width, imm)? {
                    return Ok(());
                }
                let dst_reg = Self::dst_or_zero_for_flags_arm_or_x86(dst, set_flags)?;
                let rn = Self::gpr_arm_or_x86(src1)?;
                match Self::logical_bitmask_imm(imm, width) {
                    Ok((imm_n, immr, imms)) => {
                        self.emit_logic_imm(dst_reg, rn, opc, imm_n, immr, imms, width)
                    }
                    Err(LowerError::UnsupportedOp { .. }) => {
                        match self
                            .lower_materialized_logic_imm(dst, src1, opc, set_flags, width, imm)
                        {
                            Ok(()) => Ok(()),
                            Err(_) => self.emit_logic_imm_scratch(dst_reg, rn, opc, imm, width),
                        }
                    }
                    Err(err) => Err(err),
                }
            }
            _ => {
                let (src2, shift, amount) = Self::logical_src2(src2, width)?;
                self.emit_logic_shifted(
                    Self::dst_or_zero_for_flags_arm_or_x86(dst, set_flags)?,
                    Self::gpr_arm_or_x86(src1)?,
                    src2,
                    opc,
                    n,
                    shift,
                    amount,
                    width,
                )
            }
        }
    }

    pub(crate) fn lower_materialized_logic_imm(
        &mut self,
        dst: VReg,
        src1: VReg,
        opc: u32,
        set_flags: bool,
        width: OpWidth,
        imm: i64,
    ) -> Result<(), LowerError> {
        let dst = if set_flags && opc != 0b11 {
            Self::dst_gpr(dst)?
        } else {
            Self::dst_or_zero_for_flags(dst, set_flags)?
        };
        if dst == 31 {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native materialized logical immediate needs a destination scratch"
                    .into(),
            });
        }
        let rn = Self::gpr(src1)?;
        if dst == rn {
            if matches!(opc, 0b00 | 0b11) {
                let (_, value, all_ones) = Self::logical_imm_value(imm, width)?;
                let mut remaining = (!value) & all_ones;
                if remaining == 0 {
                    if opc == 0b11 {
                        return self.emit_logic_reg_n(31, dst, dst, 0b11, false, width);
                    }
                    return Ok(());
                }
                if remaining.count_ones() <= 3 {
                    while remaining != 0 {
                        let bit = remaining.trailing_zeros();
                        let chunk = all_ones ^ (1_u64 << bit);
                        let (imm_n, immr, imms) = Self::logical_bitmask_imm(chunk as i64, width)?;
                        self.emit_logic_imm(dst, dst, opc, imm_n, immr, imms, width)?;
                        remaining &= remaining - 1;
                    }
                    return Ok(());
                }
            }
            if !set_flags && matches!(opc, 0b01 | 0b10) {
                let (_, value, _) = Self::logical_imm_value(imm, width)?;
                if value.count_ones() <= 3 {
                    let mut remaining = value;
                    while remaining != 0 {
                        let bit = remaining.trailing_zeros();
                        let chunk = 1_u64 << bit;
                        let (imm_n, immr, imms) = Self::logical_bitmask_imm(chunk as i64, width)?;
                        self.emit_logic_imm(dst, dst, opc, imm_n, immr, imms, width)?;
                        remaining &= remaining - 1;
                    }
                    return Ok(());
                }
            }
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 native materialized logical immediate needs dst != src1".into(),
            });
        }
        self.emit_mov_imm_best(dst, imm, width)?;
        self.emit_logic_reg_n(dst, rn, dst, opc, false, width)
    }

    pub(crate) fn lower_logic_with_synth_flags(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        opc: u32,
        n: bool,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match width {
            OpWidth::W32 | OpWidth::W64 => {}
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native logical flags width {other:?}"),
                });
            }
        }

        let dst = Self::dst_or_zero_for_flags_arm_or_x86(dst, true)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let mut avoid = vec![rn];
        if dst != 31 {
            avoid.push(dst);
        }
        match src2 {
            SrcOperand::Reg(reg) | SrcOperand::Shifted { reg, .. } => {
                avoid.push(Self::gpr_arm_or_x86(*reg)?);
            }
            _ => {}
        }
        let scratches = Self::scratch_regs(&avoid, 3)?;
        let result = scratches[0];
        let flags = scratches[1];
        let temp = scratches[2];

        self.emit_scratch_save(&scratches);
        self.lower_logic(
            VReg::Arch(ArchReg::Arm(ArmReg::X(result))),
            src1,
            src2,
            opc,
            n,
            false,
            width,
        )?;
        if dst != 31 {
            self.emit_mov_reg(dst, result, width)?;
        }
        self.emit_init_shift_nz_flags(flags, temp, result, width)?;
        self.emit_sysreg(flags, ArmReg::Nzcv, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_logic_special_imm(
        &mut self,
        dst: VReg,
        src1: VReg,
        opc: u32,
        set_flags: bool,
        width: OpWidth,
        imm: i64,
    ) -> Result<bool, LowerError> {
        let (_, value, all_ones) = Self::logical_imm_value(imm, width)?;
        if value != 0 && value != all_ones {
            return Ok(false);
        }

        let dst = Self::dst_or_zero_for_flags_arm_or_x86(dst, set_flags)?;
        let rn = Self::gpr_arm_or_x86(src1)?;
        let identity = matches!(
            (opc, value == all_ones),
            (0b00, true) | (0b01, false) | (0b10, false)
        );
        if !set_flags && width == OpWidth::W64 && identity && dst == rn {
            return Ok(true);
        }

        match (opc, value == all_ones) {
            (0b00, false) => self.emit_mov_imm(dst, 0, width)?,
            (0b00, true) => self.emit_mov_reg(dst, rn, width)?,
            (0b01, false) | (0b10, false) => self.emit_mov_reg(dst, rn, width)?,
            (0b01, true) => self.emit_movn_zero(dst, width)?,
            (0b10, true) => self.emit_logic_reg_n(dst, 31, rn, 0b01, true, width)?,
            (0b11, false) => self.emit_logic_reg_n(dst, 31, 31, 0b11, false, width)?,
            (0b11, true) => self.emit_logic_reg_n(dst, rn, rn, 0b11, false, width)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(crate) fn emit_logic_imm_scratch(
        &mut self,
        dst: u8,
        rn: u8,
        opc: u32,
        imm: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let scratches = Self::scratch_regs(&[dst, rn], 1)?;
        let scratch = scratches[0];
        self.emit_scratch_save(&scratches);
        self.emit_mov_imm(scratch, imm, width)?;
        self.emit_logic_shifted(dst, rn, scratch, opc, false, 0, 0, width)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn inverted_logical_imm(imm: i64, width: OpWidth) -> Result<i64, LowerError> {
        match width {
            OpWidth::W32 => Ok((!(imm as u32)) as i64),
            OpWidth::W64 => Ok((!(imm as u64)) as i64),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native inverted logical immediate width {other:?}"),
            }),
        }
    }

    pub(crate) fn logical_bitmask_imm(
        imm: i64,
        width: OpWidth,
    ) -> Result<(u32, u32, u32), LowerError> {
        let (bits, value, all_ones) = Self::logical_imm_value(imm, width)?;
        if value != 0 && value != all_ones {
            for element_bits in [2_u32, 4, 8, 16, 32, 64] {
                if element_bits > bits {
                    break;
                }
                let element_mask = if element_bits == 64 {
                    u64::MAX
                } else {
                    (1_u64 << element_bits) - 1
                };
                for ones in 1..element_bits {
                    let low_mask = (1_u64 << ones) - 1;
                    for immr in 0..element_bits {
                        let element = if immr == 0 {
                            low_mask
                        } else {
                            ((low_mask >> immr) | (low_mask << (element_bits - immr)))
                                & element_mask
                        };
                        let mut mask = 0_u64;
                        let mut offset = 0;
                        while offset < bits {
                            mask |= element << offset;
                            offset += element_bits;
                        }
                        if mask == value {
                            let len = element_bits.trailing_zeros();
                            let n = if element_bits == 64 { 1 } else { 0 };
                            let imms = (ones - 1) | ((!0_u32 << (len + 1)) & 0x3f);
                            return Ok((n, immr, imms));
                        }
                    }
                }
            }
        }
        Err(LowerError::UnsupportedOp {
            op: format!("AArch64 native logical immediate {value:#x} for {width:?}"),
        })
    }

    pub(crate) fn logical_imm_value(
        imm: i64,
        width: OpWidth,
    ) -> Result<(u32, u64, u64), LowerError> {
        let bits = match width {
            OpWidth::W32 => 32,
            OpWidth::W64 => 64,
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native logical immediate width {other:?}"),
                });
            }
        };
        let value = match width {
            OpWidth::W32 => u64::from(imm as u32),
            OpWidth::W64 => imm as u64,
            _ => unreachable!(),
        };
        let all_ones = if bits == 64 {
            u64::MAX
        } else {
            (1_u64 << bits) - 1
        };
        Ok((bits, value, all_ones))
    }

    pub(crate) fn emit_logic_imm_mask(
        &mut self,
        dst: u8,
        rn: u8,
        opc: u32,
        mask: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let (imm_n, immr, imms) = Self::logical_bitmask_imm(mask, width)?;
        self.emit_logic_imm(dst, rn, opc, imm_n, immr, imms, width)
    }

    pub(crate) fn logical_src2(
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
                    ShiftOp::Ror => 3,
                    ShiftOp::Rrx => {
                        return Err(LowerError::UnsupportedOp {
                            op: "AArch64 native logical RRX source".into(),
                        });
                    }
                };
                if u32::from(*amount) >= bits {
                    return Err(LowerError::InvalidOperand {
                        op: "AArch64 logical shifted register".into(),
                        operand: format!("amount={amount}, width={width:?}"),
                    });
                }
                Ok((Self::gpr_arm_or_x86(*reg)?, shift, u32::from(*amount)))
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native logical source {other:?}"),
                });
            }
        }
    }

    pub(crate) fn emit_logic_flags_from_source(
        &mut self,
        src: u8,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match width {
            OpWidth::W32 | OpWidth::W64 => self.emit_logic_reg_n(31, src, src, 0b11, false, width),
            OpWidth::W8 | OpWidth::W16 => {
                let scratch = Self::scratch_regs(&[src], 1)?;
                let flag_reg = scratch[0];
                self.emit_scratch_save(&scratch);
                self.emit_bitfield(flag_reg, src, 0b10, 0, width.bits() - 1, OpWidth::W32)?;

                let nonzero = self.code.position();
                self.emit(0xb500_0000 | u32::from(flag_reg));
                self.emit_mov_imm(flag_reg, NZCV_Z, OpWidth::W32)?;
                let end_zero = self.code.position();
                self.emit(0x1400_0000);

                self.patch_compare_branch_to_current(nonzero, flag_reg, true)?;
                let sign_set = self.code.position();
                self.emit_test_branch(flag_reg, width.bits() - 1, true, 0)?;
                self.emit_mov_imm(flag_reg, 0, OpWidth::W32)?;
                let end_clear = self.code.position();
                self.emit(0x1400_0000);

                self.patch_test_branch_to_current(sign_set, flag_reg, width.bits() - 1, true)?;
                self.emit_mov_imm(flag_reg, NZCV_N, OpWidth::W32)?;
                self.patch_branch_to_current(end_zero)?;
                self.patch_branch_to_current(end_clear)?;
                self.emit_sysreg(flag_reg, ArmReg::Nzcv, false)?;
                self.emit_scratch_restore(&scratch);
                Ok(())
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native bit-scan flag width {other:?}"),
            }),
        }
    }

    pub(crate) fn vector_inverted_logic_sources(
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        inverted: VReg,
        logic_op: SimdLogicOp,
    ) -> Option<(VReg, VReg, VecWidth, SimdLogicOp)> {
        if src2 == inverted {
            Some((dst, src1, width, logic_op))
        } else if src1 == inverted {
            Some((dst, src2, width, logic_op))
        } else {
            None
        }
    }

    pub(crate) fn lower_logic_flag_contract(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: &SrcOperand,
        opc_without_flags: u32,
        n: bool,
        flags: FlagUpdate,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let partial_nz = FlagSet::SF.union(FlagSet::ZF);
        if flags == FlagUpdate::Specific(partial_nz) {
            return self.lower_with_selected_nzcv(partial_nz, |lowerer| {
                let opc = if opc_without_flags == 0b00 {
                    0b11
                } else {
                    opc_without_flags
                };
                lowerer.lower_logic(dst, src1, src2, opc, n, true, width)
            });
        }

        let set_flags = flags.updates_any();
        let opc = if set_flags && opc_without_flags == 0b00 {
            0b11
        } else {
            opc_without_flags
        };
        self.lower_logic(dst, src1, src2, opc, n, set_flags, width)
    }
}
