//! AVX-512-FP16 packed arithmetic lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_fp16_arithmetic(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp == X86SsePrefix::Rep {
            return self.lift_evex_fp16_scalar_arithmetic(prefix, opcode, bytes, pc, ctx);
        }
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.pp != X86SsePrefix::None
            || prefix.w
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
        let embedded_rounding = prefix.b && !modrm.is_memory;
        if !embedded_rounding && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let width = if embedded_rounding {
            VecWidth::V512
        } else {
            prefix.width
        };
        let round = if embedded_rounding {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        let broadcast = prefix.b && modrm.is_memory;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let src1 = self.vec_reg(prefix.vvvv + if prefix.v_high { 16 } else { 0 }, width);
        let src2 = if modrm.is_memory {
            let elem = VecElementType::F16;
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    elem.bytes()
                } else {
                    width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
            match (mask, broadcast) {
                (Some(mask), true) => self.append_masked_broadcast_memory_source(
                    addr, elem, width, mask, pc, ctx, &mut ops,
                ),
                (Some(mask), false) => self.append_evex_masked_vector_source(
                    addr, elem, width, false, mask, pc, ctx, &mut ops,
                ),
                (None, true) => {
                    self.append_broadcast_memory_source(addr, elem, width, pc, ctx, &mut ops)
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
        let op = match opcode {
            0x58 => Avx10FP16Op::Add,
            0x59 => Avx10FP16Op::Mul,
            0x5C => Avx10FP16Op::Sub,
            0x5D => Avx10FP16Op::Min,
            0x5E => Avx10FP16Op::Div,
            0x5F => Avx10FP16Op::Max,
            _ => unreachable!("MAP5 FP16 dispatch filtered opcode"),
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask: (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                op,
                round,
                width,
                lanes: width.lanes(VecElementType::F16) as u8,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
