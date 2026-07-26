//! Scalar FP32/FP64 comparisons that write x86 status flags.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_vec_fp_flag_compare(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        debug_assert!(matches!(opcode, 0x2E | 0x2F));
        let cursor = prefix.bytes + 1;
        let after_opcode = &bytes[cursor..];
        let prefix_modrm = X86Prefix {
            rex: prefix.rex,
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            cursor,
            ..X86Prefix::default()
        };

        if prefix.vvvv != 0
            || prefix.v_high
            || matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
            || (prefix.encoding == VecEncodingKind::Evex && (prefix.aaa != 0 || prefix.zeroing))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.pp == X86SsePrefix::OpSize {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        if prefix.encoding == VecEncodingKind::Evex
            && ((elem == VecElementType::F32 && prefix.w)
                || (elem == VecElementType::F64 && !prefix.w))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let modrm = decode_modrm(after_opcode, &prefix_modrm, pc)?;
        if prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let src1 = self.xmm(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
        );
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                elem,
                ctx,
            );
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            let vector = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: scalar,
                    addr,
                    width: if elem == VecElementType::F32 {
                        MemWidth::B4
                    } else {
                        MemWidth::B8
                    },
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
                    lanes: 1,
                },
            ));
            vector
        } else {
            self.xmm(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
            )
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling: opcode == 0x2F,
                suppress_exceptions: prefix.encoding == VecEncodingKind::Evex && prefix.b,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
