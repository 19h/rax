//! Branch, jump, call, return, and loop lifting

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86FarReturnOp, X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind,
    X86VecAlign, X86VecMap, X86X87ArithmeticDestination, X86X87ArithmeticSource,
    X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind, X86X87EnvWidth,
    X86X87FloatWidth, X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl X86_64Lifter {
    /// Lift CALL rel32 (E8)
    pub(crate) fn lift_call_rel32(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < 4 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: 4,
            });
        }

        let rel = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64;
        let insn_len = prefix.cursor + 4;
        let next_rip = pc + insn_len as u64;
        let target = (next_rip as i64 + rel) as u64;

        Ok(LiftResult {
            ops: vec![],
            bytes_consumed: insn_len,
            control_flow: ControlFlow::Call {
                target: CallTarget::GuestAddr(target),
            },
            branch_targets: vec![target],
        })
    }

    /// Lift RET (C3)
    pub(crate) fn lift_ret(
        &self,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mut ops = Vec::new();
        let ret_addr = ctx.alloc_vreg();

        // Load return address
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Load {
                dst: ret_addr,
                addr: Address::Direct(self.rsp()),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        ));

        // RSP += 8
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Add {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        Ok(LiftResult::ret(ops, prefix.cursor))
    }

    /// Lift RET imm16 (C2)
    pub(crate) fn lift_ret_imm16(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < 2 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: 2,
            });
        }

        let imm = u16::from_le_bytes([bytes[0], bytes[1]]) as i64;
        let mut ops = Vec::new();
        let ret_addr = ctx.alloc_vreg();

        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Load {
                dst: ret_addr,
                addr: Address::Direct(self.rsp()),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        ));

        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Add {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(8 + imm),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        Ok(LiftResult::ret(ops, prefix.cursor + 2))
    }

    /// Lift far RET (`CA iw`/`CB`) as one fault-precise system operation. The
    /// operation owns all stack and descriptor accesses because decomposing
    /// them into ordinary loads would commit architectural state too early.
    pub(crate) fn lift_far_ret(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }

        let (pop_bytes, immediate_len) = match opcode {
            0xCB => (0, 0),
            0xCA => {
                if bytes.len() < 2 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: 2,
                    });
                }
                (u16::from_le_bytes([bytes[0], bytes[1]]), 2)
            }
            _ => unreachable!("far-RET lifter called for non-RETF opcode"),
        };

        let bytes_consumed = prefix.cursor + immediate_len;
        let target = VReg::Arch(ArchReg::X86(X86Reg::Rip));
        let op = SmirOp::new(
            OpId(0),
            pc,
            OpKind::X86FarReturn(X86FarReturnOp {
                target,
                offset_width: prefix.op_width(),
                pop_bytes,
                requires_apx: prefix.rex2.is_some(),
                next_pc: pc + bytes_consumed as u64,
            }),
        );

        Ok(LiftResult {
            ops: vec![op],
            bytes_consumed,
            control_flow: ControlFlow::IndirectBranch { target },
            branch_targets: vec![],
        })
    }

    /// Lift JMP rel8 (EB)
    pub(crate) fn lift_jmp_rel8(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        let rel = bytes[0] as i8 as i64;
        let insn_len = prefix.cursor + 1;
        let target = (pc as i64 + insn_len as i64 + rel) as u64;

        Ok(LiftResult::branch(vec![], insn_len, target))
    }

    /// Lift JMP rel32 (E9)
    pub(crate) fn lift_jmp_rel32(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < 4 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: 4,
            });
        }

        let rel = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64;
        let insn_len = prefix.cursor + 4;
        let target = (pc as i64 + insn_len as i64 + rel) as u64;

        Ok(LiftResult::branch(vec![], insn_len, target))
    }

    /// Lift APX JMPABS imm64 (REX2 + A1).
    pub(crate) fn lift_jmp_abs(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        if bytes.len() < 8 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: 8,
            });
        }

        let target = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Ok(LiftResult::branch(vec![], prefix.cursor + 8, target))
    }

    /// Lift Jcc rel8 (70-7F)
    pub(crate) fn lift_jcc_rel8(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        let cc = opcode & 0x0F;
        let cond = self.x86_cond(cc);
        let rel = bytes[0] as i8 as i64;
        let insn_len = prefix.cursor + 1;
        let next_pc = pc + insn_len as u64;
        let target = (next_pc as i64 + rel) as u64;

        Ok(LiftResult::cond_branch(
            vec![],
            insn_len,
            cond,
            target,
            next_pc,
        ))
    }

    /// Lift LOOPNZ/LOOPZ/LOOP/JRCXZ (E0-E3) in 64-bit mode. Counter width
    /// follows address size (RCX normally, ECX under 67h); internal comparisons
    /// restore RFLAGS because these instructions do not modify flags.
    pub(crate) fn lift_loop_rel8(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        let width = if prefix.address_size_override {
            OpWidth::W32
        } else {
            OpWidth::W64
        };
        let insn_len = prefix.cursor + 1;
        let next_pc = pc + insn_len as u64;
        let target = (next_pc as i64 + bytes[0] as i8 as i64) as u64;
        let mut ops = Vec::new();
        let saved_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::ReadFlags { dst: saved_flags },
        ));

        if opcode != 0xE3 {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Dec {
                    dst: self.gpr(1),
                    src: self.gpr(1),
                    width,
                    flags: FlagUpdate::None,
                },
            ));
        }

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Cmp {
                src1: self.gpr(1),
                src2: SrcOperand::Imm(0),
                width,
            },
        ));
        let count_condition = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::SetCC {
                dst: count_condition,
                cond: if opcode == 0xE3 {
                    Condition::Eq
                } else {
                    Condition::Ne
                },
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::WriteFlags { src: saved_flags },
        ));

        let branch_condition = match opcode {
            0xE0 | 0xE1 => {
                let zf_condition = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::SetCC {
                        dst: zf_condition,
                        cond: if opcode == 0xE0 {
                            Condition::Ne
                        } else {
                            Condition::Eq
                        },
                        width: OpWidth::W64,
                    },
                ));
                let combined = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: combined,
                        src1: count_condition,
                        src2: SrcOperand::Reg(zf_condition),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                combined
            }
            0xE2 | 0xE3 => count_condition,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![opcode],
                });
            }
        };

        Ok(LiftResult {
            ops,
            bytes_consumed: insn_len,
            control_flow: ControlFlow::CondBranchReg {
                cond: branch_condition,
                taken: target,
                not_taken: next_pc,
            },
            branch_targets: vec![target, next_pc],
        })
    }
}
