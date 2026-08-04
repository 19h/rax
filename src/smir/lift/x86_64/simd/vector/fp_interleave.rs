//! Floating-point unpack/interleave lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_vec_fp_unpack(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match prefix.pp {
            X86SsePrefix::None => VecElementType::F32,
            X86SsePrefix::OpSize => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex
                && ((elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: elem == VecElementType::F64,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let scale = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    scale,
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
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
                        dst: loaded,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Vex {
            self.append_vector_interleave(
                dst,
                src1,
                src2,
                elem,
                prefix.width,
                opcode == 0x15,
                self.vec_hint(prefix, opcode),
                pc,
                &mut ops,
            );
        } else {
            let raw = if prefix.aaa == 0 {
                dst
            } else {
                ctx.alloc_vreg()
            };
            self.append_vector_interleave(
                raw,
                src1,
                src2,
                elem,
                prefix.width,
                opcode == 0x15,
                self.vec_hint(prefix, opcode),
                pc,
                &mut ops,
            );
            if prefix.aaa != 0 {
                self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
            }
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
