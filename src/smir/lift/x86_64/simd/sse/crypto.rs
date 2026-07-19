//! crypto.rs

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
    pub(crate) fn lift_sse_gfni(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        affine: bool,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
            || (affine && !matches!(opcode, 0xCE | 0xCF))
            || (!affine && opcode != 0xCF)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if affine && bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let bytes_consumed = prefix.cursor + imm_offset + usize::from(affine);
        let next_pc = pc + bytes_consumed as u64;
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = if affine {
            self.append_gf2p8_affine_vector(
                dst,
                src2,
                VecWidth::V128,
                bytes[imm_offset],
                opcode == 0xCF,
                pc,
                ctx,
                &mut ops,
            )
        } else {
            self.append_gf2p8_mul_vector(dst, src2, VecWidth::V128, pc, ctx, &mut ops)
        };
        self.append_legacy_packed_result(dst, raw, VecElementType::I8, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }

    pub(crate) fn lift_sse_aes_round(
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
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        let (op, src2) = match opcode {
            0xDB => (X86AesOp::InvMixColumns, None),
            0xDC => (X86AesOp::Enc, Some(src)),
            0xDD => (X86AesOp::EncLast, Some(src)),
            0xDE => (X86AesOp::Dec, Some(src)),
            0xDF => (X86AesOp::DecLast, Some(src)),
            _ => unreachable!(),
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Aes {
                dst: raw,
                src1: if opcode == 0xDB { src } else { dst },
                src2,
                width: VecWidth::V128,
                op,
                imm: 0,
            },
        ));
        self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_aes_keygen(
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
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
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
            OpKind::X86Aes {
                dst: raw,
                src1: src,
                src2: None,
                width: VecWidth::V128,
                op: X86AesOp::KeygenAssist,
                imm: bytes[imm_offset],
            },
        ));
        self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }
}
