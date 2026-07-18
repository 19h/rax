//! Shift, rotate, bit-test, bit-scan, and population-count lifting

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
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

    pub(crate) fn x86_shift_op(
        group: u8,
        dst: VReg,
        src: VReg,
        amount: SrcOperand,
        width: OpWidth,
        update_flags: bool,
    ) -> OpKind {
        let rotate_flags = if update_flags {
            x86_rotate_flags()
        } else {
            FlagUpdate::None
        };
        let shift_flags = if update_flags {
            FlagUpdate::All
        } else {
            FlagUpdate::None
        };
        match group {
            0 => OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            1 => OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            2 => OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            3 => OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                flags: rotate_flags,
            },
            4 | 6 => OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags: shift_flags,
            },
            5 => OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags: shift_flags,
            },
            7 => OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags: shift_flags,
            },
            _ => unreachable!(),
        }
    }


    pub(crate) fn x86_shift_smir_op(id: OpId, pc: u64, group: u8, kind: OpKind) -> SmirOp {
        if group == 6 {
            SmirOp::with_hint(id, pc, kind, X86OpHint::ShiftGroup6)
        } else {
            SmirOp::new(id, pc, kind)
        }
    }


    /// Lift shift instructions with immediate (C0/C1)
    pub(crate) fn lift_shift_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = opcode == 0xC0;
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if bytes.len() < modrm.bytes_consumed + 1 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: modrm.bytes_consumed + 1,
            });
        }

        let imm = bytes[modrm.bytes_consumed] as i64;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let mut high_dst = None;

        let group = (modrm.byte >> 3) & 0x07;

        let (src, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else {
            if is_8bit {
                high_dst = self.high_byte_base(modrm.rm, prefix);
                (
                    self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                    None,
                )
            } else {
                (self.gpr(modrm.rm), None)
            }
        };

        let result = if addr.is_some() {
            ctx.alloc_vreg()
        } else {
            src
        };
        ops.push(Self::x86_shift_smir_op(
            OpId(ops.len() as u16),
            pc,
            group,
            Self::x86_shift_op(
                group,
                result,
                src,
                SrcOperand::Imm(imm),
                width,
                addr.is_none(),
            ),
        ));

        if let Some(base) = high_dst {
            self.merge_high_byte(base, result, pc, ctx, &mut ops);
        }

        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: self.size_to_memwidth(op_size),
                },
            ));
            let flag_result = ctx.alloc_vreg();
            ops.push(Self::x86_shift_smir_op(
                OpId(ops.len() as u16),
                pc,
                group,
                Self::x86_shift_op(group, flag_result, src, SrcOperand::Imm(imm), width, true),
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }


    /// Lift shift instructions with implicit count = 1 (D0/D1)
    pub(crate) fn lift_shift_one(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = opcode == 0xD0;
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mut high_dst = None;

        let group = (modrm.byte >> 3) & 0x07;

        let (src, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else {
            if is_8bit {
                high_dst = self.high_byte_base(modrm.rm, prefix);
                (
                    self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                    None,
                )
            } else {
                (self.gpr(modrm.rm), None)
            }
        };

        let result = if addr.is_some() {
            ctx.alloc_vreg()
        } else {
            src
        };
        ops.push(Self::x86_shift_smir_op(
            OpId(ops.len() as u16),
            pc,
            group,
            Self::x86_shift_op(
                group,
                result,
                src,
                SrcOperand::Imm(1),
                width,
                addr.is_none(),
            ),
        ));

        if let Some(base) = high_dst {
            self.merge_high_byte(base, result, pc, ctx, &mut ops);
        }

        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: self.size_to_memwidth(op_size),
                },
            ));
            let flag_result = ctx.alloc_vreg();
            ops.push(Self::x86_shift_smir_op(
                OpId(ops.len() as u16),
                pc,
                group,
                Self::x86_shift_op(group, flag_result, src, SrcOperand::Imm(1), width, true),
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift shift instructions with count in CL (D2/D3)
    pub(crate) fn lift_shift_cl(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = opcode == 0xD2;
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mut high_dst = None;

        let group = (modrm.byte >> 3) & 0x07;

        let (src, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else {
            if is_8bit {
                high_dst = self.high_byte_base(modrm.rm, prefix);
                (
                    self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                    None,
                )
            } else {
                (self.gpr(modrm.rm), None)
            }
        };

        let result = if addr.is_some() {
            ctx.alloc_vreg()
        } else {
            src
        };
        let amount = SrcOperand::Reg(self.gpr(1));
        ops.push(Self::x86_shift_smir_op(
            OpId(ops.len() as u16),
            pc,
            group,
            Self::x86_shift_op(group, result, src, amount.clone(), width, addr.is_none()),
        ));

        if let Some(base) = high_dst {
            self.merge_high_byte(base, result, pc, ctx, &mut ops);
        }

        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: self.size_to_memwidth(op_size),
                },
            ));
            let flag_result = ctx.alloc_vreg();
            ops.push(Self::x86_shift_smir_op(
                OpId(ops.len() as u16),
                pc,
                group,
                Self::x86_shift_op(group, flag_result, src, amount, width, true),
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift BSF/BSR (0F BC/0F BD)
    pub(crate) fn lift_bsf_bsr(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let src = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        let op_kind = if opcode == 0xBC {
            OpKind::Bsf {
                dst: self.gpr(modrm.reg),
                src,
                width,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            }
        } else {
            OpKind::Bsr {
                dst: self.gpr(modrm.reg),
                src,
                width,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            }
        };

        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn emit_bit_test_ops(
        &self,
        action: u8,
        modrm: &ModRm,
        index: SrcOperand,
        memory_index_reg: Option<VReg>,
        width: OpWidth,
        mem_width: MemWidth,
        prefix: &X86Prefix,
        next_pc: u64,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<Vec<SmirOp>, LiftError> {
        if !(4..=7).contains(&action) || (prefix.lock && (action == 4 || !modrm.is_memory)) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![modrm.byte],
            });
        }

        let mut ops = Vec::new();
        if !modrm.is_memory {
            let operand = self.gpr(modrm.rm);
            let kind = match action {
                4 => OpKind::Bt {
                    src: operand,
                    index,
                    width,
                },
                5 => OpKind::Bts {
                    dst: operand,
                    src: operand,
                    index,
                    width,
                },
                6 => OpKind::Btr {
                    dst: operand,
                    src: operand,
                    index,
                    width,
                },
                7 => OpKind::Btc {
                    dst: operand,
                    src: operand,
                    index,
                    width,
                },
                _ => unreachable!(),
            };
            ops.push(SmirOp::new(OpId(0), pc, kind));
            return Ok(ops);
        }

        let (mut addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        ops.extend(pre_ops);

        // Register-index memory forms address a bit string, not merely one
        // operand. A signed bit index selects floor(index / operand_bits)
        // operands away from the decoded base. Since operand sizes are powers
        // of two, arithmetic shift implements the required floor division.
        if let Some(index_reg) = memory_index_reg {
            let signed_index = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::SignExtend {
                    dst: signed_index,
                    src: index_reg,
                    from_width: width,
                    to_width: OpWidth::W64,
                },
            ));
            let operand_delta = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Sar {
                    dst: operand_delta,
                    src: signed_index,
                    amount: SrcOperand::Imm(match width {
                        OpWidth::W16 => 4,
                        OpWidth::W32 => 5,
                        OpWidth::W64 => 6,
                        _ => unreachable!(),
                    }),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            let byte_delta = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shl {
                    dst: byte_delta,
                    src: operand_delta,
                    amount: SrcOperand::Imm(match mem_width {
                        MemWidth::B2 => 1,
                        MemWidth::B4 => 2,
                        MemWidth::B8 => 3,
                        _ => unreachable!(),
                    }),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            let base_addr = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Lea {
                    dst: base_addr,
                    addr,
                },
            ));
            let effective_addr = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Add {
                    dst: effective_addr,
                    src1: base_addr,
                    src2: SrcOperand::Reg(byte_delta),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            addr = Address::Direct(effective_addr);
        }

        let bit_index = match index {
            SrcOperand::Imm(value) => SrcOperand::Imm(value & (width.bits() as i64 - 1)),
            SrcOperand::Reg(reg) => {
                let normalized = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: normalized,
                        src1: reg,
                        src2: SrcOperand::Imm(width.bits() as i64 - 1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                SrcOperand::Reg(normalized)
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![modrm.byte],
                });
            }
        };

        let old_value = ctx.alloc_vreg();
        if prefix.lock {
            let mask = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: mask,
                    src: SrcOperand::Imm(1),
                    width,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shl {
                    dst: mask,
                    src: mask,
                    amount: bit_index.clone(),
                    width,
                    flags: FlagUpdate::None,
                },
            ));
            let atomic_op = match action {
                5 => AtomicOp::Or,
                6 => {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Not {
                            dst: mask,
                            src: mask,
                            width,
                        },
                    ));
                    AtomicOp::And
                }
                7 => AtomicOp::Xor,
                _ => unreachable!(),
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::AtomicRmw {
                    dst: old_value,
                    addr,
                    src: mask,
                    op: atomic_op,
                    width: mem_width,
                    order: MemoryOrder::SeqCst,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: old_value,
                    addr: addr.clone(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            if action != 4 {
                let mask = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: mask,
                        src: SrcOperand::Imm(1),
                        width,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shl {
                        dst: mask,
                        src: mask,
                        amount: bit_index.clone(),
                        width,
                        flags: FlagUpdate::None,
                    },
                ));
                let new_value = ctx.alloc_vreg();
                let kind = match action {
                    5 => OpKind::Or {
                        dst: new_value,
                        src1: old_value,
                        src2: SrcOperand::Reg(mask),
                        width,
                        flags: FlagUpdate::None,
                    },
                    6 => OpKind::And {
                        dst: new_value,
                        src1: old_value,
                        src2: SrcOperand::Reg(mask),
                        width,
                        flags: FlagUpdate::None,
                    },
                    7 => OpKind::Xor {
                        dst: new_value,
                        src1: old_value,
                        src2: SrcOperand::Reg(mask),
                        width,
                        flags: FlagUpdate::None,
                    },
                    _ => unreachable!(),
                };
                if action == 6 {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Not {
                            dst: mask,
                            src: mask,
                            width,
                        },
                    ));
                }
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: new_value,
                        addr,
                        width: mem_width,
                    },
                ));
            }
        }

        // Commit CF only after any update store/atomic operation succeeds.
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Bt {
                src: old_value,
                index: bit_index,
                width,
            },
        ));
        Ok(ops)
    }


    /// Lift BT/BTS/BTR/BTC r/m,reg (0F A3/AB/B3/BB).
    pub(crate) fn lift_bit_test_reg(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let action = match opcode {
            0xA3 => 4,
            0xAB => 5,
            0xB3 => 6,
            0xBB => 7,
            _ => unreachable!(),
        };
        let index_reg = self.gpr(modrm.reg);
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let ops = self.emit_bit_test_ops(
            action,
            &modrm,
            SrcOperand::Reg(index_reg),
            modrm.is_memory.then_some(index_reg),
            width,
            mem_width,
            prefix,
            next_pc,
            pc,
            ctx,
        )?;
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift Group-8 BT/BTS/BTR/BTC r/m,imm8 (0F BA /4-/7).
    pub(crate) fn lift_bit_test_imm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if bytes.len() <= modrm.bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: modrm.bytes_consumed + 1,
            });
        }
        let action = (modrm.byte >> 3) & 7;
        let imm = bytes[modrm.bytes_consumed] as i64;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let ops = self.emit_bit_test_ops(
            action,
            &modrm,
            SrcOperand::Imm(imm),
            None,
            width,
            mem_width,
            prefix,
            next_pc,
            pc,
            ctx,
        )?;
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }


    /// Lift POPCNT/TZCNT/LZCNT (F3 0F B8/BC/BD), including their exact
    /// modeled RFLAGS effects. POPCNT clears the arithmetic status flags other
    /// than ZF; TZCNT/LZCNT replace CF/ZF and retain the emulator's values for
    /// their architecturally undefined status flags. DF is preserved throughout.
    pub(crate) fn lift_count_0f(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix != Some(0xF3) || prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        let source = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: loaded,
                    addr,
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            loaded
        } else {
            self.gpr(modrm.rm)
        };

        let dst = self.gpr(modrm.reg);
        let (kind, flags) = match opcode {
            0xB8 => (X86CountKind::Popcnt, FlagUpdate::All),
            0xBC => (
                X86CountKind::Tzcnt,
                FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
            ),
            0xBD => (
                X86CountKind::Lzcnt,
                FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
            ),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![opcode],
                });
            }
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Count {
                dst,
                src: source,
                width,
                kind,
                flags,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift SHLD/SHRD (0F A4/A5/AC/AD)
    pub(crate) fn lift_shld_shrd(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let uses_cl = opcode == 0xA5 || opcode == 0xAD;
        let is_shld = opcode == 0xA4 || opcode == 0xA5;

        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_size = if uses_cl { 0 } else { 1 };
        if bytes.len() < modrm.bytes_consumed + imm_size {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: modrm.bytes_consumed + imm_size,
            });
        }

        let imm = if uses_cl {
            0
        } else {
            bytes[modrm.bytes_consumed] as i8 as i64
        };

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();

        let (dst, addr) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr: addr.clone(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            (tmp, Some(addr))
        } else {
            (self.gpr(modrm.rm), None)
        };

        let amount = if uses_cl {
            SrcOperand::Reg(self.gpr(1))
        } else {
            SrcOperand::Imm(imm)
        };

        let op_kind = if is_shld {
            OpKind::Shld {
                dst,
                src: self.gpr(modrm.reg),
                amount,
                width,
                flags: FlagUpdate::All,
            }
        } else {
            OpKind::Shrd {
                dst,
                src: self.gpr(modrm.reg),
                amount,
                width,
                flags: FlagUpdate::All,
            }
        };

        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));

        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: dst,
                    addr,
                    width: mem_width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + imm_size,
        ))
    }
}
