//! EVEX reciprocal and reciprocal-square-root approximation lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_approx14(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode & 1 != 0;
        let rsqrt = opcode >= 0x4E;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0x4C..=0x4F)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        // EVEX.b selects scalar memory broadcast for packed forms. Register
        // sources and all scalar forms reserve EVEX.b; packed L'L=3 is also
        // reserved, while scalar L'L is ignored.
        if (prefix.b && (!modrm.is_memory || scalar)) || (!scalar && prefix.l_bits == 3) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        let kind = if rsqrt {
            OpKind::X86Rsqrt14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        } else {
            OpKind::X86Recip14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_fp16_approx(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode & 1 != 0;
        let rsqrt = opcode >= 0x4E;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map6
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || !matches!(opcode, 0x4C..=0x4F)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
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
        // Packed memory forms use EVEX.b for m16 broadcast. Register sources
        // and both scalar forms reserve EVEX.b. Packed L'L=3 is reserved;
        // scalar L'L is ignored by the encoding.
        if (prefix.b && (!modrm.is_memory || scalar)) || (!scalar && prefix.l_bits == 3) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let lanes = if scalar {
            1
        } else {
            width.lanes(VecElementType::F16) as u8
        };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix,
            &modrm,
            next_pc,
            VecElementType::F16,
            width,
            scalar,
            mask,
            pc,
            ctx,
        );
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        let kind = if rsqrt {
            OpKind::X86RsqrtFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        } else {
            OpKind::X86RecipFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: false,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_approx28(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode & 1 != 0;
        let rsqrt = opcode >= 0xCC;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0xCA..=0xCD)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        // EVEX.b selects SAE for register sources and broadcast for packed
        // memory. The common EVEX rules reserve it for scalar memory sources.
        // Scalar L'L is otherwise ignored; non-SAE packed forms are 512-bit.
        let embedded_sae = prefix.b && !modrm.is_memory;
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_sae && prefix.l_bits != 2)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else {
            VecWidth::V512
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        let kind = if rsqrt {
            OpKind::X86Rsqrt28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            }
        } else {
            OpKind::X86Recip28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            }
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
