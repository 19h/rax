//! Direct and conditional control-flow instruction lifting.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::lift::riscv::*;
use crate::smir::lift::{ControlFlow, LiftContext, LiftError};

impl RiscVLifter {
    /// JAL: Jump and Link
    pub(crate) fn lift_jal(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rd = Self::rd(insn);
        let imm = Self::imm_j(insn);
        let target = (addr as i64).wrapping_add(imm) as u64;
        let return_addr = addr + 4;

        // With IALIGN=32, a misaligned JAL must trap before writing the link
        // register. Keep that instruction on the direct interpreter path.
        if !self.extensions.c && target & 0x3 != 0 {
            return Err(LiftError::Unsupported {
                addr,
                mnemonic: "jal with misaligned IALIGN=32 target".to_string(),
            });
        }

        let mut ops = Vec::new();

        // Save return address to rd
        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(return_addr as i64),
                    width: self.op_width(),
                },
            ));
        }

        Ok((ops, ControlFlow::DirectBranch(target)))
    }

    /// JALR: Jump and Link Register
    pub(crate) fn lift_jalr(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        if Self::funct3(insn) != 0 {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        // JALR clears bit 0, but bit 1 is dynamic. SMIR has no precise
        // instruction-address-misaligned guard for the indirect terminator, so
        // an IALIGN=32 profile must use the direct interpreter.
        if !self.extensions.c {
            return Err(LiftError::Unsupported {
                addr,
                mnemonic: "jalr with dynamic IALIGN=32 target".to_string(),
            });
        }

        let rd = Self::rd(insn);
        let rs1_reg = Self::rs1(insn);
        let imm = Self::imm_i(insn);
        let return_addr = addr + 4;

        let mut ops = Vec::new();

        // Compute target address: (rs1 + imm) & ~1
        let rs1 = self.get_x_reg(rs1_reg, ctx);
        let target = ctx.alloc_vreg();

        if imm != 0 {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Add {
                    dst: target,
                    src1: rs1,
                    src2: SrcOperand::Imm(imm),
                    width: self.op_width(),
                    flags: FlagUpdate::None,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Mov {
                    dst: target,
                    src: SrcOperand::Reg(rs1),
                    width: self.op_width(),
                },
            ));
        }

        // Clear bit 0
        let target_aligned = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::And {
                dst: target_aligned,
                src1: target,
                src2: SrcOperand::Imm(!1i64),
                width: self.op_width(),
                flags: FlagUpdate::None,
            },
        ));

        // Save return address to rd
        if let Some(dst) = self.def_x_reg(rd, ctx) {
            ops.push(SmirOp::new(
                ctx.next_op_id(),
                addr,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(return_addr as i64),
                    width: self.op_width(),
                },
            ));
        }

        Ok((
            ops,
            ControlFlow::IndirectBranch {
                target: target_aligned,
            },
        ))
    }

    /// Branch instructions (BEQ, BNE, BLT, BGE, BLTU, BGEU)
    pub(crate) fn lift_branch(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let rs1_reg = Self::rs1(insn);
        let rs2_reg = Self::rs2(insn);
        let funct3 = Self::funct3(insn);
        let imm = Self::imm_b(insn);
        let target = (addr as i64).wrapping_add(imm) as u64;
        let fallthrough = addr + 4;

        let cond = match funct3 {
            0b000 => Condition::Eq,  // BEQ
            0b001 => Condition::Ne,  // BNE
            0b100 => Condition::Slt, // BLT
            0b101 => Condition::Sge, // BGE
            0b110 => Condition::Ult, // BLTU
            0b111 => Condition::Uge, // BGEU
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr,
                    bytes: insn.to_le_bytes().to_vec(),
                });
            }
        };

        // A misaligned conditional target traps only when taken. Falling back
        // preserves that runtime condition and the precise faulting PC.
        if !self.extensions.c && target & 0x3 != 0 {
            return Err(LiftError::Unsupported {
                addr,
                mnemonic: "branch with misaligned IALIGN=32 target".to_string(),
            });
        }

        let rs1 = self.get_x_reg(rs1_reg, ctx);
        let rs2 = self.get_x_reg(rs2_reg, ctx);

        let mut ops = Vec::new();

        // Compare rs1 and rs2
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::Cmp {
                src1: rs1,
                src2: SrcOperand::Reg(rs2),
                width: self.op_width(),
            },
        ));

        // Set condition result
        let cond_reg = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::SetCC {
                dst: cond_reg,
                cond,
                width: OpWidth::W8,
            },
        ));

        Ok((
            ops,
            ControlFlow::CondBranchReg {
                cond: cond_reg,
                taken: target,
                not_taken: fallthrough,
            },
        ))
    }
}
