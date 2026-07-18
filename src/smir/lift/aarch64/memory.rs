//! memory.rs

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

    pub(crate) fn lift_load(
        &self,
        insn: &DecodedInsn,
        width: MemWidth,
        extend: SignExtend,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rd, mem) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::Reg(r)), Some(Operand::Mem(m))) => (r, m),
            (Some(Operand::Reg(r)), Some(Operand::Label(off))) => {
                let dst = self.dst_reg(r, ctx);
                let load_dst = if extend == SignExtend::Sign && !r.is_64bit {
                    ctx.alloc_vreg()
                } else {
                    dst
                };

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: load_dst,
                        addr: Address::PcRel {
                            offset: *off,
                            disp_size: DispSize::Auto,
                            base: None,
                        },
                        width,
                        sign: extend,
                    },
                ));
                if load_dst != dst {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::ZeroExtend {
                            dst,
                            src: load_dst,
                            from_width: OpWidth::W32,
                            to_width: OpWidth::W64,
                        },
                    ));
                }
                return Ok(());
            }
            _ => return Err(LiftError::Internal("invalid load operands".to_string())),
        };

        let dst = self.dst_reg(rd, ctx);
        let load_dst = if extend == SignExtend::Sign && !rd.is_64bit {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);

        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        if mem.mode == AddressingMode::PreIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        let load_addr = self.indexed_access_addr(mem, addr);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Load {
                dst: load_dst,
                addr: load_addr,
                width,
                sign: extend,
            },
        ));
        if load_dst != dst {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::ZeroExtend {
                    dst,
                    src: load_dst,
                    from_width: OpWidth::W32,
                    to_width: OpWidth::W64,
                },
            ));
        }

        if mem.mode == AddressingMode::PostIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        Ok(())
    }


    pub(crate) fn lift_store(
        &self,
        insn: &DecodedInsn,
        width: MemWidth,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rt, mem) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::Reg(r)), Some(Operand::Mem(m))) => (r, m),
            _ => return Err(LiftError::Internal("invalid store operands".to_string())),
        };

        let src = self.arm_reg(rt);
        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);

        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        if mem.mode == AddressingMode::PreIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        let store_addr = self.indexed_access_addr(mem, addr);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Store {
                src,
                addr: store_addr,
                width,
            },
        ));

        if mem.mode == AddressingMode::PostIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        Ok(())
    }


    pub(crate) fn lift_load_exclusive(
        &self,
        insn: &DecodedInsn,
        width: MemWidth,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rd, mem) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::Reg(r)), Some(Operand::Mem(m))) => (r, m),
            _ => {
                return Err(LiftError::Internal(
                    "invalid load-exclusive operands".to_string(),
                ));
            }
        };

        let dst = self.dst_reg(rd, ctx);
        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);

        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        let load_addr = self.indexed_access_addr(mem, addr);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::LoadExclusive {
                dst,
                addr: load_addr,
                width,
            },
        ));

        Ok(())
    }


    pub(crate) fn lift_store_exclusive(
        &self,
        insn: &DecodedInsn,
        width: MemWidth,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (status, src, mem) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::Reg(status)), Some(Operand::Reg(src)), Some(Operand::Mem(mem))) => {
                (status, src, mem)
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid store-exclusive operands".to_string(),
                ));
            }
        };

        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);
        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        let store_addr = self.indexed_access_addr(mem, addr);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::StoreExclusive {
                status: self.dst_reg(status, ctx),
                src: self.arm_reg(src),
                addr: store_addr,
                width,
            },
        ));

        Ok(())
    }


    pub(crate) fn lift_load_pair(
        &self,
        insn: &DecodedInsn,
        extend: SignExtend,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rt1, rt2, mem) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::Reg(r1)), Some(Operand::Reg(r2)), Some(Operand::Mem(m))) => (r1, r2, m),
            _ => return Err(LiftError::Internal("invalid LDP operands".to_string())),
        };

        let dst1 = self.dst_reg(rt1, ctx);
        let dst2 = self.dst_reg(rt2, ctx);
        let width = if rt1.is_64bit {
            MemWidth::B8
        } else {
            MemWidth::B4
        };
        let offset2 = if rt1.is_64bit { 8i64 } else { 4i64 };

        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);

        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        if mem.mode == AddressingMode::PreIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        let load_addr = self.indexed_access_addr(mem, addr);

        if extend == SignExtend::Zero {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::LoadPair {
                    dst1,
                    dst2,
                    addr: load_addr,
                    width,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: dst1,
                    addr: load_addr.clone(),
                    width,
                    sign: extend,
                },
            ));

            let addr2 = match &load_addr {
                Address::Direct(base) => Address::BaseOffset {
                    base: *base,
                    offset: offset2,
                    disp_size: DispSize::Auto,
                },
                Address::BaseOffset {
                    base,
                    offset,
                    disp_size,
                } => Address::BaseOffset {
                    base: *base,
                    offset: *offset + offset2,
                    disp_size: *disp_size,
                },
                _ => load_addr,
            };

            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: dst2,
                    addr: addr2,
                    width,
                    sign: extend,
                },
            ));
        }

        if mem.mode == AddressingMode::PostIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        Ok(())
    }


    pub(crate) fn lift_store_pair(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rt1, rt2, mem) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::Reg(r1)), Some(Operand::Reg(r2)), Some(Operand::Mem(m))) => (r1, r2, m),
            _ => return Err(LiftError::Internal("invalid STP operands".to_string())),
        };

        let src1 = self.arm_reg(rt1);
        let src2 = self.arm_reg(rt2);
        let width = if rt1.is_64bit {
            MemWidth::B8
        } else {
            MemWidth::B4
        };

        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);

        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }

        if mem.mode == AddressingMode::PreIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        let store_addr = self.indexed_access_addr(mem, addr);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::StorePair {
                src1,
                src2,
                addr: store_addr,
                width,
            },
        ));

        if mem.mode == AddressingMode::PostIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }

        Ok(())
    }
}
