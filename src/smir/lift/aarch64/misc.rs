//! misc.rs

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
    /// Create a new AArch64 lifter
    pub fn new() -> Self {
        Aarch64Lifter { strict: false }
    }


    /// Create a lifter in strict mode
    pub fn strict() -> Self {
        Aarch64Lifter { strict: true }
    }


    // ========================================================================
    // Register Conversion
    // ========================================================================

    /// Convert ARM register to VReg
    pub(crate) fn arm_reg(&self, reg: &Register) -> VReg {
        if reg.is_sp {
            VReg::Arch(ArchReg::Arm(ArmReg::Sp))
        } else if reg.num == 31 && !reg.is_sp {
            // XZR/WZR reads as zero
            VReg::Imm(0)
        } else {
            VReg::Arch(ArchReg::Arm(ArmReg::X(reg.num)))
        }
    }


    /// Get the width for an ARM register operand
    pub(crate) fn reg_width(&self, reg: &Register) -> OpWidth {
        if reg.is_64bit {
            OpWidth::W64
        } else {
            OpWidth::W32
        }
    }


    /// Handle pre/post-index writeback for memory operand
    pub(crate) fn handle_writeback(
        &self,
        mem: &MemOperand,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        _ctx: &mut LiftContext,
    ) {
        let offset = match &mem.offset {
            MemOffset::Imm(off) => *off,
            _ => return,
        };

        match mem.mode {
            AddressingMode::PreIndex | AddressingMode::PostIndex => {
                let width = self.reg_width(&mem.base);
                let base_reg = self.arm_reg(&mem.base);

                if matches!(base_reg, VReg::Imm(_)) {
                    return;
                }

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Add {
                        dst: base_reg,
                        src1: base_reg,
                        src2: SrcOperand::Imm(offset),
                        width,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            AddressingMode::Offset => {}
        }
    }


    pub(crate) fn materialize_src_operand(
        &self,
        src: SrcOperand,
        width: OpWidth,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> VReg {
        match src {
            SrcOperand::Reg(reg) => reg,
            SrcOperand::Imm(value) | SrcOperand::Imm64(value) => VReg::Imm(value),
            src => {
                let tmp = ctx.alloc_vreg();
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::Mov {
                        dst: tmp,
                        src,
                        width,
                    },
                );
                tmp
            }
        }
    }


    /// Get destination VReg from operand, handling XZR/WZR writes
    pub(crate) fn dst_reg(&self, reg: &Register, ctx: &mut LiftContext) -> VReg {
        if reg.num == 31 && !reg.is_sp {
            ctx.alloc_vreg()
        } else if reg.is_sp {
            VReg::Arch(ArchReg::Arm(ArmReg::Sp))
        } else {
            VReg::Arch(ArchReg::Arm(ArmReg::X(reg.num)))
        }
    }


    pub(crate) fn push_lifted_op(ops: &mut Vec<SmirOp>, pc: u64, kind: OpKind) {
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
    }


    pub(crate) fn widen_w_to_x(
        &self,
        ops: &mut Vec<SmirOp>,
        pc: u64,
        ctx: &mut LiftContext,
        src: &Register,
        signed: bool,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        let kind = if signed {
            OpKind::SignExtend {
                dst,
                src: self.arm_reg(src),
                from_width: OpWidth::W32,
                to_width: OpWidth::W64,
            }
        } else {
            OpKind::ZeroExtend {
                dst,
                src: self.arm_reg(src),
                from_width: OpWidth::W32,
                to_width: OpWidth::W64,
            }
        };
        Self::push_lifted_op(ops, pc, kind);
        dst
    }


    // ========================================================================
    // FP Helpers
    // ========================================================================

    pub(crate) fn fp_vreg(reg: &FpRegister) -> VReg {
        VReg::Arch(ArchReg::Arm(ArmReg::V(reg.num)))
    }


    pub(crate) fn fp_precision(size: &FpRegSize) -> FpPrecision {
        match size {
            FpRegSize::S => FpPrecision::F32,
            FpRegSize::D => FpPrecision::F64,
            FpRegSize::H => FpPrecision::F16,
            _ => FpPrecision::F32,
        }
    }


    // ========================================================================
    // Helper Methods
    // ========================================================================

    pub(crate) fn parse_arith_operands(
        &self,
        insn: &DecodedInsn,
        ctx: &mut LiftContext,
    ) -> Result<(VReg, VReg, SrcOperand, OpWidth), LiftError> {
        let rd = match insn.operands.get(0) {
            Some(Operand::Reg(r)) => r,
            _ => return Err(LiftError::Internal("missing rd".to_string())),
        };

        let rn = match insn.operands.get(1) {
            Some(Operand::Reg(r)) => r,
            _ => return Err(LiftError::Internal("missing rn".to_string())),
        };

        let src2 = self.parse_operand2(insn, 2, ctx)?;

        Ok((
            self.dst_reg(rd, ctx),
            self.arm_reg(rn),
            src2,
            self.reg_width(rd),
        ))
    }


    pub(crate) fn parse_operand2(
        &self,
        insn: &DecodedInsn,
        idx: usize,
        _ctx: &mut LiftContext,
    ) -> Result<SrcOperand, LiftError> {
        match insn.operands.get(idx) {
            Some(Operand::Reg(r)) => Ok(SrcOperand::Reg(self.arm_reg(r))),
            Some(Operand::Imm(imm)) => Ok(SrcOperand::Imm(imm.effective_value())),
            Some(Operand::ShiftedReg(sr)) => {
                let amount = sr.immediate_amount().ok_or_else(|| {
                    LiftError::Internal("A64 operand has register-specified shift".to_string())
                })?;
                Ok(SrcOperand::Shifted {
                    reg: self.arm_reg(&sr.reg),
                    shift: self.arm_shift(sr.shift_type),
                    amount,
                })
            }
            Some(Operand::ExtendedReg(er)) => Ok(SrcOperand::Extended {
                reg: self.arm_reg(&er.reg),
                extend: self.arm_extend(er.extend_type),
                shift: er.shift,
            }),
            _ => Err(LiftError::Internal("invalid operand2".to_string())),
        }
    }


    pub(crate) fn operand_to_src(
        &self,
        op: &Operand,
        _ctx: &mut LiftContext,
    ) -> Result<SrcOperand, LiftError> {
        match op {
            Operand::Reg(r) => Ok(SrcOperand::Reg(self.arm_reg(r))),
            Operand::Imm(imm) => Ok(SrcOperand::Imm(imm.effective_value())),
            Operand::ShiftedReg(sr) => {
                let amount = sr.immediate_amount().ok_or_else(|| {
                    LiftError::Internal("A64 operand has register-specified shift".to_string())
                })?;
                Ok(SrcOperand::Shifted {
                    reg: self.arm_reg(&sr.reg),
                    shift: self.arm_shift(sr.shift_type),
                    amount,
                })
            }
            Operand::ExtendedReg(er) => Ok(SrcOperand::Extended {
                reg: self.arm_reg(&er.reg),
                extend: self.arm_extend(er.extend_type),
                shift: er.shift,
            }),
            _ => Err(LiftError::Internal("invalid operand".to_string())),
        }
    }


    pub(crate) fn lift_axflag(&self, pc: u64, ops: &mut Vec<SmirOp>, ctx: &mut LiftContext) {
        let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));

        let v_to_z = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Shl {
                dst: v_to_z,
                src: nzcv,
                amount: SrcOperand::Imm(2),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let z_or_v = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Or {
                dst: z_or_v,
                src1: nzcv,
                src2: SrcOperand::Reg(v_to_z),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let z_bit = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: z_bit,
                src1: z_or_v,
                src2: SrcOperand::Imm(NZCV_Z),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let v_to_c = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Shl {
                dst: v_to_c,
                src: nzcv,
                amount: SrcOperand::Imm(1),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let c_raw = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::And {
                dst: c_raw,
                src1: nzcv,
                src2: SrcOperand::Imm(NZCV_C),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let c_bit = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::AndNot {
                dst: c_bit,
                src1: c_raw,
                src2: SrcOperand::Reg(v_to_c),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        let result = ctx.alloc_vreg();
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Or {
                dst: result,
                src1: z_bit,
                src2: SrcOperand::Reg(c_bit),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );

        Self::push_lifted_op(
            ops,
            pc,
            OpKind::Mov {
                dst: nzcv,
                src: SrcOperand::Reg(result),
                width: OpWidth::W32,
            },
        );
    }


    pub(crate) fn lift_atomic_rmw(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rs, rt, mem) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::Reg(rs)), Some(Operand::Reg(rt)), Some(Operand::Mem(m))) => (rs, rt, m),
            _ => {
                return Err(LiftError::Internal(
                    "invalid atomic RMW operands".to_string(),
                ));
            }
        };

        let width = match (insn.raw >> 30) & 0x3 {
            0 => MemWidth::B1,
            1 => MemWidth::B2,
            2 => MemWidth::B4,
            _ => MemWidth::B8,
        };
        let order = match (((insn.raw >> 23) & 1) != 0, ((insn.raw >> 22) & 1) != 0) {
            (false, false) => MemoryOrder::Relaxed,
            (true, false) => MemoryOrder::Acquire,
            (false, true) => MemoryOrder::Release,
            (true, true) => MemoryOrder::AcqRel,
        };
        let atomic_op = match insn.mnemonic {
            Mnemonic::SWP | Mnemonic::SWPA | Mnemonic::SWPAL | Mnemonic::SWPL => AtomicOp::Swap,
            Mnemonic::LDADD | Mnemonic::LDADDA | Mnemonic::LDADDAL | Mnemonic::LDADDL => {
                AtomicOp::Add
            }
            Mnemonic::LDCLR => AtomicOp::And,
            Mnemonic::LDEOR => AtomicOp::Xor,
            Mnemonic::LDSET => AtomicOp::Or,
            Mnemonic::LDSMAX => AtomicOp::Max,
            Mnemonic::LDSMIN => AtomicOp::Min,
            Mnemonic::LDUMAX => AtomicOp::Umax,
            Mnemonic::LDUMIN => AtomicOp::Umin,
            _ => unreachable!(),
        };

        let dst = self.dst_reg(rt, ctx);
        let mut src = self.arm_reg(rs);
        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);

        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        if insn.mnemonic == Mnemonic::LDCLR {
            let inverted = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Not {
                    dst: inverted,
                    src,
                    width: OpWidth::W64,
                },
            ));
            src = inverted;
        }

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::AtomicRmw {
                dst,
                addr: self.indexed_access_addr(mem, addr),
                src,
                op: atomic_op,
                width,
                order,
            },
        ));

        Ok(())
    }


    pub(crate) fn lift_cas(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (compare, new_val, mem) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::Reg(compare)), Some(Operand::Reg(new_val)), Some(Operand::Mem(mem))) => {
                (compare, new_val, mem)
            }
            _ => return Err(LiftError::Internal("invalid CAS operands".to_string())),
        };

        let width = match (insn.raw >> 30) & 0x3 {
            0 => MemWidth::B1,
            1 => MemWidth::B2,
            2 => MemWidth::B4,
            _ => MemWidth::B8,
        };
        let order = match (((insn.raw >> 22) & 1) != 0, ((insn.raw >> 15) & 1) != 0) {
            (false, false) => MemoryOrder::Relaxed,
            (true, false) => MemoryOrder::Acquire,
            (false, true) => MemoryOrder::Release,
            (true, true) => MemoryOrder::AcqRel,
        };

        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);
        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Cas {
                dst: self.dst_reg(compare, ctx),
                success: ctx.alloc_vreg(),
                addr: self.indexed_access_addr(mem, addr),
                expected: self.arm_reg(compare),
                new_val: self.arm_reg(new_val),
                width,
                order,
            },
        ));

        Ok(())
    }
}
