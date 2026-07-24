//! AVX10.2 MAP5 saturating floating-point-to-integer conversions.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_saturating_fp_to_int(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let (fp_elem, int_elem, signed) = match (opcode, prefix.pp, prefix.w) {
            (0x68, X86SsePrefix::OpSize, false) => (VecElementType::F32, VecElementType::I8, true),
            (0x6A, X86SsePrefix::OpSize, false) => (VecElementType::F32, VecElementType::I8, false),
            (0x6D, X86SsePrefix::OpSize, true) => (VecElementType::F64, VecElementType::I64, true),
            (0x6C, X86SsePrefix::OpSize, true) => (VecElementType::F64, VecElementType::I64, false),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let suppress_exceptions = prefix.b && !modrm.is_memory;
        if suppress_exceptions && prefix.l_bits != 0 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let width = if suppress_exceptions {
            VecWidth::V512
        } else {
            prefix.width
        };
        let broadcast = prefix.b && modrm.is_memory;
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    fp_elem.bytes()
                } else {
                    width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            match (mask, broadcast) {
                (Some(mask), true) => self.append_masked_broadcast_memory_source(
                    addr, fp_elem, width, mask, pc, ctx, &mut ops,
                ),
                (Some(mask), false) => self.append_evex_masked_vector_source(
                    addr, fp_elem, width, false, mask, pc, ctx, &mut ops,
                ),
                (None, true) => {
                    self.append_broadcast_memory_source(addr, fp_elem, width, pc, ctx, &mut ops)
                }
                (None, false) => {
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
                }
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, width)
        };
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                width,
                signed,
                zeroing: prefix.zeroing,
                suppress_exceptions,
            },
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
