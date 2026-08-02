//! AMD four-operand FMA4 lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86FmaOp, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::{
    VecEncodingKind, VecPrefix, X86_64Lifter, X86Prefix, decode_modrm,
};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn lift_vex_fma4(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F3A
            || prefix.pp != X86SsePrefix::OpSize
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let (elem, kind, scalar) = match opcode {
            0x5C => (VecElementType::F32, X86FmaKind::AddSub, false),
            0x5D => (VecElementType::F64, X86FmaKind::AddSub, false),
            0x5E => (VecElementType::F32, X86FmaKind::SubAdd, false),
            0x5F => (VecElementType::F64, X86FmaKind::SubAdd, false),
            0x68 => (VecElementType::F32, X86FmaKind::Add, false),
            0x69 => (VecElementType::F64, X86FmaKind::Add, false),
            0x6A => (VecElementType::F32, X86FmaKind::Add, true),
            0x6B => (VecElementType::F64, X86FmaKind::Add, true),
            0x6C => (VecElementType::F32, X86FmaKind::Sub, false),
            0x6D => (VecElementType::F64, X86FmaKind::Sub, false),
            0x6E => (VecElementType::F32, X86FmaKind::Sub, true),
            0x6F => (VecElementType::F64, X86FmaKind::Sub, true),
            0x78 => (VecElementType::F32, X86FmaKind::NegativeMultiplyAdd, false),
            0x79 => (VecElementType::F64, X86FmaKind::NegativeMultiplyAdd, false),
            0x7A => (VecElementType::F32, X86FmaKind::NegativeMultiplyAdd, true),
            0x7B => (VecElementType::F64, X86FmaKind::NegativeMultiplyAdd, true),
            0x7C => (VecElementType::F32, X86FmaKind::NegativeMultiplySub, false),
            0x7D => (VecElementType::F64, X86FmaKind::NegativeMultiplySub, false),
            0x7E => (VecElementType::F32, X86FmaKind::NegativeMultiplySub, true),
            0x7F => (VecElementType::F64, X86FmaKind::NegativeMultiplySub, true),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };

        let cursor = prefix.bytes + 1;
        let modrm_prefix = prefix.modrm_prefix(cursor);
        let modrm = decode_modrm(&bytes[cursor.min(bytes.len())..], &modrm_prefix, pc).map_err(
            |error| match error {
                LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                    addr,
                    have: cursor + have,
                    need: cursor + need,
                },
                error => error,
            },
        )?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }

        let operation_width = if scalar { VecWidth::V128 } else { prefix.width };
        let lanes = if scalar {
            1
        } else {
            operation_width.lanes(elem) as u8
        };
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let dst = self.vec_reg(modrm.reg, operation_width);
        let vex_source = self.vec_reg(prefix.vvvv, operation_width);
        let is4_source = self.vec_reg(bytes[imm_offset] >> 4, operation_width);
        let rm_source = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            if scalar {
                let scalar_value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar_value,
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
                        scalar: scalar_value,
                        elem,
                        lanes: 1,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: operation_width,
                    },
                ));
            }
            loaded
        } else {
            self.vec_reg(modrm.rm, operation_width)
        };

        // AMD APM Volume 4: VEX.W swaps only the r/m and /is4 operands.
        let (src2, src3) = if prefix.w {
            (is4_source, rm_source)
        } else {
            (rm_source, is4_source)
        };
        let raw = ctx.alloc_vreg();
        let operation_prefix = VecPrefix {
            width: operation_width,
            ..prefix
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Fma(X86FmaOp {
                dst: raw,
                src1: vex_source,
                src2,
                src3,
                mask: None,
                elem,
                kind,
                order: X86FmaOrder::Order123,
                round: FpRoundMode::Dynamic,
                lanes,
            }),
            self.vec_hint(operation_prefix, opcode),
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: raw,
                width: operation_width,
            },
        ));

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
