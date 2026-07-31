//! EVEX signed and unsigned saturating integer packs.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_integer_pack(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let src_elem = match opcode {
            0x63 | 0x67 => VecElementType::I16,
            0x6B | 0x2B => VecElementType::I32,
            _ => unreachable!(),
        };
        let dst_elem = if src_elem == VecElementType::I16 {
            VecElementType::I8
        } else {
            VecElementType::I16
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
            || (src_elem == VecElementType::I32 && prefix.w)
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
        let broadcast = prefix.b && modrm.is_memory && src_elem == VecElementType::I32;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let src_lanes = prefix.width.lanes(src_elem) as u8;
        let block_lanes = (16 / src_elem.bytes()) as u8;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let scale = if broadcast {
                src_elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                scale,
                ctx,
            );
            ops.extend(pre_ops);
            if broadcast {
                self.append_broadcast_memory_source(addr, src_elem, prefix.width, pc, ctx, &mut ops)
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = if prefix.aaa == 0 {
            dst
        } else {
            ctx.alloc_vreg()
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPackSat {
                dst: raw,
                src1: src2,
                src2: self.vec_reg(
                    prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                    prefix.width,
                ),
                src_elem,
                to_unsigned: matches!(opcode, 0x67 | 0x2B),
                src_lanes,
                block_lanes,
            },
            self.vec_hint(prefix, opcode),
        ));
        if prefix.aaa != 0 {
            self.append_evex_vector_mask_result(prefix, dst, raw, dst_elem, pc, ctx, &mut ops);
        }

        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed),
        ))
    }
}
