//! APX (Advanced Performance Extensions) instruction lifting

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86OpHint,
    X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
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

mod alu;
mod bmi;
mod cmpccxadd;
mod conditional;
mod count;
mod invalid;
mod movbe;
mod movrs;
mod paired_stack;
mod shift;

impl X86_64Lifter {
    pub(crate) fn lift_apx_group3(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm_byte = Self::apx_operand_modrm_byte(prefix, bytes, pc)?;
        let group = (modrm_byte >> 3) & 0x07;
        // Intel APX revision 7.0 specifies {NF=0} for NOT. ModR/M.reg=/2
        // establishes the reserved form before SIB, displacement, or memory.
        if prefix.nf && group == 2 {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }
        if matches!(group, 0 | 1) {
            return self.lift_apx_ctest_imm(prefix, opcode, bytes, pc, ctx);
        }
        // Intel APX revision 7.0 specifies {ND=0} and optional {NF} for the
        // implicit MUL/IMUL/DIV/IDIV forms. ModR/M.reg=/4..=/7 establishes an
        // ND=1 reserved encoding before address generation or operand access.
        if matches!(group, 4..=7) && prefix.nd {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let is_byte = opcode == 0xF6;
        let op_size = prefix.op_size(is_byte);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;

        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let (src, store_addr) = if modrm.is_memory {
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

        let dst = if prefix.nd {
            self.gpr(prefix.vvvv_reg())
        } else {
            src
        };

        match group {
            2 => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Not { dst, src, width },
            )),
            3 => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Neg {
                    dst,
                    src,
                    width,
                    flags: prefix.flags(),
                },
            )),
            4 => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::MulU {
                    dst_lo: self.gpr(0),
                    dst_hi: (width != OpWidth::W8).then_some(self.gpr(2)),
                    src1: self.gpr(0),
                    src2: SrcOperand::Reg(src),
                    width,
                    flags: prefix.flags(),
                },
            )),
            5 => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::MulS {
                    dst_lo: self.gpr(0),
                    dst_hi: (width != OpWidth::W8).then_some(self.gpr(2)),
                    src1: self.gpr(0),
                    src2: SrcOperand::Reg(src),
                    width,
                    flags: prefix.flags(),
                },
            )),
            6 => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::DivU {
                    quot: self.gpr(0),
                    rem: (width != OpWidth::W8).then_some(self.gpr(2)),
                    src1: self.gpr(0),
                    src2: SrcOperand::Reg(src),
                    width,
                    flags: prefix.flags(),
                },
            )),
            7 => ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::DivS {
                    quot: self.gpr(0),
                    rem: (width != OpWidth::W8).then_some(self.gpr(2)),
                    src1: self.gpr(0),
                    src2: SrcOperand::Reg(src),
                    width,
                    flags: prefix.flags(),
                },
            )),
            _ => unreachable!(),
        }

        if !prefix.nd && matches!(group, 2 | 3) {
            if let Some(addr) = store_addr {
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
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_inc_dec(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let Some(&modrm_byte) = bytes.first() else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        };
        let group = (modrm_byte >> 3) & 0x07;
        if !matches!(group, 0 | 1) {
            return Ok(Self::apx_modrm_invalid_opcode(prefix));
        }

        let is_byte = opcode == 0xFE;
        let op_size = prefix.op_size(is_byte);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;

        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let (src, store_addr) = if modrm.is_memory {
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

        let dst = if prefix.nd {
            self.gpr(prefix.vvvv_reg())
        } else {
            src
        };

        if group == 0 {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Inc {
                    dst,
                    src,
                    width,
                    flags: prefix.flags(),
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Dec {
                    dst,
                    src,
                    width,
                    flags: prefix.flags(),
                },
            ));
        }

        if !prefix.nd {
            if let Some(addr) = store_addr {
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
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_rao_int(
        &self,
        prefix: ApxEvexPrefix,
        bytes: &[u8],
        full_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.nd
            || prefix.nf
            || prefix.aaa != 0
            || prefix.vvvv != 0x0F
            || !prefix.v_prime
            || (full_bytes[prefix.bytes - 1] & 0xE0) != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: full_bytes.to_vec(),
            });
        }

        let op = match prefix.pp {
            0 => AtomicOp::Add,
            1 => AtomicOp::And,
            2 => AtomicOp::Or,
            3 => AtomicOp::Xor,
            _ => unreachable!(),
        };
        let width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        self.lift_rao_int_modrm(bytes, &modrm_prefix, pc, ctx, op, width)
    }

    pub(crate) fn lift_apx_adx(
        &self,
        prefix: ApxEvexPrefix,
        bytes: &[u8],
        full_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.nd || prefix.nf || (full_bytes[prefix.bytes - 1] & 0x80) != 0 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: full_bytes.to_vec(),
            });
        }

        let kind = match prefix.pp {
            1 => X86AdxKind::Adcx,
            2 => X86AdxKind::Adox,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: full_bytes.to_vec(),
                });
            }
        };
        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let dst = self.gpr(prefix.vvvv_reg());
        self.lift_adx_modrm(bytes, &modrm_prefix, pc, ctx, kind, width, Some(dst))
    }

    pub(crate) fn lift_apx_setzucc(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let cond = self.x86_cond(opcode & 0x0F);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::SetCC {
                    dst: tmp,
                    cond,
                    width: OpWidth::W8,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: tmp,
                    addr,
                    width: MemWidth::B1,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::SetCC {
                    dst: self.gpr(modrm.rm),
                    cond,
                    width: OpWidth::W64,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn push_apx_condition(
        &self,
        ops: &mut Vec<SmirOp>,
        pc: u64,
        ctx: &mut LiftContext,
        cond: Condition,
    ) -> VReg {
        let cond_reg = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::SetCC {
                dst: cond_reg,
                cond,
                width: OpWidth::W8,
            },
        ));
        cond_reg
    }

    pub(crate) fn lift_apx_evex_setcc(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let cond = self.x86_cond(opcode & 0x0F);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::SetCC {
                    dst: tmp,
                    cond,
                    width: OpWidth::W8,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: tmp,
                    addr,
                    width: MemWidth::B1,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::SetCC {
                    dst: self.gpr(modrm.rm),
                    cond,
                    width: OpWidth::W8,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_cmovcc(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let cond = self.x86_cond(opcode & 0x0F);
        let op_size = prefix.op_size(false);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        if prefix.nd {
            let dst = self.gpr(prefix.vvvv_reg());
            let src1 = self.gpr(modrm.reg);

            if prefix.nf {
                let cond_reg = self.push_apx_condition(&mut ops, pc, ctx, cond);
                if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);

                    let loaded = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: loaded,
                            cond: cond_reg,
                            addr,
                            width: mem_width,
                            signed: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Select {
                            dst,
                            cond: cond_reg,
                            src_true: loaded,
                            src_false: src1,
                            width,
                        },
                    ));
                } else {
                    let src2 = self.gpr(modrm.rm);
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Select {
                            dst,
                            cond: cond_reg,
                            src_true: src2,
                            src_false: src1,
                            width,
                        },
                    ));
                }
            } else {
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

                let cond_reg = self.push_apx_condition(&mut ops, pc, ctx, cond);
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Select {
                        dst,
                        cond: cond_reg,
                        src_true: src2,
                        src_false: src1,
                        width,
                    },
                ));
            }
        } else if prefix.nf {
            let src = self.gpr(modrm.reg);
            let cond_reg = self.push_apx_condition(&mut ops, pc, ctx, cond);

            if modrm.is_memory {
                let x86_addr = modrm.addr.as_ref().unwrap();
                let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                ops.extend(pre_ops);

                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredStore {
                        src: SrcOperand::Reg(src),
                        cond: cond_reg,
                        addr,
                        width: mem_width,
                    },
                ));
            } else {
                let dst = self.gpr(modrm.rm);
                let zero = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: zero,
                        src: SrcOperand::Imm(0),
                        width,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Select {
                        dst,
                        cond: cond_reg,
                        src_true: src,
                        src_false: zero,
                        width,
                    },
                ));
            }
        } else {
            let dst = self.gpr(modrm.reg);
            let zero = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: zero,
                    src: SrcOperand::Imm(0),
                    width,
                },
            ));
            let cond_reg = self.push_apx_condition(&mut ops, pc, ctx, cond);

            let src = if modrm.is_memory {
                let x86_addr = modrm.addr.as_ref().unwrap();
                let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                ops.extend(pre_ops);

                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: loaded,
                        cond: cond_reg,
                        addr,
                        width: mem_width,
                        signed: SignExtend::Zero,
                    },
                ));
                loaded
            } else {
                self.gpr(modrm.rm)
            };

            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst,
                    cond: cond_reg,
                    src_true: src,
                    src_false: zero,
                    width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_conditional_map4(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp == 0x02 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(2)].to_vec(),
            });
        }

        if prefix.pp == 0x03 && !prefix.nf {
            if prefix.nd {
                self.lift_apx_setzucc(prefix, opcode, bytes, pc, ctx)
            } else {
                self.lift_apx_evex_setcc(prefix, opcode, bytes, pc, ctx)
            }
        } else {
            self.lift_apx_cmovcc(prefix, opcode, bytes, pc, ctx)
        }
    }

    pub(crate) fn lift_apx_imul_reg(
        &self,
        prefix: ApxEvexPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = prefix.op_size(false);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
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

        let dst = if prefix.nd {
            self.gpr(prefix.vvvv_reg())
        } else {
            src1
        };
        let src2_operand = SrcOperand::Reg(src2);

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::MulS {
                dst_lo: dst,
                dst_hi: None,
                src1,
                src2: src2_operand,
                width,
                flags: prefix.flags(),
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_apx_imul_imm(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let op_size = prefix.op_size(false);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
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

        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();
        let src1 = if modrm.is_memory {
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
        let dst = if prefix.nd {
            self.gpr(prefix.vvvv_reg())
        } else {
            self.gpr(modrm.reg)
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
                dst_lo: dst,
                dst_hi: None,
                src1,
                src2: SrcOperand::Imm(imm),
                width,
                flags: prefix.flags(),
            },
            hint,
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed + imm_size,
        ))
    }

    pub(crate) fn lift_apx_double_shift(
        &self,
        prefix: ApxEvexPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let uses_cl = matches!(opcode, 0xA5 | 0xAD);
        let is_shld = matches!(opcode, 0x24 | 0xA5);
        let op_size = prefix.op_size(false);
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = decode_modrm(bytes, &modrm_prefix, pc)?;
        let imm_size = if uses_cl { 0 } else { 1 };
        if bytes.len() < modrm.bytes_consumed + imm_size {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: modrm.bytes_consumed + imm_size,
            });
        }

        let amount = if uses_cl {
            SrcOperand::Reg(self.gpr(1))
        } else {
            SrcOperand::Imm(bytes[modrm.bytes_consumed] as i8 as i64)
        };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64 + imm_size as u64;
        let mut ops = Vec::new();
        let (legacy_dst, legacy_dst_addr) = if modrm.is_memory {
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

        if prefix.nd {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86NddDoubleShift {
                    dst: self.gpr(prefix.vvvv_reg()),
                    base: legacy_dst,
                    fill: self.gpr(modrm.reg),
                    amount,
                    width,
                    left: is_shld,
                    flags: prefix.flags(),
                },
            ));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.bytes + 1 + modrm.bytes_consumed + imm_size,
            ));
        }

        let architectural_dst = if prefix.nd {
            self.gpr(prefix.vvvv_reg())
        } else {
            legacy_dst
        };
        let amount_uses_cl = matches!(amount, SrcOperand::Reg(reg) if reg == self.gpr(1));
        let op_dst = if prefix.nd
            && amount_uses_cl
            && architectural_dst == self.gpr(1)
            && architectural_dst != legacy_dst
        {
            ctx.alloc_vreg()
        } else {
            architectural_dst
        };
        let mut src = self.gpr(modrm.reg);

        if prefix.nd && op_dst == src && op_dst != legacy_dst {
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: tmp,
                    src: SrcOperand::Reg(src),
                    width,
                },
            ));
            src = tmp;
        }

        if op_dst != legacy_dst {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: op_dst,
                    src: SrcOperand::Reg(legacy_dst),
                    width,
                },
            ));
        }

        let op_kind = if is_shld {
            OpKind::Shld {
                dst: op_dst,
                src,
                amount,
                width,
                flags: prefix.flags(),
            }
        } else {
            OpKind::Shrd {
                dst: op_dst,
                src,
                amount,
                width,
                flags: prefix.flags(),
            }
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op_kind));

        if op_dst != architectural_dst {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: architectural_dst,
                    src: SrcOperand::Reg(op_dst),
                    width,
                },
            ));
        }

        if !prefix.nd {
            if let Some(addr) = legacy_dst_addr {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: op_dst,
                        addr,
                        width: mem_width,
                    },
                ));
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed + imm_size,
        ))
    }

    pub(crate) fn lift_apx_evex_map4(
        &self,
        pc: u64,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let prefix = decode_apx_evex_prefix(bytes, pc)?;
        if bytes.len() < prefix.bytes + 1 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.bytes + 1,
            });
        }

        let opcode = bytes[prefix.bytes];
        let nf_carry_opcode = prefix.nf
            && matches!(
                opcode,
                0x10..=0x13 | 0x18..=0x1B
            );
        let fixed_conditional_fields_invalid = matches!(opcode, 0x38..=0x3B | 0x84 | 0x85)
            && !Self::apx_conditional_opcode_fields_valid(prefix, opcode);
        let fixed_movrs_fields_invalid =
            matches!(opcode, 0x8A | 0x8B) && !Self::apx_movrs_opcode_fields_valid(prefix, opcode);
        let opcode_is_terminal = nf_carry_opcode
            || opcode == 0x82
            || fixed_conditional_fields_invalid
            || fixed_movrs_fields_invalid;
        if !opcode_is_terminal && bytes.len() < prefix.bytes + 2 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix.bytes + 2,
            });
        }
        match opcode {
            0x00..=0x03
            | 0x08..=0x0B
            | 0x10..=0x13
            | 0x18..=0x1B
            | 0x20..=0x23
            | 0x28..=0x2B
            | 0x30..=0x33 => self.lift_apx_alu(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx),
            0x38..=0x3B => self.lift_apx_ccmp(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx),
            0x80 | 0x81 | 0x83 => {
                self.lift_apx_group1_imm(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0x82 => Ok(Self::apx_invalid_opcode(prefix.bytes + 1)),
            0x84 | 0x85 => {
                self.lift_apx_ctest_reg(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0xC0 | 0xC1 | 0xD0 | 0xD1 | 0xD2 | 0xD3 => {
                self.lift_apx_shift(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0x24 | 0x2C | 0xA5 | 0xAD => {
                self.lift_apx_double_shift(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0x40..=0x4F => {
                self.lift_apx_conditional_map4(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0x60 | 0x61 => self.lift_apx_movbe(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx),
            0x66 => self.lift_apx_adx(prefix, &bytes[prefix.bytes + 1..], bytes, pc, ctx),
            0x8A | 0x8B => self.lift_apx_movrs(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx),
            0x69 | 0x6B => {
                self.lift_apx_imul_imm(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0xAF => self.lift_apx_imul_reg(prefix, &bytes[prefix.bytes + 1..], pc, ctx),
            0x88 | 0xF4 | 0xF5 => {
                self.lift_apx_count(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0xF0 | 0xF1 => {
                self.lift_apx_crc32(prefix, opcode, &bytes[prefix.bytes + 1..], bytes, pc, ctx)
            }
            0xF2 => self.lift_apx_invpcid(prefix, &bytes[prefix.bytes + 1..], bytes, pc, ctx),
            0xF8 if prefix.pp == 1 => {
                self.lift_apx_movdir64b(prefix, &bytes[prefix.bytes + 1..], bytes, pc, ctx)
            }
            0xF9 => self.lift_apx_movdiri(prefix, &bytes[prefix.bytes + 1..], bytes, pc, ctx),
            0xFC => self.lift_apx_rao_int(prefix, &bytes[prefix.bytes + 1..], bytes, pc, ctx),
            0xF6 | 0xF7 => {
                self.lift_apx_group3(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
            }
            0xFE => self.lift_apx_inc_dec(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx),
            0x8F => {
                let modrm = Self::apx_operand_modrm_byte(prefix, &bytes[prefix.bytes + 1..], pc)?;
                self.lift_apx_pop2(prefix, modrm, pc, ctx)
            }
            0xFF => {
                let modrm = Self::apx_operand_modrm_byte(prefix, &bytes[prefix.bytes + 1..], pc)?;
                if ((modrm >> 3) & 0x07) == 6 {
                    self.lift_apx_push2(prefix, modrm, pc, ctx)
                } else {
                    self.lift_apx_inc_dec(prefix, opcode, &bytes[prefix.bytes + 1..], pc, ctx)
                }
            }
            _ => {
                // This fallback still includes valid-but-unimplemented APX
                // encodings (notably WRUSS and several F8 families). Preserve
                // its historical ModR/M frontier until those opcode classes
                // are classified and implemented independently.
                let _ = Self::apx_operand_modrm_byte(prefix, &bytes[prefix.bytes + 1..], pc)?;
                Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("APX MAP4 opcode 0x{opcode:02X}"),
                })
            }
        }
    }
}
