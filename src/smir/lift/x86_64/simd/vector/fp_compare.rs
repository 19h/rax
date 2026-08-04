//! Packed and scalar floating-point comparison lifting.

use crate::smir::lift::x86_64::*;

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap};
use crate::smir::ir::types::*;

impl X86_64Lifter {
    pub(crate) fn lift_vec_fp_compare(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let fp16 = prefix.map == X86VecMap::Map0F3A;
        let (elem, scalar) = match (fp16, prefix.pp) {
            (true, X86SsePrefix::None) => (VecElementType::F16, false),
            (true, X86SsePrefix::Rep) => (VecElementType::F16, true),
            (true, _) => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            (false, X86SsePrefix::None) => (VecElementType::F32, false),
            (false, X86SsePrefix::OpSize) => (VecElementType::F64, false),
            (false, X86SsePrefix::Rep) => (VecElementType::F32, true),
            (false, X86SsePrefix::Repne) => (VecElementType::F64, true),
        };
        if (fp16 && (prefix.encoding != VecEncodingKind::Evex || prefix.w))
            || (!scalar
                && prefix.l_bits == 3
                && !(prefix.encoding == VecEncodingKind::Evex && prefix.b))
            || (prefix.encoding == VecEncodingKind::Evex
                && scalar
                && !prefix.b
                && prefix.l_bits == 3)
            || (prefix.encoding == VecEncodingKind::Evex
                && ((elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let predicate = bytes[imm_offset];
        if predicate & !0x1F != 0
            || (prefix.encoding == VecEncodingKind::Evex && (modrm.reg >= 8 || prefix.reg_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..=imm_offset].to_vec(),
            });
        }
        let packed_sae =
            prefix.encoding == VecEncodingKind::Evex && !scalar && prefix.b && !modrm.is_memory;
        // Intel SDM Table 2-43: register-source SAE ignores L'L.
        if !scalar && prefix.l_bits == 3 && !packed_sae {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..=imm_offset].to_vec(),
            });
        }
        if prefix.encoding == VecEncodingKind::Evex && scalar && prefix.b && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..=imm_offset].to_vec(),
            });
        }
        let width = if packed_sae {
            VecWidth::V512
        } else if scalar {
            VecWidth::V128
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let next_pc = pc + imm_offset as u64 + 1;
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast =
            prefix.encoding == VecEncodingKind::Evex && !scalar && prefix.b && modrm.is_memory;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let scale = if scalar || broadcast {
                elem.bytes()
            } else {
                width.bytes()
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
            if scalar {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: value,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                if let Some(mask_reg) = mask {
                    let active = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: active,
                            src1: mask_reg,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: value,
                            cond: active,
                            addr,
                            width: match elem {
                                VecElementType::F16 => MemWidth::B2,
                                VecElementType::F32 => MemWidth::B4,
                                _ => MemWidth::B8,
                            },
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: value,
                            addr,
                            width: match elem {
                                VecElementType::F16 => MemWidth::B2,
                                VecElementType::F32 => MemWidth::B4,
                                _ => MemWidth::B8,
                            },
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar: value,
                        elem,
                        lanes: 1,
                    },
                ));
                loaded
            } else if let Some(mask_reg) = mask {
                if broadcast {
                    self.append_masked_broadcast_memory_source(
                        addr, elem, width, mask_reg, pc, ctx, &mut ops,
                    )
                } else {
                    self.append_evex_masked_vector_source(
                        addr, elem, width, false, mask_reg, pc, ctx, &mut ops,
                    )
                }
            } else if broadcast {
                let value = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: value,
                        addr,
                        width: match elem {
                            VecElementType::F16 => MemWidth::B2,
                            VecElementType::F32 => MemWidth::B4,
                            _ => MemWidth::B8,
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar: value,
                        elem,
                        lanes,
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
                        width,
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
                width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            width,
        );
        let mask_destination = prefix.encoding == VecEncodingKind::Evex;
        let dst = if mask_destination {
            VReg::Arch(ArchReg::X86(X86Reg::K(modrm.reg)))
        } else {
            self.vec_reg(modrm.reg, width)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86VectorFpCompare {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                predicate,
                scalar,
                mask_destination,
                zero_upper: !mask_destination,
                suppress_exceptions: prefix.encoding == VecEncodingKind::Evex
                    && (scalar && prefix.b || packed_sae),
            },
            self.vec_hint(prefix, 0xC2),
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
