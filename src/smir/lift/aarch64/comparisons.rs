//! Compare aliases and the shared AArch32/Thumb dispatch boundary.

use crate::isa::arm::ExecutionState;
use crate::isa::arm::decoder::{DecodedInsn, Mnemonic, Operand};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::lift::{LiftContext, LiftError};

use super::Aarch64Lifter;

impl Aarch64Lifter {
    pub(super) fn lift_compare(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rn, operand) = match (insn.operands.first(), insn.operands.get(1)) {
            (Some(Operand::Reg(rn)), Some(operand)) => (rn, operand),
            _ => {
                return Err(LiftError::Internal(format!(
                    "invalid {:?} operands",
                    insn.mnemonic
                )));
            }
        };
        let src1 = self.arm_reg(rn);
        let width = self.reg_width(rn);
        let src2 = self.operand_to_src(operand, ctx)?;

        // AArch32 and Thumb also use this dispatcher. Their native admission
        // contract consumes the existing discard-result ALU representation.
        let flag_only = insn.state == ExecutionState::Aarch64;
        let kind = match (insn.mnemonic, flag_only) {
            (Mnemonic::CMP, true) => OpKind::Cmp { src1, src2, width },
            (Mnemonic::CMN, true) => OpKind::Cmn { src1, src2, width },
            (Mnemonic::TST, true) => OpKind::Test { src1, src2, width },
            (Mnemonic::CMP, false) => OpKind::Sub {
                dst: ctx.alloc_vreg(),
                src1,
                src2,
                width,
                flags: FlagUpdate::All,
            },
            (Mnemonic::CMN, false) => OpKind::Add {
                dst: ctx.alloc_vreg(),
                src1,
                src2,
                width,
                flags: FlagUpdate::All,
            },
            (Mnemonic::TST, false) => OpKind::And {
                dst: ctx.alloc_vreg(),
                src1,
                src2,
                width,
                flags: FlagUpdate::All,
            },
            _ => {
                return Err(LiftError::Internal(format!(
                    "invalid comparison mnemonic {:?}",
                    insn.mnemonic
                )));
            }
        };
        Self::push_lifted_op(ops, pc, kind);
        Ok(())
    }
}
