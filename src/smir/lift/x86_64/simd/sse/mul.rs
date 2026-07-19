//! mul.rs

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
    pub(crate) fn lift_sse_pmaddubsw(
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
        let src2 = if modrm.is_memory {
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
                OpKind::VDotProduct {
                    dst,
                    acc: VReg::Imm(0),
                    src1: dst,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V64,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x04,
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
        } else if modrm.is_memory {
            // Keep the computation detached from the architectural destination:
            // the aligned source read must fault before PMADDUBSW changes XMM1,
            // and the generic vector write would otherwise clear its legacy
            // YMM/ZMM backing state above bit 127.
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VDotProduct {
                    dst: raw,
                    acc: VReg::Imm(0),
                    src1: dst,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V128,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        } else {
            // A zero immediate is also the canonical all-zero vector for the
            // interpreter. Keeping the register form atomic lets strict native
            // admission reproduce the original SSSE3 instruction exactly.
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VDotProduct {
                    dst,
                    acc: VReg::Imm(0),
                    src1: dst,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V128,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x04,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_pmulhrsw(
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
        let src2 = if modrm.is_memory {
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
                OpKind::VMulShiftSat {
                    dst,
                    src1: dst,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes: 4,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x0B,
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
        } else if modrm.is_memory {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMulShiftSat {
                    dst: raw,
                    src1: dst,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes: 8,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMulShiftSat {
                    dst,
                    src1: dst,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes: 8,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_pmuldq(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mmx = !signed && !prefix.operand_size_override;
        if (!mmx && !prefix.operand_size_override)
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
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
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
            self.append_pmuldq(dst, dst, src2, VecWidth::V64, false, pc, ctx, &mut ops);
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
            self.append_pmuldq(raw, dst, src2, VecWidth::V128, signed, pc, ctx, &mut ops);
            self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift SSE4.1 PMULLD (66 0F 38 40)
    pub(crate) fn lift_sse_pmulld(
        &self,
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
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let dst = self.xmm(modrm.reg);
        let hint = X86OpHint::SseOp {
            prefix: X86SsePrefix::OpSize,
            opcode: 0x40,
        };
        if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: tmp,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMul {
                    dst,
                    src1: dst,
                    src2: tmp,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                hint,
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(0),
                pc,
                OpKind::VMul {
                    dst,
                    src1: dst,
                    src2: self.xmm(modrm.rm),
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                hint,
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_pmullw(
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
        let src2 = if modrm.is_memory {
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
        let kind = OpKind::VMul {
            dst,
            src1: dst,
            src2,
            elem: VecElementType::I16,
            lanes: if mmx { 4 } else { 8 },
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xD5,
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
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xD5,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_pmul_high_word(
        &self,
        opcode: u8,
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
        let src2 = if modrm.is_memory {
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
                Self::pmul_high_word_kind(dst, dst, src2, VecWidth::V64, opcode == 0xE5),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
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
        } else if modrm.is_memory {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(raw, dst, src2, VecWidth::V128, opcode == 0xE5),
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(dst, dst, src2, VecWidth::V128, opcode == 0xE5),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_pmaddwd(
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
        let src2 = if modrm.is_memory {
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
                Self::pmaddwd_kind(dst, dst, src2, VecWidth::V64),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xF5,
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
        } else if modrm.is_memory {
            // Keep the computation detached from the architectural destination
            // until the aligned source load has completed successfully.
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(raw, dst, src2, VecWidth::V128),
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I32, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(dst, dst, src2, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF5,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_pclmulqdq(
        &self,
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
        self.append_pclmulqdq(raw, dst, src2, VecWidth::V128, imm, pc, ctx, &mut ops);
        self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }

    pub(crate) fn lift_sse_dot_product(
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
        let elem = if opcode == 0x40 {
            VecElementType::F32
        } else {
            VecElementType::F64
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
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86DotProduct {
                dst: raw,
                src1: dst,
                src2,
                elem,
                width: VecWidth::V128,
                imm: bytes[imm_offset],
            },
        ));
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }
}
