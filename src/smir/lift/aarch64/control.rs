//! control.rs

use crate::smir::lift::aarch64::*;
use std::collections::HashSet;

use crate::isa::arm::decoder::{
    AddressingMode, Condition as ArmCondition, DecodedInsn, ExtendType, FpRegSize, FpRegister,
    MemOffset, MemOperand, Mnemonic, Operand, Register, ShiftType,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader};

impl Aarch64Lifter {
    /// Convert ARM condition to SMIR condition
    pub(crate) fn arm_cond(&self, cond: ArmCondition) -> Condition {
        match cond {
            ArmCondition::EQ => Condition::Eq,
            ArmCondition::NE => Condition::Ne,
            ArmCondition::CS => Condition::Uge,
            ArmCondition::CC => Condition::Ult,
            ArmCondition::MI => Condition::Negative,
            ArmCondition::PL => Condition::Positive,
            ArmCondition::VS => Condition::Overflow,
            ArmCondition::VC => Condition::NoOverflow,
            ArmCondition::HI => Condition::Ugt,
            ArmCondition::LS => Condition::Ule,
            ArmCondition::GE => Condition::Sge,
            ArmCondition::LT => Condition::Slt,
            ArmCondition::GT => Condition::Sgt,
            ArmCondition::LE => Condition::Sle,
            ArmCondition::AL | ArmCondition::NV => Condition::Always,
        }
    }

    pub(crate) fn lift_cond_compare(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let invalid = || LiftError::Internal("invalid conditional compare operands".to_string());
        let (rn, op2, nzcv, cond) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
            insn.operands.get(3),
        ) {
            (
                Some(Operand::Reg(rn)),
                Some(op2),
                Some(Operand::Imm(nzcv)),
                Some(Operand::Cond(cond)),
            ) => (rn, op2, nzcv, cond),
            _ => return Err(invalid()),
        };

        let cond_reg = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::TestCondition {
                dst: cond_reg,
                cond: self.arm_cond(*cond),
            },
        );

        let cmp_result = ctx.alloc_vreg();
        let width = self.reg_width(rn);
        let src2 = self.operand_to_src(op2, ctx)?;
        let cmp_op = if insn.mnemonic == Mnemonic::CCMN {
            OpKind::Add {
                dst: cmp_result,
                src1: self.arm_reg(rn),
                src2,
                width,
                flags: FlagUpdate::All,
            }
        } else {
            OpKind::Sub {
                dst: cmp_result,
                src1: self.arm_reg(rn),
                src2,
                width,
                flags: FlagUpdate::All,
            }
        };
        Self::push_lifted_op(ops, pc, cmp_op);

        let cmp_nzcv = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Mov {
                dst: cmp_nzcv,
                src: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::Nzcv))),
                width: OpWidth::W32,
            },
        );

        let final_nzcv = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Select {
                dst: final_nzcv,
                cond: cond_reg,
                src_true: cmp_nzcv,
                src_false: VReg::Imm((nzcv.value & 0xF) << 28),
                width: OpWidth::W32,
            },
        );
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
                src: SrcOperand::Reg(final_nzcv),
                width: OpWidth::W32,
            },
        );

        Ok(())
    }

    pub(crate) fn lift_cond_select(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let invalid = || LiftError::Internal("invalid conditional select operands".to_string());

        // Canonical CS* mnemonics transform the false operand. Alias mnemonics
        // carry the user-visible inverted condition, so they transform on true.
        let (rd, src_true_base, transform_base, transform_op, cond, transform_on_true) = match insn
            .mnemonic
        {
            Mnemonic::CSEL | Mnemonic::CSINC | Mnemonic::CSINV | Mnemonic::CSNEG => {
                let (rd, rn, rm, cond) = match (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                    insn.operands.get(3),
                ) {
                    (
                        Some(Operand::Reg(rd)),
                        Some(Operand::Reg(rn)),
                        Some(Operand::Reg(rm)),
                        Some(Operand::Cond(cond)),
                    ) => (*rd, *rn, *rm, *cond),
                    _ => return Err(invalid()),
                };
                let false_op = match insn.mnemonic {
                    Mnemonic::CSEL => CondSelectFalseOp::Identity,
                    Mnemonic::CSINC => CondSelectFalseOp::Increment,
                    Mnemonic::CSINV => CondSelectFalseOp::Invert,
                    Mnemonic::CSNEG => CondSelectFalseOp::Negate,
                    _ => unreachable!(),
                };
                (
                    rd,
                    self.arm_reg(&rn),
                    self.arm_reg(&rm),
                    false_op,
                    cond,
                    false,
                )
            }
            Mnemonic::CINC | Mnemonic::CINV | Mnemonic::CNEG => {
                let (rd, rn, cond) = match (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Cond(cond))) => {
                        (*rd, *rn, *cond)
                    }
                    _ => return Err(invalid()),
                };
                let false_op = match insn.mnemonic {
                    Mnemonic::CINC => CondSelectFalseOp::Increment,
                    Mnemonic::CINV => CondSelectFalseOp::Invert,
                    Mnemonic::CNEG => CondSelectFalseOp::Negate,
                    _ => unreachable!(),
                };
                (
                    rd,
                    self.arm_reg(&rn),
                    self.arm_reg(&rn),
                    false_op,
                    cond,
                    true,
                )
            }
            Mnemonic::CSET | Mnemonic::CSETM => {
                let (rd, cond) = match (insn.operands.get(0), insn.operands.get(1)) {
                    (Some(Operand::Reg(rd)), Some(Operand::Cond(cond))) => (*rd, *cond),
                    _ => return Err(invalid()),
                };
                let false_op = if insn.mnemonic == Mnemonic::CSET {
                    CondSelectFalseOp::Increment
                } else {
                    CondSelectFalseOp::Invert
                };
                (rd, VReg::Imm(0), VReg::Imm(0), false_op, cond, true)
            }
            _ => return Err(invalid()),
        };

        let dst = self.dst_reg(&rd, ctx);
        let width = self.reg_width(&rd);
        let cmp = ctx.alloc_vreg();

        Self::push_lifted_op(
            ops,
            pc,
            OpKind::TestCondition {
                dst: cmp,
                cond: self.arm_cond(cond),
            },
        );

        let transformed = match transform_op {
            CondSelectFalseOp::Identity => transform_base,
            CondSelectFalseOp::Increment => {
                let tmp = ctx.alloc_vreg();
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Add {
                        dst: tmp,
                        src1: transform_base,
                        src2: SrcOperand::Imm(1),
                        width,
                        flags: FlagUpdate::None,
                    },
                );
                tmp
            }
            CondSelectFalseOp::Invert => {
                let tmp = ctx.alloc_vreg();
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Not {
                        dst: tmp,
                        src: transform_base,
                        width,
                    },
                );
                tmp
            }
            CondSelectFalseOp::Negate => {
                let tmp = ctx.alloc_vreg();
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Neg {
                        dst: tmp,
                        src: transform_base,
                        width,
                        flags: FlagUpdate::None,
                    },
                );
                tmp
            }
        };
        let (src_true, src_false) = if transform_on_true {
            (transformed, src_true_base)
        } else {
            (src_true_base, transformed)
        };

        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Select {
                dst,
                cond: cmp,
                src_true,
                src_false,
                width,
            },
        );

        Ok(())
    }
}
