//! MOV family, stack push/pop, exchange, and LEA lifting

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86EnterOp, X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign,
    X86VecMap, X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource,
    X86X87Constant, X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth,
    X86X87IntWidth, X86XSaveKind,
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
    /// Lift XCHG r/m, r (86/87).
    pub(crate) fn lift_xchg_rm_r(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_byte = opcode == 0x86;
        let op_size = if is_byte { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if prefix.lock && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }
        if modrm.is_memory {
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, mut ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            let src = if is_byte {
                self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
            } else {
                self.gpr(modrm.reg)
            };
            let old = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::AtomicRmw {
                    dst: old,
                    addr,
                    src,
                    op: AtomicOp::Swap,
                    width: mem_width,
                    order: MemoryOrder::SeqCst,
                },
            ));
            if is_byte {
                self.write_byte_reg(modrm.reg, prefix, old, pc, ctx, &mut ops);
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: self.gpr(modrm.reg),
                        src: SrcOperand::Reg(old),
                        width,
                    },
                ));
            }

            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        if is_byte {
            // AH/CH/DH/BH have no standalone architectural-register identity
            // in SMIR and must retain the explicit extract/merge graph below.
            // Every other register form is an exact low-byte exchange and can
            // use the canonical operation directly, including SPL/BPL/SIL/DIL,
            // R8B-R15B, and REX2-addressed APX EGPR bytes.
            if self.high_byte_base(modrm.rm, prefix).is_none()
                && self.high_byte_base(modrm.reg, prefix).is_none()
            {
                return Ok(LiftResult::fallthrough(
                    vec![SmirOp::new(
                        OpId(0),
                        pc,
                        OpKind::Xchg {
                            reg1: self.gpr(modrm.rm),
                            reg2: self.gpr(modrm.reg),
                            width: OpWidth::W8,
                        },
                    )],
                    prefix.cursor + modrm.bytes_consumed,
                ));
            }

            let mut ops = Vec::new();
            let rm = self.read_byte_reg(modrm.rm, prefix, pc, ctx, &mut ops);
            let reg = self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops);
            let old_rm = ctx.alloc_vreg();
            let old_reg = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: old_rm,
                    src: SrcOperand::Reg(rm),
                    width: OpWidth::W8,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: old_reg,
                    src: SrcOperand::Reg(reg),
                    width: OpWidth::W8,
                },
            ));
            self.write_byte_reg(modrm.rm, prefix, old_reg, pc, ctx, &mut ops);
            self.write_byte_reg(modrm.reg, prefix, old_rm, pc, ctx, &mut ops);
            Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ))
        } else {
            Ok(LiftResult::fallthrough(
                vec![SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::Xchg {
                        reg1: self.gpr(modrm.rm),
                        reg2: self.gpr(modrm.reg),
                        width,
                    },
                )],
                prefix.cursor + modrm.bytes_consumed,
            ))
        }
    }

    pub(crate) fn lift_movnti(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock
            || prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Store {
                src: self.gpr(modrm.reg),
                addr,
                width: if prefix.rex_w() {
                    MemWidth::B8
                } else {
                    MemWidth::B4
                },
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_movbe_0f38(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rep_prefix == Some(0xF3) {
            let mut err_bytes = vec![opcode];
            err_bytes.extend_from_slice(bytes);
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: err_bytes,
            });
        }

        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            let mut err_bytes = vec![opcode];
            err_bytes.extend_from_slice(bytes);
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: err_bytes,
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let x86_addr = modrm.addr.as_ref().unwrap();
        let (addr, mut ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);

        match opcode {
            0xF0 => {
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
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Bswap {
                        dst: self.gpr(modrm.reg),
                        src: tmp,
                        width,
                    },
                ));
            }
            0xF1 => {
                let tmp = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Bswap {
                        dst: tmp,
                        src: self.gpr(modrm.reg),
                        width,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: tmp,
                        addr,
                        width: mem_width,
                    },
                ));
            }
            _ => unreachable!("MOVBE is only dispatched for opcodes F0/F1"),
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift POP r/m16 or r/m64 (8F /0) in 64-bit mode.
    pub(crate) fn lift_pop_rm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if ((modrm.byte >> 3) & 0x07) != 0 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }

        let (stack_bytes, width, mem_width) = if prefix.stack_op_size() == 2 {
            (2, OpWidth::W16, MemWidth::B2)
        } else {
            (8, OpWidth::W64, MemWidth::B8)
        };

        if !modrm.is_memory {
            let destination = self.gpr(modrm.rm);
            let mut ops = Vec::new();
            if destination != self.rsp() {
                // Use the canonical register-POP graph shared with 58+rd. The
                // helper-backed native fusion commits the load only on success,
                // then performs the architectural stack increment.
                ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::Load {
                        dst: destination,
                        addr: Address::Direct(self.rsp()),
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(1),
                    pc,
                    OpKind::Add {
                        dst: self.rsp(),
                        src1: self.rsp(),
                        src2: SrcOperand::Imm(stack_bytes),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            } else {
                let popped = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::Load {
                        dst: popped,
                        addr: Address::Direct(self.rsp()),
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
                if width == OpWidth::W16 {
                    let incremented_rsp = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(1),
                        pc,
                        OpKind::Add {
                            dst: incremented_rsp,
                            src1: self.rsp(),
                            src2: SrcOperand::Imm(stack_bytes),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(2),
                        pc,
                        OpKind::Mov {
                            dst: self.rsp(),
                            src: SrcOperand::Reg(incremented_rsp),
                            width: OpWidth::W64,
                        },
                    ));
                }
                // POP RSP discards the otherwise implicit increment. POP SP
                // first retains its carry, then replaces only the low 16 bits.
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: self.rsp(),
                        src: SrcOperand::Reg(popped),
                        width,
                    },
                ));
            }
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let popped = ctx.alloc_vreg();

        // POP obtains the value at the old RSP, increments RSP, then evaluates a
        // memory destination using the updated RSP (relevant to POP [RSP+disp]).
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Load {
                dst: popped,
                addr: Address::Direct(self.rsp()),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        ));
        let incremented_rsp = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Add {
                dst: incremented_rsp,
                src1: self.rsp(),
                src2: SrcOperand::Imm(stack_bytes),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        let x86_addr = modrm
            .addr
            .as_ref()
            .expect("register POP returned before memory destination lowering");
        let address_uses_materialized_addr32 = x86_addr.address_width == OpWidth::W32;
        let (addr, mut post_ops) = if address_uses_materialized_addr32 {
            // POP evaluates an ESP-based memory destination after the
            // architectural stack increment. Substitute the incremented
            // value before materializing the modulo-2^32 address.
            self.x86_addr32_to_smir(x86_addr, next_pc, ctx, Some((4, incremented_rsp)))
        } else {
            self.x86_addr_to_smir(x86_addr, next_pc, ctx)
        };
        for op in &mut post_ops {
            op.id = OpId(ops.len() as u16);
            ops.push(op.clone());
        }
        let addr = if address_uses_materialized_addr32 {
            addr
        } else {
            Self::replace_address_reg(addr, self.rsp(), incremented_rsp)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Store {
                src: popped,
                addr,
                width: mem_width,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: self.rsp(),
                src: SrcOperand::Reg(incremented_rsp),
                width: OpWidth::W64,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift MOV accumulator to/from a moffs absolute offset (A0-A3).
    pub(crate) fn lift_mov_moffs(
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
        let addr_bytes = if prefix.address_size_override { 4 } else { 8 };
        if bytes.len() < addr_bytes {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: addr_bytes,
            });
        }
        let offset = if addr_bytes == 4 {
            u32::from_le_bytes(bytes[..4].try_into().unwrap()) as u64
        } else {
            u64::from_le_bytes(bytes[..8].try_into().unwrap())
        };
        let addr = match prefix.segment_override {
            Some(0x64) => Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: None,
                index: None,
                scale: 1,
                disp: offset as i64,
            },
            Some(0x65) => Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                base: None,
                index: None,
                scale: 1,
                disp: offset as i64,
            },
            _ => Address::Absolute(offset),
        };
        let mem_width = if matches!(opcode, 0xA0 | 0xA2) {
            MemWidth::B1
        } else {
            let size = prefix.op_size();
            self.size_to_memwidth(size)
        };

        let kind = match opcode {
            0xA0 | 0xA1 => OpKind::Load {
                dst: self.gpr(0),
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
            },
            0xA2 | 0xA3 => OpKind::Store {
                src: self.gpr(0),
                addr,
                width: mem_width,
            },
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![opcode],
                });
            }
        };
        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, kind)],
            prefix.cursor + addr_bytes,
        ))
    }

    /// Lift MOV r, imm (B8-BF)
    pub(crate) fn lift_mov_r_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();
        let op_size = prefix.op_size();
        let width = self.size_to_width(op_size);

        // In 64-bit mode with REX.W, we can have 64-bit immediate
        let (imm, imm_size): (i64, usize) = if prefix.rex_w() {
            if bytes.len() < 8 {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: bytes.len(),
                    need: 8,
                });
            }
            (
                i64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]),
                8,
            )
        } else {
            match op_size {
                2 => {
                    if bytes.len() < 2 {
                        return Err(LiftError::Incomplete {
                            addr: pc,
                            have: bytes.len(),
                            need: 2,
                        });
                    }
                    (i16::from_le_bytes([bytes[0], bytes[1]]) as i64, 2)
                }
                _ => {
                    if bytes.len() < 4 {
                        return Err(LiftError::Incomplete {
                            addr: pc,
                            have: bytes.len(),
                            need: 4,
                        });
                    }
                    (
                        i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
                        4,
                    )
                }
            }
        };

        let src = if prefix.rex_w() {
            SrcOperand::Imm64(imm)
        } else {
            SrcOperand::Imm(imm)
        };

        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::Mov {
                dst: self.gpr(reg),
                src,
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_size))
    }

    /// Lift MOV r8, imm8 (B0-B7)
    pub(crate) fn lift_mov_r8_imm8(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();

        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: 0,
                need: 1,
            });
        }

        let imm = bytes[0] as i8 as i64;

        let mut ops = Vec::new();
        if let Some(base) = self.high_byte_base(reg, prefix) {
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::Mov {
                    dst: value,
                    src: SrcOperand::Imm(imm),
                    width: OpWidth::W8,
                },
            ));
            self.merge_high_byte(base, value, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::Mov {
                    dst: self.gpr(reg),
                    src: SrcOperand::Imm(imm),
                    width: OpWidth::W8,
                },
            ));
        }

        Ok(LiftResult::fallthrough(ops, prefix.cursor + 1))
    }

    /// Lift PUSH r64 (50-57)
    pub(crate) fn lift_push_r64(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();
        let (width, mem_width, stack_bytes) = if prefix.stack_op_size() == 2 {
            (OpWidth::W16, MemWidth::B2, 2)
        } else {
            (OpWidth::W64, MemWidth::B8, 8)
        };
        let mut ops = Vec::new();

        // Intel defines PUSH RSP/PUSH SP to store the source value as it
        // existed before the stack-pointer decrement. Preserve that ordering
        // explicitly for the interpreter and for helper-backed JIT fusion.
        let source = if self.gpr(reg) == self.rsp() {
            let old_sp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: old_sp,
                    src: SrcOperand::Reg(self.rsp()),
                    width,
                },
            ));
            old_sp
        } else {
            self.gpr(reg)
        };

        // RSP -= operand width
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Sub {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(stack_bytes),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        // [RSP] = reg
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Store {
                src: source,
                addr: Address::Direct(self.rsp()),
                width: mem_width,
            },
        ));

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift POP r64 (58-5F)
    pub(crate) fn lift_pop_r64(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();
        let (width, mem_width, stack_bytes) = if prefix.stack_op_size() == 2 {
            (OpWidth::W16, MemWidth::B2, 2)
        } else {
            (OpWidth::W64, MemWidth::B8, 8)
        };
        let mut ops = Vec::new();

        if self.gpr(reg) == self.rsp() {
            let popped = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::Load {
                    dst: popped,
                    addr: Address::Direct(self.rsp()),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            if width == OpWidth::W16 {
                let incremented = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(1),
                    pc,
                    OpKind::Add {
                        dst: incremented,
                        src1: self.rsp(),
                        src2: SrcOperand::Imm(stack_bytes),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(2),
                    pc,
                    OpKind::Mov {
                        dst: self.rsp(),
                        src: SrcOperand::Reg(incremented),
                        width: OpWidth::W64,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.rsp(),
                    src: SrcOperand::Reg(popped),
                    width,
                },
            ));
            return Ok(LiftResult::fallthrough(ops, prefix.cursor));
        }

        // reg = [RSP]
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Load {
                dst: self.gpr(reg),
                addr: Address::Direct(self.rsp()),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        ));

        // RSP += operand width
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Add {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(stack_bytes),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift XCHG rax, r64 (90-97)
    pub(crate) fn lift_xchg_rax(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let reg = (opcode & 0x07) | prefix.rex_b();
        let width = self.size_to_width(prefix.op_size());
        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::Xchg {
                reg1: self.gpr(0),
                reg2: self.gpr(reg),
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift BSWAP r16/r32/r64 (0F C8+rd).
    ///
    /// Intel and AMD define the r16 result as undefined. The direct engine's
    /// deterministic profile preserves the complete destination register, so
    /// represent that form as an empty fallthrough rather than lowering a
    /// byte swap with observably different semantics.
    pub(crate) fn lift_bswap_opcode(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }

        if prefix.operand_size_override && !prefix.rex_w() {
            return Ok(LiftResult::fallthrough(Vec::new(), prefix.cursor));
        }

        let reg = (opcode & 0x07) | prefix.rex_b();
        let width = if prefix.rex_w() {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::Bswap {
                dst: self.gpr(reg),
                src: self.gpr(reg),
                width,
            },
        )];

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift PUSH imm8/imm32 (6A/68)
    pub(crate) fn lift_push_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let stack_bytes = prefix.stack_op_size();
        let imm_size = if opcode == 0x6A {
            1
        } else if stack_bytes == 2 {
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
            4 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
            _ => unreachable!(),
        };

        let hint = match imm_size {
            1 => X86OpHint::PushImm8,
            2 => X86OpHint::PushImm16,
            4 => X86OpHint::PushImm32,
            _ => unreachable!(),
        };
        let mem_width = if stack_bytes == 2 {
            MemWidth::B2
        } else {
            MemWidth::B8
        };

        let mut ops = Vec::new();
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::Sub {
                dst: self.rsp(),
                src1: self.rsp(),
                src2: SrcOperand::Imm(i64::from(stack_bytes)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        ops.push(SmirOp::with_hint(
            OpId(1),
            pc,
            OpKind::Store {
                src: VReg::Imm(imm),
                addr: Address::Direct(self.rsp()),
                width: mem_width,
            },
            hint,
        ));

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_size))
    }

    /// Lift LEAVE (C9)
    pub(crate) fn lift_leave(&self, prefix: &X86Prefix, pc: u64) -> Result<LiftResult, LiftError> {
        let ops = vec![SmirOp::new(OpId(0), pc, OpKind::Leave)];
        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift ENTER imm16, imm8 (C8) as one fault-precise implicit stack
    /// transaction, including all 32 architectural nesting levels.
    pub(crate) fn lift_enter(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0xC8],
            });
        }
        if bytes.len() < 3 {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: 3,
            });
        }

        let allocation_size = u16::from_le_bytes([bytes[0], bytes[1]]);
        let nesting = bytes[2] & 0x1F;
        let width = if prefix.operand_size_override && !prefix.rex_w() {
            OpWidth::W16
        } else {
            OpWidth::W64
        };
        let next_pc = pc + prefix.cursor as u64 + 3;
        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86Enter(X86EnterOp {
                    allocation_size,
                    nesting_level: nesting,
                    width,
                    requires_apx: prefix.rex2.is_some(),
                    next_pc,
                }),
            )],
            prefix.cursor + 3,
        ))
    }

    /// Lift LEA (8D)
    pub(crate) fn lift_lea(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;

        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        // LEA computes the effective ADDRESS — the segment OFFSET — and IGNORES
        // any segment override: `lea rax, fs:[rbx]` yields rbx, NOT fs_base+rbx
        // (LEA performs no memory access, so the segment base never applies).
        // Strip the override before lowering, else x86_addr_to_smir would emit a
        // SegmentRel that wrongly adds the FS/GS base.
        let mut lea_addr = modrm.addr.as_ref().unwrap().clone();
        lea_addr.segment = None;
        // LEA's addr32 result is an integer destination rather than a memory
        // address. Keep its explicit W32 computation in ordinary SMIR ops;
        // `Address::X86Addr32` is reserved for memory consumers whose helper
        // lowering reconstructs the address directly from architectural state.
        let (addr, mut ops) = if lea_addr.address_width == OpWidth::W32 {
            self.x86_addr32_to_smir(&lea_addr, next_pc, ctx, None)
        } else {
            self.x86_addr_to_smir(&lea_addr, next_pc, ctx)
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Lea {
                dst: self.gpr(modrm.reg),
                addr,
                width: prefix.op_width(),
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift XLAT/XLATB (D7): AL := byte ptr [segment:(E)BX + AL].
    pub(crate) fn lift_xlat(
        &self,
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0xD7],
            });
        }

        let address_width = if prefix.address_size_override {
            OpWidth::W32
        } else {
            OpWidth::W64
        };
        let al = ctx.alloc_vreg();
        let offset = ctx.alloc_vreg();
        let value = ctx.alloc_vreg();
        let mut ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::And {
                dst: al,
                src1: self.gpr(0),
                src2: SrcOperand::Imm(0xFF),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        )];
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::Add {
                dst: offset,
                src1: self.gpr(3),
                src2: SrcOperand::Reg(al),
                width: address_width,
                flags: FlagUpdate::None,
            },
        ));

        let addr = match prefix.segment_override {
            Some(0x64) => Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(offset),
                index: None,
                scale: 1,
                disp: 0,
            },
            Some(0x65) => Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                base: Some(offset),
                index: None,
                scale: 1,
                disp: 0,
            },
            _ => Address::Direct(offset),
        };
        ops.push(SmirOp::new(
            OpId(2),
            pc,
            OpKind::Load {
                dst: value,
                addr,
                width: MemWidth::B1,
                sign: SignExtend::Zero,
            },
        ));
        ops.push(SmirOp::new(
            OpId(3),
            pc,
            OpKind::Mov {
                dst: self.gpr(0),
                src: SrcOperand::Reg(value),
                width: OpWidth::W8,
            },
        ));

        Ok(LiftResult::fallthrough(ops, prefix.cursor))
    }

    /// Lift MOV r/m, r and MOV r, r/m (88-8B)
    pub(crate) fn lift_mov_rm_r(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let is_8bit = (opcode & 0x01) == 0;
        let dir_reg_rm = (opcode & 0x02) != 0; // true = reg is src, rm is dst

        let op_size = if is_8bit { 1 } else { prefix.op_size() };
        let width = self.size_to_width(op_size);
        let mem_width = self.size_to_memwidth(op_size);

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            if dir_reg_rm {
                // MOV r, rm - load from memory
                let dst = if is_8bit && self.high_byte_base(modrm.reg, prefix).is_some() {
                    ctx.alloc_vreg()
                } else {
                    self.gpr(modrm.reg)
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst,
                        addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
                if let Some(base) = is_8bit
                    .then(|| self.high_byte_base(modrm.reg, prefix))
                    .flatten()
                {
                    self.merge_high_byte(base, dst, pc, ctx, &mut ops);
                }
            } else {
                // MOV rm, r - store to memory
                let src = if is_8bit {
                    self.read_byte_reg(modrm.reg, prefix, pc, ctx, &mut ops)
                } else {
                    self.gpr(modrm.reg)
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src,
                        addr,
                        width: mem_width,
                    },
                ));
            }
        } else {
            // Register to register
            let (dst_code, src_code) = if dir_reg_rm {
                (modrm.reg, modrm.rm)
            } else {
                (modrm.rm, modrm.reg)
            };
            let src = if is_8bit {
                self.read_byte_reg(src_code, prefix, pc, ctx, &mut ops)
            } else {
                self.gpr(src_code)
            };
            let high_dst = is_8bit
                .then(|| self.high_byte_base(dst_code, prefix))
                .flatten();
            let dst = if high_dst.is_some() {
                ctx.alloc_vreg()
            } else {
                self.gpr(dst_code)
            };

            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(src),
                    width,
                },
            ));
            if let Some(base) = high_dst {
                self.merge_high_byte(base, dst, pc, ctx, &mut ops);
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift MOVSXD r64, r/m32 (63)
    pub(crate) fn lift_movsxd(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        if modrm.is_memory {
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
                    width: MemWidth::B4,
                    sign: SignExtend::Sign,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::SignExtend {
                    dst: self.gpr(modrm.reg),
                    src: tmp,
                    from_width: OpWidth::W32,
                    to_width: OpWidth::W64,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::SignExtend {
                    dst: self.gpr(modrm.reg),
                    src: self.gpr(modrm.rm),
                    from_width: OpWidth::W32,
                    to_width: OpWidth::W64,
                },
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
