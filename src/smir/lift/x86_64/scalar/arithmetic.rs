//! Integer arithmetic, ALU, multiply, compare-exchange lifting

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
    pub(crate) fn x86_alu_op(
        group: u8,
        dst: VReg,
        src1: VReg,
        src2: SrcOperand,
        width: OpWidth,
        flags: FlagUpdate,
    ) -> OpKind {
        match group {
            0 => OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            1 => OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            2 => OpKind::Adc {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            3 => OpKind::Sbb {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            4 => OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            5 => OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            6 => OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            },
            _ => unreachable!("x86 ALU group {group}"),
        }
    }

    /// Append one architecturally atomic x86 memory ALU operation. ADC/SBB
    /// fold the incoming CF into the atomic operand while retaining the
    /// original source for the post-retirement flag calculation.
    pub(crate) fn append_locked_alu(
        group: u8,
        source: VReg,
        addr: Address,
        width: OpWidth,
        mem_width: MemWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let atomic_source = if group == 2 || group == 3 {
            let saved_flags = ctx.alloc_vreg();
            let carry = ctx.alloc_vreg();
            let combined = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::ReadFlags { dst: saved_flags },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: carry,
                    src1: saved_flags,
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Add {
                    dst: combined,
                    src1: source,
                    src2: SrcOperand::Reg(carry),
                    width,
                    flags: FlagUpdate::None,
                },
            ));
            combined
        } else {
            source
        };
        let atomic_op = match group {
            0 | 2 => AtomicOp::Add,
            1 => AtomicOp::Or,
            3 | 5 => AtomicOp::Sub,
            4 => AtomicOp::And,
            6 => AtomicOp::Xor,
            _ => unreachable!("locked x86 ALU group {group}"),
        };
        let old = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::AtomicRmw {
                dst: old,
                addr,
                src: atomic_source,
                op: atomic_op,
                width: mem_width,
                order: MemoryOrder::SeqCst,
            },
        ));
        let flag_result = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            Self::x86_alu_op(
                group,
                flag_result,
                old,
                SrcOperand::Reg(source),
                width,
                FlagUpdate::All,
            ),
        ));
    }

    /// Lift arithmetic instruction (ADD, SUB, ADC, SBC, CMP)
    pub(crate) fn lift_arith(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        // Determine operation type from opcode
        let (is_8bit, dir_rm_reg) = match opcode & 0x07 {
            0 => (true, true),   // rm8, r8
            1 => (false, true),  // rm, r
            2 => (true, false),  // r8, rm8
            3 => (false, false), // r, rm
            4 => (true, true),   // AL, imm8 (handled separately)
            5 => (false, true),  // rAX, imm (handled separately)
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };

        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        if (opcode & 0x07) == 4 || (opcode & 0x07) == 5 {
            if prefix.lock {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let imm_size = if is_8bit {
                1
            } else if op_size == 2 {
                2
            } else {
                4
            };
            if bytes.len() < imm_size {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: bytes.len(),
                    need: imm_size,
                });
            }

            let imm = match imm_size {
                1 => bytes[0] as i8 as i64,
                2 => i16::from_le_bytes([bytes[0], bytes[1]]) as i64,
                _ => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
            };

            let dst = self.gpr(0);
            let hint = X86OpHint::AluEncoding(X86AluEncoding::AccImm);
            let op_kind = match (opcode >> 3) & 0x07 {
                0 => OpKind::Add {
                    dst,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags: FlagUpdate::All,
                },
                1 => OpKind::Or {
                    dst,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags: FlagUpdate::All,
                },
                2 => OpKind::Adc {
                    dst,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags: FlagUpdate::All,
                },
                3 => OpKind::Sbb {
                    dst,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags: FlagUpdate::All,
                },
                4 => OpKind::And {
                    dst,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags: FlagUpdate::All,
                },
                5 => OpKind::Sub {
                    dst,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags: FlagUpdate::All,
                },
                6 => OpKind::Xor {
                    dst,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags: FlagUpdate::All,
                },
                7 => {
                    let op = SmirOp::with_hint(
                        OpId(0),
                        pc,
                        OpKind::Cmp {
                            src1: dst,
                            src2: SrcOperand::Imm(imm),
                            width,
                        },
                        hint,
                    );
                    return Ok(LiftResult::fallthrough(vec![op], prefix.cursor + imm_size));
                }
                _ => unreachable!(),
            };

            let op = SmirOp::with_hint(OpId(0), pc, op_kind, hint);
            return Ok(LiftResult::fallthrough(vec![op], prefix.cursor + imm_size));
        }

        // Decode ModR/M
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let alu_group = (opcode >> 3) & 0x07;
        if prefix.lock {
            if !modrm.is_memory || !dir_rm_reg || alu_group == 7 {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let source = if is_8bit {
                self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
            } else {
                self.gpr(modrm.reg)
            };
            Self::append_locked_alu(
                alu_group,
                source,
                addr,
                width,
                self.size_to_memwidth(op_size),
                pc,
                ctx,
                &mut ops,
            );
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        let mut high_dst = None;
        let mut writeback_addr = None;

        // Get source and destination
        let (dst, src) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            if dir_rm_reg {
                // rm is destination, reg is source
                writeback_addr = Some(addr.clone());
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
                let src = if is_8bit {
                    self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
                } else {
                    self.gpr(modrm.reg)
                };
                (tmp, src)
            } else {
                // reg is destination, rm is source
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: tmp,
                        addr,
                        width: self.size_to_memwidth(op_size),
                        sign: SignExtend::Zero,
                    },
                ));
                let dst = if is_8bit {
                    high_dst = self.high_byte_base(modrm.reg, prefix);
                    self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
                } else {
                    self.gpr(modrm.reg)
                };
                (dst, tmp)
            }
        } else if dir_rm_reg {
            if is_8bit {
                high_dst = self.high_byte_base(modrm.rm, prefix);
                (
                    self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                    self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops),
                )
            } else {
                (self.gpr(modrm.rm), self.gpr(modrm.reg))
            }
        } else {
            if is_8bit {
                high_dst = self.high_byte_base(modrm.reg, prefix);
                (
                    self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops),
                    self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                )
            } else {
                (self.gpr(modrm.reg), self.gpr(modrm.rm))
            }
        };

        // Determine operation from opcode high bits. A memory-destination RMW
        // computes its value without flags, stores it, and only then replays
        // the operation to commit flags. A faulting store therefore leaves the
        // architectural flags unchanged.
        let hint = X86OpHint::AluEncoding(if dir_rm_reg {
            X86AluEncoding::RmReg
        } else {
            X86AluEncoding::RegRm
        });
        if alu_group == 7 {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::Cmp {
                    src1: dst,
                    src2: SrcOperand::Reg(src),
                    width,
                },
                hint,
            ));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        let make_alu = |result, flags| match alu_group {
            0 => OpKind::Add {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags,
            },
            1 => OpKind::Or {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags,
            },
            2 => OpKind::Adc {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags,
            },
            3 => OpKind::Sbb {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags,
            },
            4 => OpKind::And {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags,
            },
            5 => OpKind::Sub {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags,
            },
            6 => OpKind::Xor {
                dst: result,
                src1: dst,
                src2: SrcOperand::Reg(src),
                width,
                flags,
            },
            7 => {
                unreachable!()
            }
            _ => unreachable!(),
        };

        let result = if writeback_addr.is_some() {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            make_alu(
                result,
                if writeback_addr.is_some() {
                    FlagUpdate::None
                } else {
                    FlagUpdate::All
                },
            ),
            hint,
        ));

        if let Some(base) = high_dst {
            self.merge_high_byte(base, result, pc, ctx, &mut ops);
        }

        if let Some(addr) = writeback_addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: result,
                    addr,
                    width: self.size_to_memwidth(op_size),
                },
            ));
            let flags_result = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                make_alu(flags_result, FlagUpdate::All),
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift Group 1 immediate instructions (80/81/83)
    pub(crate) fn lift_group1_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = opcode == 0x80;
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;

        let (imm, imm_size) = match opcode {
            0x80 => {
                if bytes.len() < imm_offset + 1 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: imm_offset + 1,
                    });
                }
                (bytes[imm_offset] as i8 as i64, 1)
            }
            0x81 => {
                let imm_size = if op_size == 2 { 2 } else { 4 };
                if bytes.len() < imm_offset + imm_size {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: imm_offset + imm_size,
                    });
                }
                let imm = if imm_size == 2 {
                    i16::from_le_bytes([bytes[imm_offset], bytes[imm_offset + 1]]) as i64
                } else {
                    i32::from_le_bytes([
                        bytes[imm_offset],
                        bytes[imm_offset + 1],
                        bytes[imm_offset + 2],
                        bytes[imm_offset + 3],
                    ]) as i64
                };
                (imm, imm_size)
            }
            0x83 => {
                if bytes.len() < imm_offset + 1 {
                    return Err(LiftError::Incomplete {
                        addr: pc,
                        have: bytes.len(),
                        need: imm_offset + 1,
                    });
                }
                (bytes[imm_offset] as i8 as i64, 1)
            }
            _ => unreachable!(),
        };

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();
        let mut high_dst = None;
        let group = (modrm.byte >> 3) & 0x07;

        if prefix.lock {
            if !modrm.is_memory || group == 7 {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let source = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: source,
                    src: SrcOperand::Imm(imm),
                    width,
                },
            ));
            Self::append_locked_alu(group, source, addr, width, mem_width, pc, ctx, &mut ops);
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed + imm_size,
            ));
        }

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
        } else if is_8bit {
            high_dst = self.high_byte_base(modrm.rm, prefix);
            (
                self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                None,
            )
        } else {
            (self.gpr(modrm.rm), None)
        };

        if group == 7 {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Cmp {
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                },
            ));
        } else {
            let make_alu = |result, flags| match group {
                0 => OpKind::Add {
                    dst: result,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags,
                },
                1 => OpKind::Or {
                    dst: result,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags,
                },
                2 => OpKind::Adc {
                    dst: result,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags,
                },
                3 => OpKind::Sbb {
                    dst: result,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags,
                },
                4 => OpKind::And {
                    dst: result,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags,
                },
                5 => OpKind::Sub {
                    dst: result,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags,
                },
                6 => OpKind::Xor {
                    dst: result,
                    src1: dst,
                    src2: SrcOperand::Imm(imm),
                    width,
                    flags,
                },
                _ => unreachable!(),
            };
            let result = if addr.is_some() {
                ctx.alloc_vreg()
            } else {
                dst
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                make_alu(
                    result,
                    if addr.is_some() {
                        FlagUpdate::None
                    } else {
                        FlagUpdate::All
                    },
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
                        width: mem_width,
                    },
                ));
                let flag_result = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    make_alu(flag_result, FlagUpdate::All),
                ));
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + imm_size,
        ))
    }

    /// Lift CMPXCHG r/m, r (0F B0/0F B1).
    pub(crate) fn lift_cmpxchg(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_byte = opcode == 0xB0;
        let op_size = if is_byte { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        let acc = self.gpr(0);
        let src = if is_byte {
            self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
        } else {
            self.gpr(modrm.reg)
        };

        let saved_src = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: saved_src,
                src: SrcOperand::Reg(src),
                width,
            },
        ));

        let saved_acc = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: saved_acc,
                src: SrcOperand::Reg(acc),
                width,
            },
        ));

        let mut dst_high_base = None;
        let (old_dst, dst_reg, dst_addr) = if modrm.is_memory {
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
            (tmp, None, Some(addr))
        } else {
            let dst = if is_byte {
                dst_high_base = self.high_byte_base(modrm.rm, prefix);
                self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops)
            } else {
                self.gpr(modrm.rm)
            };
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: tmp,
                    src: SrcOperand::Reg(dst),
                    width,
                },
            ));
            (tmp, Some(self.gpr(modrm.rm)), None)
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Cmp {
                src1: saved_acc,
                src2: SrcOperand::Reg(old_dst),
                width,
            },
        ));

        let matched = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::SetCC {
                dst: matched,
                cond: Condition::Eq,
                width: OpWidth::W8,
            },
        ));

        if let Some(addr) = dst_addr {
            let new_dst = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: new_dst,
                    cond: matched,
                    src_true: saved_src,
                    src_false: old_dst,
                    width,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::PredStore {
                    src: SrcOperand::Reg(new_dst),
                    cond: matched,
                    addr,
                    width: mem_width,
                },
            ));
            // On a mismatch the accumulator takes the old destination value; on a
            // match it must be left UNCHANGED. The previous unconditional Select
            // wrote `saved_acc` back on the match path, and for a 32-bit CMPXCHG a
            // W32 write zero-extends — clearing RAX's upper half. A predicated
            // CMove only writes on the mismatch path (ZF=0 → Ne), preserving the
            // high bits on the no-op (match) path. The flags were set by the Cmp
            // above. (#21)
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::CMove {
                    dst: acc,
                    src: old_dst,
                    cond: Condition::Ne,
                    width,
                },
            ));
        } else if let Some(base) = dst_high_base {
            let new_dst = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: new_dst,
                    cond: matched,
                    src_true: saved_src,
                    src_false: old_dst,
                    width,
                },
            ));
            self.merge_high_byte(base, new_dst, pc, ctx, &mut ops);

            // AH/CH/DH/BH are distinct from AL even when AH aliases RAX.
            // On mismatch, AL receives the old high-byte destination.
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::CMove {
                    dst: acc,
                    src: old_dst,
                    cond: Condition::Ne,
                    width,
                },
            ));
        } else if let Some(dst) = dst_reg {
            // On a match the destination register takes the source; on a mismatch
            // it must be UNCHANGED. CMove writes only on the match path (Eq),
            // preserving the high bits on the no-op (mismatch) path — the old
            // Select's W32 write zero-extended and cleared the destination's upper
            // half. (#21)
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::CMove {
                    dst,
                    src: saved_src,
                    cond: Condition::Eq,
                    width,
                },
            ));

            if dst != acc {
                // On a mismatch the accumulator takes the old destination; on a
                // match it is UNCHANGED (see the memory path above). (#21)
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::CMove {
                        dst: acc,
                        src: old_dst,
                        cond: Condition::Ne,
                        width,
                    },
                ));
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift XADD r/m, r (0F C0/0F C1).
    pub(crate) fn lift_xadd(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_byte = opcode == 0xC0;
        let op_size = if is_byte { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let src_reg = self.gpr(modrm.reg);
        let mut ops = Vec::new();
        let src_value = if is_byte {
            self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
        } else {
            src_reg
        };

        let saved_src = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: saved_src,
                src: SrcOperand::Reg(src_value),
                width,
            },
        ));

        if modrm.is_memory {
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let old_dst = ctx.alloc_vreg();
            if prefix.lock {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::AtomicRmw {
                        dst: old_dst,
                        addr: addr.clone(),
                        src: saved_src,
                        op: AtomicOp::Add,
                        width: mem_width,
                        order: MemoryOrder::SeqCst,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: old_dst,
                        addr: addr.clone(),
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
            }

            // A non-LOCK memory XADD does a *separate* store that can fault (e.g.
            // a read-only page). Architecturally a faulting XADD must leave both
            // the flags and the source register unchanged, so the flag update must
            // not be committed before the store retires. Compute the sum WITHOUT
            // flags here; the flags are emitted after the store/writeback below.
            // A LOCK XADD instead uses the AtomicRmw above, which has already
            // committed the memory update, so its flags are computed here. (#23)
            let sum = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Add {
                    dst: sum,
                    src1: old_dst,
                    src2: SrcOperand::Reg(saved_src),
                    width,
                    flags: if prefix.lock {
                        FlagUpdate::All
                    } else {
                        FlagUpdate::None
                    },
                },
            ));

            if !prefix.lock {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: sum,
                        addr,
                        width: mem_width,
                    },
                ));
            }

            if is_byte {
                self.write_byte_reg(modrm.reg, prefix, old_dst, pc, ctx, &mut ops);
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: src_reg,
                        src: SrcOperand::Reg(old_dst),
                        width,
                    },
                ));
            }

            if !prefix.lock {
                // Now that the store has retired, commit the arithmetic flags. If
                // the store faulted, none of these ops execute, so a faulting XADD
                // leaves flags and the source register unchanged. (#23)
                let flag_tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Add {
                        dst: flag_tmp,
                        src1: old_dst,
                        src2: SrcOperand::Reg(saved_src),
                        width,
                        flags: FlagUpdate::All,
                    },
                ));
            }

            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed].to_vec(),
            });
        }

        let dst_reg = self.gpr(modrm.rm);
        let dst_value = if is_byte {
            self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops)
        } else {
            dst_reg
        };
        let old_dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: old_dst,
                src: SrcOperand::Reg(dst_value),
                width,
            },
        ));

        let sum = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Add {
                dst: sum,
                src1: old_dst,
                src2: SrcOperand::Reg(saved_src),
                width,
                flags: FlagUpdate::All,
            },
        ));

        if is_byte {
            self.write_byte_reg(modrm.reg, prefix, old_dst, pc, ctx, &mut ops);
            self.write_byte_reg(modrm.rm, prefix, sum, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: src_reg,
                    src: SrcOperand::Reg(old_dst),
                    width,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: dst_reg,
                    src: SrcOperand::Reg(sum),
                    width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_crc32_0f38(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix != Some(0xF2) || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let data_width = if opcode == 0xF0 {
            OpWidth::W8
        } else if prefix.rex_w() {
            OpWidth::W64
        } else if prefix.operand_size_override {
            OpWidth::W16
        } else {
            OpWidth::W32
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let data = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: loaded,
                    addr,
                    width: match data_width {
                        OpWidth::W8 => MemWidth::B1,
                        OpWidth::W16 => MemWidth::B2,
                        OpWidth::W32 => MemWidth::B4,
                        OpWidth::W64 => MemWidth::B8,
                        OpWidth::W128 => unreachable!(),
                    },
                    sign: SignExtend::Zero,
                },
            ));
            loaded
        } else if data_width == OpWidth::W8 {
            self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops)
        } else {
            self.gpr(modrm.rm)
        };
        let dst = self.gpr(modrm.reg);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Crc32C {
                dst,
                crc: dst,
                data,
                data_width,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_adx_0f38(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let kind = match prefix.rep_prefix {
            Some(0xF3) => X86AdxKind::Adox,
            Some(_) => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            None if prefix.operand_size_override => X86AdxKind::Adcx,
            None => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };

        let width = if prefix.rex_w() {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        self.lift_adx_modrm(bytes, prefix, pc, ctx, kind, width, None)
    }

    pub(crate) fn lift_adx_modrm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        kind: X86AdxKind,
        width: OpWidth,
        dst_override: Option<VReg>,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mem_width = match width {
            OpWidth::W32 => MemWidth::B4,
            OpWidth::W64 => MemWidth::B8,
            _ => unreachable!("ADX is only defined for 32- and 64-bit operands"),
        };
        let mut ops = Vec::new();

        let src1 = self.gpr(modrm.reg);
        let src2 = if modrm.is_memory {
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
        let dst = dst_override.unwrap_or(src1);
        let flags = FlagUpdate::Specific(match kind {
            X86AdxKind::Adcx => FlagSet::CF,
            X86AdxKind::Adox => FlagSet::OF,
        });
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Adx {
                dst,
                src1,
                src2,
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

    /// Lift IMUL r, r/m, imm (69/6B)
    pub(crate) fn lift_imul_rmi(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        let imm_size = if opcode == 0x6B {
            1
        } else if op_size == 2 {
            2
        } else {
            4
        };

        if bytes.len() < imm_offset + imm_size {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + imm_size,
            });
        }

        let imm = match imm_size {
            1 => bytes[imm_offset] as i8 as i64,
            2 => i16::from_le_bytes([bytes[imm_offset], bytes[imm_offset + 1]]) as i64,
            _ => i32::from_le_bytes([
                bytes[imm_offset],
                bytes[imm_offset + 1],
                bytes[imm_offset + 2],
                bytes[imm_offset + 3],
            ]) as i64,
        };

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();

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
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        let hint = if opcode == 0x6B {
            X86OpHint::ImulImm8
        } else {
            X86OpHint::ImulImm32
        };

        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::MulS {
                dst_lo: self.gpr(modrm.reg),
                dst_hi: None,
                src1: src,
                src2: SrcOperand::Imm(imm),
                width,
                flags: FlagUpdate::All,
            },
            hint,
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + imm_size,
        ))
    }

    /// Lift CWD/CDQ/CQO (99)
    pub(crate) fn lift_cwd_cdq_cqo(
        &self,
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let width = self.size_to_width(prefix.op_size());
        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::Cwd {
                dst: self.gpr(2),
                src: self.gpr(0),
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift CBW/CWDE/CDQE (98).
    pub(crate) fn lift_cbw_cwde_cdqe(
        &self,
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x98],
            });
        }
        let (from_width, to_width) = if prefix.rex_w() {
            (OpWidth::W32, OpWidth::W64)
        } else if prefix.operand_size_override {
            (OpWidth::W8, OpWidth::W16)
        } else {
            (OpWidth::W16, OpWidth::W32)
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::SignExtend {
                    dst: self.gpr(0),
                    src: self.gpr(0),
                    from_width,
                    to_width,
                },
            )],
            prefix.cursor,
        ))
    }

    /// Lift TEST r/m, r (84/85)
    pub(crate) fn lift_test_rm_r(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = (opcode & 0x01) == 0;
        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let (src1, src2) = if modrm.is_memory {
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
                    width: self.size_to_memwidth(op_size),
                    sign: SignExtend::Zero,
                },
            ));
            let reg = if is_8bit {
                self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
            } else {
                self.gpr(modrm.reg)
            };
            (tmp, reg)
        } else {
            if is_8bit {
                (
                    self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops),
                    self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops),
                )
            } else {
                (self.gpr(modrm.rm), self.gpr(modrm.reg))
            }
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Test {
                src1,
                src2: SrcOperand::Reg(src2),
                width,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift TEST AL/AX/EAX/RAX, imm (A8/A9)
    pub(crate) fn lift_test_acc_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let (width, imm_size): (OpWidth, usize) = if opcode == 0xA8 {
            (OpWidth::W8, 1)
        } else {
            let op_size = prefix.op_size();
            (
                self.size_to_width(op_size),
                if op_size == 8 { 4 } else { op_size as usize },
            )
        };

        if bytes.len() < imm_size {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_size,
            });
        }

        let imm = match imm_size {
            1 => bytes[0] as i8 as i64,
            2 => i16::from_le_bytes([bytes[0], bytes[1]]) as i64,
            4 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
            _ => unreachable!("invalid TEST accumulator immediate size"),
        };

        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::Test {
                src1: self.gpr(0),
                src2: SrcOperand::Imm(imm),
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_size))
    }

    /// Lift XOR r/m, r and XOR r, r/m (30-33)
    pub(crate) fn lift_xor_rm_r(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = (opcode & 0x01) == 0;
        let dir_reg_rm = (opcode & 0x02) != 0;

        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let hint = X86OpHint::AluEncoding(if dir_reg_rm {
            X86AluEncoding::RegRm
        } else {
            X86AluEncoding::RmReg
        });

        let (dst, src1, src2) = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            if dir_reg_rm {
                // XOR r, rm
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: tmp,
                        addr,
                        width: self.size_to_memwidth(op_size),
                        sign: SignExtend::Zero,
                    },
                ));
                (self.gpr(modrm.reg), self.gpr(modrm.reg), tmp)
            } else {
                // XOR rm, r - load-modify-store
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
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Xor {
                        dst: tmp,
                        src1: tmp,
                        src2: SrcOperand::Reg(self.gpr(modrm.reg)),
                        width,
                        flags: FlagUpdate::All,
                    },
                    hint,
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: tmp,
                        addr,
                        width: self.size_to_memwidth(op_size),
                    },
                ));
                return Ok(LiftResult::fallthrough(
                    ops,
                    prefix.cursor + modrm.bytes_consumed,
                ));
            }
        } else if dir_reg_rm {
            (self.gpr(modrm.reg), self.gpr(modrm.reg), self.gpr(modrm.rm))
        } else {
            (self.gpr(modrm.rm), self.gpr(modrm.rm), self.gpr(modrm.reg))
        };

        let result = dst;
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::Xor {
                dst: result,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags: FlagUpdate::All,
            },
            hint,
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
