//! AVX10.2 MAP5 saturating floating-point-to-integer conversions.

use crate::smir::ir::ops::{
    OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap, x86_sat_fp_to_int_widths,
};
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
        let (fp_elem, int_elem, signed, truncate) = match (opcode, prefix.pp, prefix.w) {
            (0x68, X86SsePrefix::OpSize, false) => {
                (VecElementType::F32, VecElementType::I8, true, true)
            }
            (0x69, X86SsePrefix::OpSize, false) => {
                (VecElementType::F32, VecElementType::I8, true, false)
            }
            (0x6A, X86SsePrefix::OpSize, false) => {
                (VecElementType::F32, VecElementType::I8, false, true)
            }
            (0x6B, X86SsePrefix::OpSize, false) => {
                (VecElementType::F32, VecElementType::I8, false, false)
            }
            (0x6D, X86SsePrefix::None, false) => {
                (VecElementType::F32, VecElementType::I32, true, true)
            }
            (0x6C, X86SsePrefix::None, false) => {
                (VecElementType::F32, VecElementType::I32, false, true)
            }
            (0x6D, X86SsePrefix::OpSize, false) => {
                (VecElementType::F32, VecElementType::I64, true, true)
            }
            (0x6C, X86SsePrefix::OpSize, false) => {
                (VecElementType::F32, VecElementType::I64, false, true)
            }
            (0x6D, X86SsePrefix::None, true) => {
                (VecElementType::F64, VecElementType::I32, true, true)
            }
            (0x6C, X86SsePrefix::None, true) => {
                (VecElementType::F64, VecElementType::I32, false, true)
            }
            (0x6D, X86SsePrefix::OpSize, true) => {
                (VecElementType::F64, VecElementType::I64, true, true)
            }
            (0x6C, X86SsePrefix::OpSize, true) => {
                (VecElementType::F64, VecElementType::I64, false, true)
            }
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
        let embedded_control = prefix.b && !modrm.is_memory;
        if (!embedded_control && prefix.l_bits == 3)
            || (embedded_control && truncate && prefix.l_bits != 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let encoded_width = if embedded_control {
            VecWidth::V512
        } else {
            prefix.width
        };
        let (src_width, width) = match (fp_elem, int_elem, encoded_width) {
            (VecElementType::F64, VecElementType::I32, VecWidth::V128) => {
                (VecWidth::V128, VecWidth::V64)
            }
            (VecElementType::F64, VecElementType::I32, VecWidth::V256) => {
                (VecWidth::V256, VecWidth::V128)
            }
            (VecElementType::F64, VecElementType::I32, VecWidth::V512) => {
                (VecWidth::V512, VecWidth::V256)
            }
            (VecElementType::F32, VecElementType::I64, VecWidth::V128) => {
                (VecWidth::V64, VecWidth::V128)
            }
            (VecElementType::F32, VecElementType::I64, VecWidth::V256) => {
                (VecWidth::V128, VecWidth::V256)
            }
            (VecElementType::F32, VecElementType::I64, VecWidth::V512) => {
                (VecWidth::V256, VecWidth::V512)
            }
            (_, _, width) => (width, width),
        };
        debug_assert_eq!(
            x86_sat_fp_to_int_widths(fp_elem, int_elem, width, truncate),
            Some((src_width, encoded_width))
        );
        let round = if truncate {
            FpRoundMode::RoundTowardZero
        } else if embedded_control {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                3 => FpRoundMode::RoundTowardZero,
                _ => unreachable!("EVEX L'L is two bits"),
            }
        } else {
            FpRoundMode::Dynamic
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
                    src_width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            match (mask, broadcast) {
                (Some(mask), true) => self.append_masked_broadcast_memory_source(
                    addr, fp_elem, src_width, mask, pc, ctx, &mut ops,
                ),
                (Some(mask), false) => self.append_evex_masked_vector_source(
                    addr, fp_elem, src_width, false, mask, pc, ctx, &mut ops,
                ),
                (None, true) => {
                    self.append_broadcast_memory_source(addr, fp_elem, src_width, pc, ctx, &mut ops)
                }
                (None, false) => {
                    let loaded = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: loaded,
                            addr,
                            width: src_width,
                        },
                    ));
                    loaded
                }
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, src_width)
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
                truncate,
                round,
                zeroing: prefix.zeroing,
                suppress_exceptions: embedded_control,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: encoded_width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
