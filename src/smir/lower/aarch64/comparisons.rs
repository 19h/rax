//! Flag-only scalar comparisons and architectural NZCV lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, ArmReg, OpWidth, ShiftOp, SrcOperand, VReg};
use crate::smir::lower::LowerError;

use super::Aarch64Lowerer;

/// Admit CMN only when its operands fit the AArch64 identity-register ABI and
/// the flag-setting ADD encoding. Generic JIT admission does not include CMN.
pub(crate) fn aarch64_cmn_shape_valid(op: &SmirOp) -> bool {
    let OpKind::Cmn { src1, src2, width } = &op.kind else {
        return false;
    };
    if op.x86_hint.is_some() || !matches!(width, OpWidth::W32 | OpWidth::W64) {
        return false;
    }

    let source_register_valid = |reg: VReg| {
        matches!(reg, VReg::Imm(0))
            || matches!(
                reg,
                VReg::Arch(ArchReg::Arm(ArmReg::X(index)))
                    if index < 31 && !matches!(index, 18 | 28 | 30)
            )
    };
    if !source_register_valid(*src1) {
        return false;
    }

    match src2 {
        SrcOperand::Reg(reg) => source_register_valid(*reg),
        SrcOperand::Shifted { reg, shift, amount } => {
            source_register_valid(*reg)
                && matches!(shift, ShiftOp::Lsl | ShiftOp::Lsr | ShiftOp::Asr)
                && u32::from(*amount) < width.bits()
        }
        SrcOperand::Extended { reg, shift, .. } => source_register_valid(*reg) && *shift <= 4,
        SrcOperand::Imm(value) | SrcOperand::Imm64(value) => {
            (0..=0xfff).contains(value) || ((0..=0xfff000).contains(value) && value & 0xfff == 0)
        }
    }
}

impl Aarch64Lowerer {
    pub(crate) fn lower_cmn(
        &mut self,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(left) = src1 {
            if let SrcOperand::Imm(right) | SrcOperand::Imm64(right) = src2 {
                let lhs = left as u64 & width.mask();
                let rhs = *right as u64 & width.mask();
                let sum = u128::from(lhs) + u128::from(rhs);
                let result = sum as u64 & width.mask();
                let n = u32::from(result & width.sign_bit() != 0);
                let z = u32::from(result == 0);
                let c = u32::from(sum > u128::from(width.mask()));
                let v = u32::from(!(lhs ^ rhs) & (lhs ^ result) & width.sign_bit() != 0);
                let nzcv = (n << 3) | (z << 2) | (c << 1) | v;
                return self.lower_constant_cmp_nzcv(nzcv, width);
            }
        }

        // The shared flag-setting ADD path maps a discarded virtual result to
        // Rd=31 (WZR/XZR). CMN itself defines no virtual or guest register.
        self.lower_addsub(VReg::virt(0), src1, src2, false, true, width)
    }

    pub(crate) fn lower_cmp(
        &mut self,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if let VReg::Imm(left) = src1 {
            if let SrcOperand::Imm(right) | SrcOperand::Imm64(right) = src2 {
                let nzcv = Self::constant_sub_nzcv(left, *right, width)?;
                return self.lower_constant_cmp_nzcv(nzcv, width);
            }
        }

        if src1 == VReg::Imm(0) && Self::src_operand_is_zero(src2) {
            let nzcv = Self::constant_sub_nzcv(0, 0, width)?;
            return self.lower_constant_cmp_nzcv(nzcv, width);
        }

        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            return self.lower_subword_addsub_with_flags(VReg::virt(0), src1, src2, true, width);
        }

        if src1 == VReg::Imm(0) {
            if Self::src_operand_is_zero(src2) {
                let nzcv = Self::constant_sub_nzcv(0, 0, width)?;
                return self.lower_constant_cmp_nzcv(nzcv, width);
            }
            match src2 {
                SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                    let is_zero = match width {
                        OpWidth::W32 => *imm as u32 == 0,
                        OpWidth::W64 => *imm == 0,
                        _ => false,
                    };
                    if is_zero {
                        return self.emit_addsub_reg(31, 31, 31, true, true, width);
                    }
                    if matches!(width, OpWidth::W32 | OpWidth::W64) {
                        let value = match width {
                            OpWidth::W32 => u64::from(*imm as u32),
                            OpWidth::W64 => *imm as u64,
                            _ => unreachable!(),
                        };
                        if (value & width.sign_bit()) != 0 && value != width.sign_bit() {
                            return self.emit_sysreg(31, ArmReg::Nzcv, false);
                        }
                    }
                    return Err(LowerError::UnsupportedOp {
                        op: "AArch64 native CMP zero base with nonzero immediate".into(),
                    });
                }
                SrcOperand::Reg(_) | SrcOperand::Shifted { .. } => {
                    if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("AArch64 native CMP zero base source width {width:?}"),
                        });
                    }
                    let (rm, shift, amount) = Self::addsub_src2(src2, width)?;
                    return self.emit_addsub_shifted(31, 31, rm, true, true, shift, amount, width);
                }
                SrcOperand::Extended { .. } => {
                    let (rm, option, amount) = Self::addsub_ext_src2(src2)?;
                    return self.emit_zero_base_extended_flags(rm, option, amount, true, width);
                }
                _ => {}
            }
        }

        let rn = Self::gpr_arm_or_x86(src1)?;
        match src2 {
            SrcOperand::Reg(_) | SrcOperand::Shifted { .. } => {
                let (rm, shift, amount) = Self::addsub_src2(src2, width)?;
                self.emit_addsub_shifted(31, rn, rm, true, true, shift, amount, width)
            }
            SrcOperand::Extended { .. } => {
                let (rm, option, amount) = Self::addsub_ext_src2(src2)?;
                self.emit_addsub_extended(31, rn, rm, true, true, option, amount, width)
            }
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let (subtract, imm) =
                    Self::canonical_addsub_imm(*imm, true, width).unwrap_or((true, *imm));
                self.emit_addsub_imm(31, rn, imm, subtract, true, width)
            }
            other => Err(LowerError::UnsupportedOp {
                op: format!("AArch64 native CMP source {other:?}"),
            }),
        }
    }

    pub(crate) fn lower_test(
        &mut self,
        src1: VReg,
        src2: &SrcOperand,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let zero_operand = match width {
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64 => {
                let mask = width.mask();
                matches!(src1, VReg::Imm(value) if (value as u64 & mask) == 0)
                    || matches!(
                        src2,
                        SrcOperand::Imm(value) | SrcOperand::Imm64(value)
                            if (*value as u64 & mask) == 0
                    )
            }
            _ => false,
        };
        if zero_operand {
            let emit_width = if width == OpWidth::W64 {
                OpWidth::W64
            } else {
                OpWidth::W32
            };
            return self.emit_logic_reg_n(31, 31, 31, 0b11, false, emit_width);
        }
        if let VReg::Imm(left) = src1 {
            if let SrcOperand::Imm(right) | SrcOperand::Imm64(right) = src2 {
                if matches!(
                    width,
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
                ) {
                    let result = (left as u64 & width.mask()) & (*right as u64 & width.mask());
                    return self.lower_constant_test_result(result, width);
                }
            }
        }
        if matches!(width, OpWidth::W8 | OpWidth::W16) {
            return self.lower_subword_logic_with_flags(
                VReg::virt(0),
                src1,
                src2,
                0b00,
                false,
                width,
            );
        }
        if matches!(width, OpWidth::W32 | OpWidth::W64) {
            if let VReg::Imm(value) = src1 {
                if (value as u64 & width.mask()) == width.mask() {
                    // 0 + transformed src sets N/Z from the transformed src and clears C/V.
                    match src2 {
                        SrcOperand::Reg(src) => {
                            let src = Self::gpr_arm_or_x86(*src)?;
                            return self.emit_logic_reg_n(31, src, src, 0b11, false, width);
                        }
                        SrcOperand::Shifted { .. } => {
                            let (src, shift, amount) = Self::addsub_src2(src2, width)?;
                            return self.emit_addsub_shifted(
                                31, 31, src, false, true, shift, amount, width,
                            );
                        }
                        SrcOperand::Extended { .. } => {
                            let (src, option, amount) = Self::addsub_ext_src2(src2)?;
                            return self
                                .emit_zero_base_extended_flags(src, option, amount, false, width);
                        }
                        _ => {}
                    }
                }
            }
        }

        match src2 {
            SrcOperand::Imm(imm) | SrcOperand::Imm64(imm) => {
                let (_, value, all_ones) = Self::logical_imm_value(*imm, width)?;
                if value == 0 {
                    return self.emit_logic_reg_n(31, 31, 31, 0b11, false, width);
                }
                if value == all_ones {
                    let rn = Self::gpr_arm_or_x86(src1)?;
                    return self.emit_logic_reg_n(31, rn, rn, 0b11, false, width);
                }
                let rn = Self::gpr_arm_or_x86(src1)?;
                match Self::logical_bitmask_imm(*imm, width) {
                    Ok((n, immr, imms)) => self.emit_logic_imm(31, rn, 0b11, n, immr, imms, width),
                    Err(LowerError::UnsupportedOp { .. }) => {
                        self.emit_logic_imm_scratch(31, rn, 0b11, *imm, width)
                    }
                    Err(err) => Err(err),
                }
            }
            _ => {
                let (src2, shift, amount) = Self::logical_src2(src2, width)?;
                self.emit_logic_shifted(
                    31,
                    Self::gpr_arm_or_x86(src1)?,
                    src2,
                    0b11,
                    false,
                    shift,
                    amount,
                    width,
                )
            }
        }
    }
}
