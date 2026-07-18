//! simd.rs

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

    /// Emit a vector per-lane unary op (advanced SIMD two-register misc) as an
    /// `OpKind::VUnary`. When `byte_wise` the element is forced to I8 (CNT/NOT/
    /// RBIT operate per byte); otherwise the element width comes from
    /// size = bits[23:22] and the lane count from Q.
    pub(crate) fn lift_vector_unary(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        op: VecUnaryOp,
        byte_wise: bool,
    ) -> Result<(), LiftError> {
        let (rd, rn) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn))) => (rd, rn),
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("vector {:?}", insn.mnemonic),
                });
            }
        };
        let q = (insn.raw >> 30) & 1;
        let size = (insn.raw >> 22) & 0x3;
        // AArch64 vector REV reverses elements WITHIN a container, so the element
        // size must be strictly smaller than the container; otherwise the encoding
        // is reserved (the interpreter raises UNDEFINED). Reject the invalid
        // arrangements — REV16: byte only; REV32: byte/halfword; REV64:
        // byte/halfword/word — so they bail to the interpreter instead of lowering
        // to an architecturally undefined host op and a SIGILL. (#55)
        let rev_max_size = match op {
            VecUnaryOp::Rev16 => Some(0),
            VecUnaryOp::Rev32 => Some(1),
            VecUnaryOp::Rev64 => Some(2),
            _ => None,
        };
        if let Some(max) = rev_max_size {
            if size > max {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("vector {:?}", insn.mnemonic),
                });
            }
        }
        let (elem, lane_bytes) = if byte_wise {
            (VecElementType::I8, 1u8)
        } else {
            match size {
                0 => (VecElementType::I8, 1),
                1 => (VecElementType::I16, 2),
                2 => (VecElementType::I32, 4),
                _ => (VecElementType::I64, 8),
            }
        };
        let lanes = (if q == 1 { 16u8 } else { 8 }) / lane_bytes;
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VUnary {
                dst: Self::fp_vreg(rd),
                src: Self::fp_vreg(rn),
                elem,
                lanes,
                op,
            },
        ));
        Ok(())
    }


    /// Emit a vector across-lanes reduction (advanced SIMD across lanes) as an
    /// `OpKind::VReduce`. Operand 0 is the scalar destination, operand 1 the
    /// vector source; the source element width comes from size = bits[23:22]
    /// and the lane count from Q.
    pub(crate) fn lift_vector_reduce(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        op: VecReduceOp,
    ) -> Result<(), LiftError> {
        let (rd, rn) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn))) => (rd, rn),
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("{:?}", insn.mnemonic),
                });
            }
        };
        let q = (insn.raw >> 30) & 1;
        let size = (insn.raw >> 22) & 0x3;
        // Across-vector integer reductions (ADDV/SADDLV/UADDLV/SMAXV/...) are defined
        // ONLY for 8B/16B/4H/8H/4S. Reject 2S (size=0b10, Q=0) and the 64-bit-element
        // forms (size=0b11, 1D/2D): both are reserved encodings that would otherwise
        // lower to an invalid host across-lanes op and SIGILL. Bail to the
        // interpreter, which treats these as UNDEFINED. (#28)
        if size == 0b11 || (size == 0b10 && q == 0) {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!("{:?}", insn.mnemonic),
            });
        }
        let (elem, lane_bytes) = match size {
            0 => (VecElementType::I8, 1u8),
            1 => (VecElementType::I16, 2),
            2 => (VecElementType::I32, 4),
            _ => (VecElementType::I64, 8),
        };
        let lanes = (if q == 1 { 16u8 } else { 8 }) / lane_bytes;
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VReduce {
                dst: Self::fp_vreg(rd),
                src: Self::fp_vreg(rn),
                elem,
                lanes,
                op,
            },
        ));
        Ok(())
    }


    /// Emit a vector two-source permute (ZIP/UZP/TRN) as an `OpKind::VPermute2`.
    /// Three vector operands; element width from size = bits[23:22], lane count
    /// from Q.
    pub(crate) fn lift_vpermute(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        kind: VecPermuteKind,
    ) -> Result<(), LiftError> {
        let (rd, rn, rm) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn)), Some(Operand::FpReg(rm))) => {
                (rd, rn, rm)
            }
            _ => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("{:?}", insn.mnemonic),
                });
            }
        };
        let q = (insn.raw >> 30) & 1;
        let (elem, lane_bytes) = match (insn.raw >> 22) & 0x3 {
            0 => (VecElementType::I8, 1u8),
            1 => (VecElementType::I16, 2),
            2 => (VecElementType::I32, 4),
            _ => (VecElementType::I64, 8),
        };
        let lanes = (if q == 1 { 16u8 } else { 8 }) / lane_bytes;
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPermute2 {
                dst: Self::fp_vreg(rd),
                src1: Self::fp_vreg(rn),
                src2: Self::fp_vreg(rm),
                elem,
                lanes,
                kind,
            },
        ));
        Ok(())
    }


    /// Lift a SIMD/FP register load/store (LDR/STR whose Rt the decoder resolved
    /// to an FP register) into a VLoad/VStore. Only Q (128-bit) and D (64-bit)
    /// map to a VecWidth; smaller scalar-FP widths bail so the region deopts.
    pub(crate) fn lift_vector_mem(
        &self,
        insn: &DecodedInsn,
        is_load: bool,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rt, mem) = match (insn.operands.get(0), insn.operands.get(1)) {
            (Some(Operand::FpReg(r)), Some(Operand::Mem(m))) => (r, m),
            _ => {
                return Err(LiftError::Internal(
                    "invalid vector mem operands".to_string(),
                ));
            }
        };
        let width = match rt.size {
            FpRegSize::Q => VecWidth::V128,
            FpRegSize::D => VecWidth::V64,
            other => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("SIMD {other:?} load/store"),
                });
            }
        };
        let vreg = Self::fp_vreg(rt);
        let (addr, pre_ops) = self.mem_to_addr(mem, ctx);
        for mut op in pre_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op);
        }
        if mem.mode == AddressingMode::PreIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }
        let access_addr = self.indexed_access_addr(mem, addr);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            if is_load {
                OpKind::VLoad {
                    dst: vreg,
                    addr: access_addr,
                    width,
                }
            } else {
                OpKind::VStore {
                    src: vreg,
                    addr: access_addr,
                    width,
                }
            },
        ));
        if mem.mode == AddressingMode::PostIndex {
            self.handle_writeback(mem, pc, ops, ctx);
        }
        Ok(())
    }
}
