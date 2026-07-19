//! System-register and cross-arch guest lowering

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
    pub(crate) fn gpr_arm_or_x86(vreg: VReg) -> Result<u8, LowerError> {
        match vreg {
            VReg::Arch(ArchReg::Arm(ArmReg::X(n))) if n < 30 => Ok(n),
            // The mixed native/x86 paths identity-map guest registers to host Xn.
            // Reject X/R30 (host X30 = the link register the region's `RET`
            // branches through) and X/R31 (SP/XZR): both are reserved and not
            // guest-state-backed, so mapping a guest operand onto them would
            // corrupt the return address / SP. Such ops fall back to the
            // interpreter. (#61)
            VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().filter(|&n| n < 30).ok_or_else(|| {
                LowerError::InvalidRegister(format!(
                    "AArch64 native lowerer expected GPR, got X86({reg:?})"
                ))
            }),
            VReg::Imm(0) => Ok(31),
            other => Err(LowerError::InvalidRegister(format!(
                "AArch64 native lowerer expected GPR, got {other:?}"
            ))),
        }
    }

    pub(crate) fn dst_gpr_arm_or_x86(vreg: VReg) -> Result<u8, LowerError> {
        match vreg {
            VReg::Arch(ArchReg::Arm(ArmReg::X(n))) if n < 30 => Ok(n),
            // Reject X/R30 (host X30 = link register) and X/R31 (SP/XZR): reserved
            // host registers, not guest-state-backed. Writing a guest value onto X30
            // would overwrite the native return target used by the region's `RET`. (#61)
            VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().filter(|&n| n < 30).ok_or_else(|| {
                LowerError::InvalidRegister(format!(
                    "AArch64 native lowerer expected writable GPR, got X86({reg:?})"
                ))
            }),
            other => Err(LowerError::InvalidRegister(format!(
                "AArch64 native lowerer expected writable GPR, got {other:?}"
            ))),
        }
    }

    pub(crate) fn dst_or_zero_for_flags_arm_or_x86(
        vreg: VReg,
        set_flags: bool,
    ) -> Result<u8, LowerError> {
        match vreg {
            VReg::Virtual(_) if set_flags => Ok(31),
            other => Self::dst_gpr_arm_or_x86(other),
        }
    }

    pub(crate) fn emit_sysreg(
        &mut self,
        rt: u8,
        reg: ArmReg,
        read: bool,
    ) -> Result<(), LowerError> {
        let Some(info) = Self::sysreg_info(reg) else {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native system register {reg:?}"),
            });
        };
        self.emit(
            0xd500_0000
                | ((read as u32) << 21)
                | (3 << 19)
                | (info.op1 << 16)
                | (info.crn << 12)
                | (info.crm << 8)
                | (info.op2 << 5)
                | u32::from(rt),
        );
        Ok(())
    }

    pub(crate) fn lower_sysreg_read(
        &mut self,
        dst: VReg,
        reg: ArmReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        Self::validate_sysreg_width("MRS", width)?;
        self.emit_sysreg(Self::dst_gpr(dst)?, reg, true)
    }

    pub(crate) fn lower_sysreg_write(
        &mut self,
        reg: ArmReg,
        src: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        Self::validate_sysreg_width("MSR", width)?;
        let rt = match src {
            SrcOperand::Reg(src) => Self::gpr(*src)?,
            SrcOperand::Imm(0) | SrcOperand::Imm64(0) => 31,
            SrcOperand::Imm(value) | SrcOperand::Imm64(value) => {
                return self.lower_sysreg_write_imm(reg, *value, width);
            }
            other => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native system register write source {other:?}"),
                });
            }
        };
        self.emit_sysreg(rt, reg, false)
    }

    pub(crate) fn lower_sysreg_write_imm(
        &mut self,
        reg: ArmReg,
        value: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if value == 0 {
            return self.emit_sysreg(31, reg, false);
        }

        let scratches = Self::scratch_regs(&[], 1)?;
        let rt = scratches[0];
        self.emit_scratch_save(&scratches);
        self.emit_mov_imm(rt, value, width)?;
        self.emit_sysreg(rt, reg, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_raw_sysreg_read(&mut self, dst: VReg, reg: u32) -> Result<(), LowerError> {
        let Some(reg) = Self::raw_sysreg(reg) else {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native MRS sysreg {reg:#06x}"),
            });
        };
        self.emit_sysreg(Self::dst_gpr(dst)?, reg, true)
    }

    pub(crate) fn lower_raw_sysreg_write(&mut self, reg: u32, src: VReg) -> Result<(), LowerError> {
        let Some(reg) = Self::raw_sysreg(reg) else {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native MSR sysreg {reg:#06x}"),
            });
        };
        if let VReg::Imm(value) = src {
            let Some(info) = Self::sysreg_info(reg) else {
                return Err(LowerError::UnsupportedOp {
                    op: format!("AArch64 native MSR sysreg {reg:?}"),
                });
            };
            return self.lower_sysreg_write_imm(reg, value, info.write_width);
        }

        self.emit_sysreg(Self::gpr(src)?, reg, false)
    }

    pub(crate) fn lower_x86_count(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86CountKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let op = match kind {
            X86CountKind::Popcnt => "AArch64 native X86Count::Popcnt",
            X86CountKind::Tzcnt => "AArch64 native X86Count::Tzcnt",
            X86CountKind::Lzcnt => "AArch64 native X86Count::Lzcnt",
        };
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: op.into(),
                operand: format!("unsupported width {width:?}"),
            });
        }

        let requested = flags.as_set();
        let defined = match kind {
            X86CountKind::Popcnt => FlagSet::ALL_X86,
            X86CountKind::Tzcnt | X86CountKind::Lzcnt => FlagSet::CF.union(FlagSet::ZF),
        };
        if !requested.difference(defined).is_empty() {
            return Err(LowerError::InvalidOperand {
                op: op.into(),
                operand: format!("unsupported flag update {flags:?}"),
            });
        }
        if matches!(src, VReg::Imm(_)) {
            return Err(LowerError::InvalidOperand {
                op: op.into(),
                operand: "immediate source".into(),
            });
        }

        let lower_count = |this: &mut Self, source: VReg| match kind {
            X86CountKind::Popcnt => this.lower_popcnt(dst, source, width),
            X86CountKind::Tzcnt => this.lower_ctz(dst, source, width),
            X86CountKind::Lzcnt => this.lower_clz(dst, source, width),
        };
        let mut output_mask = 0_i64;
        if requested.contains(FlagSet::SF) {
            output_mask |= NZCV_N;
        }
        if requested.contains(FlagSet::ZF) {
            output_mask |= NZCV_Z;
        }
        if requested.contains(FlagSet::CF) {
            output_mask |= NZCV_C;
        }
        if requested.contains(FlagSet::OF) {
            output_mask |= NZCV_V;
        }
        if output_mask == 0 {
            return lower_count(self, src);
        }

        let dst_reg = Self::dst_gpr_arm_or_x86(dst)?;
        let src_reg = Self::gpr_arm_or_x86(src)?;
        let scratches = Self::scratch_regs(&[dst_reg, src_reg], 3)?;
        let original = scratches[0];
        let saved_flags = scratches[1];
        let produced = scratches[2];
        self.emit_scratch_save(&scratches);
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;
        match width {
            OpWidth::W16 => {
                self.emit_bitfield(original, src_reg, 0b10, 0, 15, OpWidth::W32)?;
            }
            OpWidth::W32 | OpWidth::W64 => {
                self.emit_mov_reg(original, src_reg, width)?;
            }
            _ => unreachable!("x86 count width already validated"),
        }
        lower_count(self, Self::arm_x_reg(original))?;
        self.emit_mov_imm(produced, 0, OpWidth::W32)?;

        match kind {
            X86CountKind::Popcnt => {
                if requested.contains(FlagSet::ZF) {
                    let skip_zero = self.code.position();
                    self.emit(0xb500_0000 | u32::from(original));
                    self.emit_logic_imm_mask(produced, produced, 0b01, NZCV_Z, OpWidth::W32)?;
                    self.patch_compare_branch_to_current(skip_zero, original, true)?;
                }
            }
            X86CountKind::Tzcnt | X86CountKind::Lzcnt => {
                if requested.contains(FlagSet::CF) {
                    let skip_zero = self.code.position();
                    self.emit(0xb500_0000 | u32::from(original));
                    self.emit_logic_imm_mask(produced, produced, 0b01, NZCV_C, OpWidth::W32)?;
                    self.patch_compare_branch_to_current(skip_zero, original, true)?;
                }
                if requested.contains(FlagSet::ZF) {
                    let bit = if kind == X86CountKind::Tzcnt {
                        0
                    } else {
                        width.bits() - 1
                    };
                    let skip_clear = self.code.position();
                    self.emit_test_branch(original, bit, false, 0)?;
                    self.emit_logic_imm_mask(produced, produced, 0b01, NZCV_Z, OpWidth::W32)?;
                    self.patch_test_branch_to_current(skip_clear, original, bit, false)?;
                }
            }
        }

        self.emit_logic_imm_mask(
            saved_flags,
            saved_flags,
            0b00,
            !(output_mask as u32) as i64,
            OpWidth::W32,
        )?;
        self.emit_logic_imm_mask(produced, produced, 0b00, output_mask, OpWidth::W32)?;
        self.emit_logic_shifted(
            saved_flags,
            saved_flags,
            produced,
            0b01,
            false,
            0,
            0,
            OpWidth::W32,
        )?;
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_x86_bls(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86BlsKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let defined_flags = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        let set_flags = match flags {
            FlagUpdate::None => false,
            FlagUpdate::Specific(set) if set == defined_flags => true,
            other => {
                return Err(LowerError::InvalidOperand {
                    op: "AArch64 native X86Bls".into(),
                    operand: format!("flag contract {other:?}"),
                });
            }
        };
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native X86Bls width {width:?}"),
            });
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let src = Self::gpr_arm_or_x86(src)?;
        let scratches = Self::scratch_regs(&[dst, src], 2)?;
        let original = scratches[0];
        let transformed = scratches[1];
        self.emit_scratch_save(&scratches);

        // Preserve the source independently of destination aliasing. The saved
        // value also drives the dynamic x86 CF definition after the result has
        // replaced an aliased source register.
        self.emit_mov_reg(original, src, width)?;
        match kind {
            X86BlsKind::Blsr | X86BlsKind::Blsmsk => {
                self.emit_addsub_imm(transformed, original, 1, true, false, width)?;
                self.emit_logic_reg_n(
                    dst,
                    original,
                    transformed,
                    if kind == X86BlsKind::Blsr { 0b00 } else { 0b10 },
                    false,
                    width,
                )?;
            }
            X86BlsKind::Blsi => {
                self.emit_addsub_reg(transformed, 31, original, true, false, width)?;
                self.emit_logic_reg_n(dst, original, transformed, 0b00, false, width)?;
            }
        }

        if set_flags {
            // ANDS XZR/WZR,result,result establishes x86 SF/ZF and clears C/V.
            // BLSR/BLSMSK then set CF iff src==0; BLSI sets CF iff src!=0.
            self.emit_logic_reg_n(31, dst, dst, 0b11, false, width)?;
            let skip_carry_set = self.code.position();
            let skip_when_nonzero = !matches!(kind, X86BlsKind::Blsi);
            self.emit(
                if skip_when_nonzero {
                    0xb500_0000
                } else {
                    0xb400_0000
                } | u32::from(original),
            );
            self.lower_cfinv()?;
            self.patch_compare_branch_to_current(skip_carry_set, original, skip_when_nonzero)?;
        }

        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn lower_x86_adx(
        &mut self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: OpWidth,
        kind: X86AdxKind,
        flags: FlagUpdate,
    ) -> Result<(), LowerError> {
        let selected_flag = match kind {
            X86AdxKind::Adcx => FlagSet::CF,
            X86AdxKind::Adox => FlagSet::OF,
        };
        let set_flags = match flags {
            FlagUpdate::None => false,
            FlagUpdate::Specific(set) if set == selected_flag => true,
            other => {
                return Err(LowerError::InvalidOperand {
                    op: "AArch64 native X86Adx".into(),
                    operand: format!("flag contract {other:?}"),
                });
            }
        };
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native X86Adx width {width:?}"),
            });
        }
        if matches!(src1, VReg::Imm(_)) || matches!(src2, VReg::Imm(_)) {
            return Err(LowerError::InvalidOperand {
                op: "AArch64 native X86Adx".into(),
                operand: "immediate carry-chain source".into(),
            });
        }

        let dst = Self::dst_gpr_arm_or_x86(dst)?;
        let src1 = Self::gpr_arm_or_x86(src1)?;
        let src2 = Self::gpr_arm_or_x86(src2)?;
        let scratches = Self::scratch_regs(&[dst, src1, src2], 3)?;
        let saved_flags = scratches[0];
        let work = scratches[1];
        let temp = scratches[2];
        self.emit_scratch_save(&scratches);
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, true)?;

        if kind == X86AdxKind::Adox {
            // AArch64 ADC consumes C. Re-map the saved x86 OF representation
            // (NZCV.V) into C without changing the saved snapshot used below.
            self.emit_logic_imm_mask(
                work,
                saved_flags,
                0b00,
                !(NZCV_C as u32) as i64,
                OpWidth::W32,
            )?;
            self.emit_logic_imm_mask(temp, saved_flags, 0b00, NZCV_V, OpWidth::W32)?;
            self.lower_shift_imm(temp, temp, 1, ShiftOp::Lsl, OpWidth::W32)?;
            self.emit_logic_shifted(work, work, temp, 0b01, false, 0, 0, OpWidth::W32)?;
            self.emit_sysreg(work, ArmReg::Nzcv, false)?;
        }

        self.emit_addsub_carry(dst, src1, src2, false, set_flags, width)?;
        if !set_flags {
            self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
            self.emit_scratch_restore(&scratches);
            return Ok(());
        }

        // ADCS places the unsigned carry-out in C. Merge only the selected x86
        // chain output into the pre-op NZCV snapshot: ADCX replaces C, whereas
        // ADOX shifts C down one bit to replace V. N/Z and the other chain are
        // bit-for-bit preserved.
        self.emit_sysreg(work, ArmReg::Nzcv, true)?;
        self.emit_logic_imm_mask(work, work, 0b00, NZCV_C, OpWidth::W32)?;
        let output_mask = if kind == X86AdxKind::Adcx {
            NZCV_C
        } else {
            self.lower_shift_imm(work, work, 1, ShiftOp::Lsr, OpWidth::W32)?;
            NZCV_V
        };
        self.emit_logic_imm_mask(
            saved_flags,
            saved_flags,
            0b00,
            !(output_mask as u32) as i64,
            OpWidth::W32,
        )?;
        self.emit_logic_shifted(
            saved_flags,
            saved_flags,
            work,
            0b01,
            false,
            0,
            0,
            OpWidth::W32,
        )?;
        self.emit_sysreg(saved_flags, ArmReg::Nzcv, false)?;
        self.emit_scratch_restore(&scratches);
        Ok(())
    }

    pub(crate) fn sysreg_vreg(vreg: VReg) -> Option<ArmReg> {
        match vreg {
            VReg::Arch(ArchReg::Arm(reg @ (ArmReg::Nzcv | ArmReg::Fpcr | ArmReg::Fpsr))) => {
                Some(reg)
            }
            _ => None,
        }
    }

    pub(crate) fn raw_sysreg(reg: u32) -> Option<ArmReg> {
        match reg {
            SYSREG_NZCV => Some(ArmReg::Nzcv),
            SYSREG_FPCR => Some(ArmReg::Fpcr),
            SYSREG_FPSR => Some(ArmReg::Fpsr),
            _ => None,
        }
    }

    pub(crate) fn sysreg_info(reg: ArmReg) -> Option<SysRegInfo> {
        match reg {
            ArmReg::Nzcv => Some(SysRegInfo {
                op1: 3,
                crn: 4,
                crm: 2,
                op2: 0,
                mask: NZCV_MASK,
                read_width: OpWidth::W32,
                write_width: OpWidth::W32,
            }),
            ArmReg::Fpcr => Some(SysRegInfo {
                op1: 3,
                crn: 4,
                crm: 4,
                op2: 0,
                mask: FPCR_SYSREG_MASK,
                read_width: OpWidth::W64,
                write_width: OpWidth::W64,
            }),
            ArmReg::Fpsr => Some(SysRegInfo {
                op1: 3,
                crn: 4,
                crm: 4,
                op2: 1,
                mask: FPSR_SYSREG_MASK,
                read_width: OpWidth::W64,
                write_width: OpWidth::W64,
            }),
            _ => None,
        }
    }

    pub(crate) fn validate_sysreg_width(op: &str, width: OpWidth) -> Result<(), LowerError> {
        match width {
            OpWidth::W32 | OpWidth::W64 => Ok(()),
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native {op} width {other:?}"),
            }),
        }
    }
}
