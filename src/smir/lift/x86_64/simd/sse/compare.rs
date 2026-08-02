//! compare.rs

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86PackedStringKind, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind,
    X86VecAlign, X86VecMap, X86X87ArithmeticDestination, X86X87ArithmeticSource,
    X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind, X86X87EnvWidth,
    X86X87FloatWidth, X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};

impl X86_64Lifter {
    /// Lift the four exact legacy SSE4.2 packed-string comparison forms.
    pub(crate) fn lift_sse_pcmpxstrx(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let kind = match opcode {
            0x60 => X86PackedStringKind::ExplicitMask,
            0x61 => X86PackedStringKind::ExplicitIndex,
            0x62 => X86PackedStringKind::ImplicitMask,
            0x63 => X86PackedStringKind::ImplicitIndex,
            _ => unreachable!(),
        };
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
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let bytes_consumed = prefix.cursor + imm_offset + 1;
        let next_pc = pc + bytes_consumed as u64;
        let length_width = if kind.is_explicit() && prefix.rex_w() {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        Ok(self.lift_pcmpxstrx_decoded(
            kind,
            modrm,
            imm,
            length_width,
            false,
            bytes_consumed,
            next_pc,
            pc,
            ctx,
        ))
    }

    /// Lift the four AVX VEX.128 packed-string comparison forms. Intel SDM
    /// Vol. 2B defines map 0F3A opcodes 60H through 63H with mandatory 66H,
    /// VEX.L=0, and reserved VEX.vvvv=1111b. VEX.W retains REX.W's explicit-
    /// length selection; it is ignored by the implicit-length forms.
    pub(crate) fn lift_vex_pcmpxstrx(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let kind = match opcode {
            0x60 => X86PackedStringKind::ExplicitMask,
            0x61 => X86PackedStringKind::ExplicitIndex,
            0x62 => X86PackedStringKind::ImplicitMask,
            0x63 => X86PackedStringKind::ImplicitIndex,
            _ => unreachable!(),
        };
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.width != VecWidth::V128
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&imm) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };
        let bytes_consumed = imm_offset + 1;
        let next_pc = pc + bytes_consumed as u64;
        let length_width = if kind.is_explicit() && prefix.w {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        Ok(self.lift_pcmpxstrx_decoded(
            kind,
            modrm,
            imm,
            length_width,
            kind.returns_mask(),
            bytes_consumed,
            next_pc,
            pc,
            ctx,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn lift_pcmpxstrx_decoded(
        &self,
        kind: X86PackedStringKind,
        modrm: ModRm,
        imm: u8,
        length_width: OpWidth,
        zero_upper: bool,
        bytes_consumed: usize,
        next_pc: u64,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> LiftResult {
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        let (len1, len2) = if kind.is_explicit() {
            (Some(self.gpr(0)), Some(self.gpr(2)))
        } else {
            (None, None)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedStringCompare {
                dst: if kind.returns_mask() {
                    self.xmm(0)
                } else {
                    self.gpr(1)
                },
                src1: self.xmm(modrm.reg),
                src2,
                len1,
                len2,
                length_width,
                kind,
                imm,
                zero_upper,
            },
        ));

        LiftResult::fallthrough(ops, bytes_consumed)
    }

    /// Lift MMX/SSE2/SSE4.1 packed integer equality and signed greater-than
    /// comparisons.
    pub(crate) fn lift_sse_integer_compare(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mmx_opcode = matches!(opcode, 0x64 | 0x65 | 0x66 | 0x74 | 0x75 | 0x76);
        let mmx = !prefix.operand_size_override && mmx_opcode;
        if (!prefix.operand_size_override && !mmx)
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let (elem, cond) = match opcode {
            0x64 => (VecElementType::I8, VecCmpCond::Gt),
            0x65 => (VecElementType::I16, VecCmpCond::Gt),
            0x66 => (VecElementType::I32, VecCmpCond::Gt),
            0x74 => (VecElementType::I8, VecCmpCond::Eq),
            0x75 => (VecElementType::I16, VecCmpCond::Eq),
            0x76 => (VecElementType::I32, VecCmpCond::Eq),
            0x29 => (VecElementType::I64, VecCmpCond::Eq),
            0x37 => (VecElementType::I64, VecCmpCond::Gt),
            _ => unreachable!(),
        };
        let width = if mmx { VecWidth::V64 } else { VecWidth::V128 };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width,
                },
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        if mmx {
            // A faulting memory source must not enter MMX state.
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        }
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        let result = if modrm.is_memory && !mmx {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: result,
                src1: dst,
                src2,
                cond,
                elem,
                lanes: width.lanes(elem) as u8,
            },
            X86OpHint::SseOp {
                prefix: if mmx {
                    X86SsePrefix::None
                } else {
                    X86SsePrefix::OpSize
                },
                opcode,
            },
        ));
        if modrm.is_memory && !mmx {
            self.append_legacy_packed_result(dst, result, elem, pc, ctx, &mut ops);
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_ptest(
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
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let second = if modrm.is_memory {
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
        self.append_ptest_flags(
            self.xmm(modrm.reg),
            second,
            VecWidth::V128,
            None,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
