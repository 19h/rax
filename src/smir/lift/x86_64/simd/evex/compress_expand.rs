//! EVEX VCOMPRESS*/VEXPAND* register and Type-E4 memory lifting.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_compress_expand(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let compress = matches!(opcode, 0x63 | 0x8A | 0x8B);
        let elem = match opcode {
            0x62 | 0x63 => {
                if prefix.w {
                    VecElementType::I16
                } else {
                    VecElementType::I8
                }
            }
            0x88 | 0x8A => {
                if prefix.w {
                    VecElementType::F64
                } else {
                    VecElementType::F32
                }
            }
            0x89 | 0x8B => {
                if prefix.w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                }
            }
            _ => unreachable!(),
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
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
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if compress && modrm.is_memory && prefix.zeroing {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let reg = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let rm = self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width);
        let mut ops = Vec::new();
        if !modrm.is_memory {
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                if compress {
                    OpKind::VCompress {
                        dst: rm,
                        src: reg,
                        mask,
                        elem,
                        width: prefix.width,
                        zeroing: prefix.zeroing,
                    }
                } else {
                    OpKind::VExpand {
                        dst: reg,
                        src: rm,
                        mask,
                        elem,
                        width: prefix.width,
                        zeroing: prefix.zeroing,
                    }
                },
            ));
            return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
        }

        let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
            prefix,
            modrm.addr.as_ref().unwrap(),
            next_pc,
            elem.bytes(),
            ctx,
        );
        ops.extend(pre_ops);
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mut count = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: count,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        let lanes = prefix.width.lanes(elem) as u8;
        let mem_width = match elem.bytes() {
            1 => MemWidth::B1,
            2 => MemWidth::B2,
            4 => MemWidth::B4,
            8 => MemWidth::B8,
            _ => unreachable!(),
        };

        if compress {
            for lane in 0..lanes {
                let active = self.append_mask_bit_condition(mask, lane, pc, ctx, &mut ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: reg,
                        lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredStore {
                        src: SrcOperand::Reg(scalar),
                        cond: active,
                        addr: Address::BaseIndexScale {
                            base: Some(base),
                            index: count,
                            scale: elem.bytes() as u8,
                            disp: 0,
                            disp_size: DispSize::Auto,
                        },
                        width: mem_width,
                    },
                ));
                let incremented = ctx.alloc_vreg();
                let selected = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Add {
                        dst: incremented,
                        src1: count,
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Select {
                        dst: selected,
                        cond: active,
                        src_true: incremented,
                        src_false: count,
                        width: OpWidth::W64,
                    },
                ));
                count = selected;
            }
        } else {
            let raw = if prefix.zeroing {
                self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops)
            } else {
                let raw = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst: raw,
                        src: reg,
                        width: prefix.width,
                    },
                ));
                raw
            };
            for lane in 0..lanes {
                let active = self.append_mask_bit_condition(mask, lane, pc, ctx, &mut ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: raw,
                        lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: scalar,
                        cond: active,
                        addr: Address::BaseIndexScale {
                            base: Some(base),
                            index: count,
                            scale: elem.bytes() as u8,
                            disp: 0,
                            disp_size: DispSize::Auto,
                        },
                        width: mem_width,
                        signed: SignExtend::Zero,
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
                let incremented = ctx.alloc_vreg();
                let selected = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Add {
                        dst: incremented,
                        src1: count,
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Select {
                        dst: selected,
                        cond: active,
                        src_true: incremented,
                        src_false: count,
                        width: OpWidth::W64,
                    },
                ));
                count = selected;
            }
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst: reg,
                    src: raw,
                    width: prefix.width,
                },
                self.vec_hint(prefix, opcode),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
