//! system.rs

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
    pub(crate) fn supported_sysreg(sysreg: u16) -> Option<SysRegAccess> {
        match sysreg {
            SYSREG_NZCV => Some(SysRegAccess {
                reg: ArmReg::Nzcv,
                mask: NZCV_MASK,
                read_width: OpWidth::W32,
                write_width: OpWidth::W32,
            }),
            SYSREG_FPCR => Some(SysRegAccess {
                reg: ArmReg::Fpcr,
                mask: FPCR_SYSREG_MASK,
                read_width: OpWidth::W64,
                write_width: OpWidth::W64,
            }),
            SYSREG_FPSR => Some(SysRegAccess {
                reg: ArmReg::Fpsr,
                mask: FPSR_SYSREG_MASK,
                read_width: OpWidth::W64,
                write_width: OpWidth::W64,
            }),
            _ => None,
        }
    }

    pub(crate) fn lift_mrs(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rd, sysreg) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::Reg(rd)), Some(Operand::SysReg(sysreg))) => (rd, sysreg),
            _ => return Err(LiftError::Internal("invalid MRS operands".to_string())),
        };
        let Some(access) = Self::supported_sysreg(*sysreg) else {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!("MRS sysreg {sysreg:#06x}"),
            });
        };

        let masked = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: masked,
                src1: VReg::Arch(ArchReg::Arm(access.reg)),
                src2: SrcOperand::Imm(access.mask),
                width: access.read_width,
                flags: FlagUpdate::None,
            },
        );
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Mov {
                dst: self.dst_reg(rd, ctx),
                src: SrcOperand::Reg(masked),
                width: OpWidth::W64,
            },
        );
        Ok(())
    }

    pub(crate) fn lift_msr(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (sysreg, rt) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::SysReg(sysreg)), Some(Operand::Reg(rt))) => (sysreg, rt),
            _ => return Err(LiftError::Internal("invalid MSR operands".to_string())),
        };
        let Some(access) = Self::supported_sysreg(*sysreg) else {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!("MSR sysreg {sysreg:#06x}"),
            });
        };

        let masked = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: masked,
                src1: self.arm_reg(rt),
                src2: SrcOperand::Imm(access.mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::Arm(access.reg)),
                src: SrcOperand::Reg(masked),
                width: access.write_width,
            },
        );
        Ok(())
    }
}
