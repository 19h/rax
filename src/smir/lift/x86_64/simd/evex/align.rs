//! EVEX VALIGND/Q vector-alignment lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_vector_align(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F3A
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.w {
            VecElementType::I64
        } else {
            VecElementType::I32
        };
        let cursor = prefix.bytes + 1;
        let modrm = decode_modrm(&bytes[cursor..], &prefix.modrm_prefix(cursor), pc)?;
        if prefix.b && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let low = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b {
                    elem.bytes()
                } else {
                    prefix.width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            if prefix.b {
                self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
            } else {
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: vector,
                        addr,
                        width: prefix.width,
                    },
                ));
                vector
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let high = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let lanes = prefix.width.lanes(elem) as u8;
        let shift = bytes[imm_offset] % lanes;
        let raw = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
        for lane in 0..lanes {
            let index = lane + shift;
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: if index < lanes { low } else { high },
                    lane: if index < lanes { index } else { index - lanes },
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: raw,
                    vec: raw,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
