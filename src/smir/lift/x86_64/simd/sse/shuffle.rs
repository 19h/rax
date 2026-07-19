//! shuffle.rs

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

impl X86_64Lifter {
    pub(crate) fn lift_sse_duplicate_move(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let (elem, high) = match (opcode, prefix.operand_size_override, prefix.rep_prefix) {
            (0x12, false, Some(0xF3)) => (VecElementType::F32, false),
            (0x16, false, Some(0xF3)) => (VecElementType::F32, true),
            (0x12, false, Some(0xF2)) => (VecElementType::F64, false),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory && elem == VecElementType::F64 {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            let vector = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: scalar,
                    addr,
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem,
                    lanes: 2,
                },
            ));
            vector
        } else if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_duplicate_shuffle(raw, src, VecWidth::V128, elem, high, pc, ctx, &mut ops);
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_two_source_shuffle_imm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match (prefix.operand_size_override, prefix.rep_prefix) {
            (false, None) => VecElementType::F32,
            (true, None) => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_two_source_shuffle_imm(
            raw,
            dst,
            src2,
            VecWidth::V128,
            elem,
            bytes[imm_offset],
            pc,
            ctx,
            &mut ops,
        );
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }

    pub(crate) fn lift_sse_packed_shuffle_imm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let prefix_kind = match (prefix.operand_size_override, prefix.rep_prefix) {
            (true, None) => X86SsePrefix::OpSize,
            (false, Some(0xF3)) => X86SsePrefix::Rep,
            (false, Some(0xF2)) => X86SsePrefix::Repne,
            (false, None) => X86SsePrefix::None,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let mmx = prefix_kind == X86SsePrefix::None;
        if prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let (elem, high_words) = match prefix_kind {
            X86SsePrefix::None => (VecElementType::I16, None),
            X86SsePrefix::OpSize => (VecElementType::I32, None),
            X86SsePrefix::Rep => (VecElementType::I16, Some(true)),
            X86SsePrefix::Repne => (VecElementType::I16, Some(false)),
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86PackedShuffleImm {
                    dst,
                    src,
                    width: VecWidth::V64,
                    elem,
                    imm: bytes[imm_offset],
                    high_words,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x70,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else {
            let raw = ctx.alloc_vreg();
            self.append_packed_shuffle_imm(
                raw,
                src,
                VecWidth::V128,
                elem,
                bytes[imm_offset],
                high_words,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }

    /// Lift legacy SSSE3 PSHUFB with MMX or XMM operands.
    pub(crate) fn lift_sse_pshufb(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let control = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        let raw = if !mmx && modrm.is_memory {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let shuffle = OpKind::VByteShuffle {
            dst: raw,
            src: dst,
            control,
            lanes: if mmx { 8 } else { 16 },
            block_lanes: if mmx { 8 } else { 16 },
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                shuffle,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x00,
                },
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                shuffle,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x00,
                },
            ));
        }
        if mmx {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else if modrm.is_memory {
            self.append_legacy_packed_result(dst, raw, VecElementType::I8, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_variable_blend(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x10 => VecElementType::I8,
            0x14 => VecElementType::I32,
            0x15 => VecElementType::I64,
            _ => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CheckAlignment {
                    addr: addr.clone(),
                    alignment: 16,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Aligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_variable_blend(
            raw,
            dst,
            src2,
            self.xmm(0),
            elem,
            VecWidth::V128,
            pc,
            ctx,
            &mut ops,
        );
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_palignr(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if bytes.len() <= modrm.bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + modrm.bytes_consumed,
                need: prefix.cursor + modrm.bytes_consumed + 1,
            });
        }
        let imm = bytes[modrm.bytes_consumed];
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let low = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86PackedAlignRight {
                    dst,
                    high: dst,
                    low,
                    width: VecWidth::V64,
                    amount: imm,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x0F,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else {
            let raw = ctx.alloc_vreg();
            self.append_align_right(raw, dst, low, VecWidth::V128, imm, pc, ctx, &mut ops);
            self.append_legacy_packed_result(dst, raw, VecElementType::I8, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }

    pub(crate) fn lift_sse_immediate_blend(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, repeat_128) = match opcode {
            0x0C => (VecElementType::I32, false),
            0x0D => (VecElementType::I64, false),
            0x0E => (VecElementType::I16, true),
            _ => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + imm_offset,
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CheckAlignment {
                    addr: addr.clone(),
                    alignment: 16,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Aligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_immediate_blend(
            raw,
            dst,
            src2,
            elem,
            VecWidth::V128,
            imm,
            repeat_128,
            pc,
            ctx,
            &mut ops,
        );
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }

    pub(crate) fn lift_sse_extract_0f3a(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, lane_mask, mem_width, op_width) = match opcode {
            0x14 => (VecElementType::I8, 0x0F, MemWidth::B1, OpWidth::W32),
            0x15 => (VecElementType::I16, 0x07, MemWidth::B2, OpWidth::W32),
            0x16 if prefix.rex_w() => (VecElementType::I64, 0x01, MemWidth::B8, OpWidth::W64),
            0x16 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            0x17 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            _ => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + imm_offset,
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let lane = bytes[imm_offset] & lane_mask;
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let addr = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            Some(addr)
        } else {
            None
        };
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: scalar,
                vec: self.xmm(modrm.reg),
                lane,
                elem,
                sign: SignExtend::Zero,
            },
        ));
        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: scalar,
                    addr,
                    width: mem_width,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.rm),
                    src: SrcOperand::Reg(scalar),
                    width: op_width,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }

    pub(crate) fn lift_sse_insert_0f3a(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + imm_offset,
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let dst = self.xmm(modrm.reg);

        if opcode == 0x21 {
            let inserted = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: self.xmm(modrm.rm),
                        lane: (imm >> 6) & 0x03,
                        elem: VecElementType::I32,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            };
            let raw = ctx.alloc_vreg();
            self.append_insertps(
                raw,
                dst,
                inserted,
                (imm >> 4) & 0x03,
                imm & 0x0F,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, VecElementType::I32, pc, ctx, &mut ops);
        } else {
            let (elem, lane_mask, mem_width) = match opcode {
                0x20 => (VecElementType::I8, 0x0F, MemWidth::B1),
                0x22 if prefix.rex_w() => (VecElementType::I64, 0x01, MemWidth::B8),
                0x22 => (VecElementType::I32, 0x03, MemWidth::B4),
                _ => unreachable!(),
            };
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                self.gpr(modrm.rm)
            };
            let raw = ctx.alloc_vreg();
            self.append_insert_scalar_lane(
                raw,
                dst,
                scalar,
                elem,
                imm & lane_mask,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        }

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }
}
