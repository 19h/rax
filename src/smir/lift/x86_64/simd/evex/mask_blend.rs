//! EVEX opmask-selector blend lifting.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_mask_blend(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0x64..=0x66)
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match (opcode, prefix.w) {
            (0x64 | 0x65, false) => VecElementType::I32,
            (0x64 | 0x65, true) => VecElementType::I64,
            (0x66, false) => VecElementType::I8,
            (0x66, true) => VecElementType::I16,
            _ => unreachable!("validated EVEX mask-blend opcode"),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.b && (!modrm.is_memory || opcode == 0x66) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
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
            let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
            match (mask, prefix.b) {
                (Some(mask), true) => self.append_masked_broadcast_memory_source(
                    addr,
                    elem,
                    prefix.width,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (Some(mask), false) => self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (None, true) => {
                    self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
                }
                (None, false) => {
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
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let lanes = prefix.width.lanes(elem) as u8;
        let raw = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        for lane in 0..lanes {
            let active = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: active,
                    vec: src2,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            let selected = if prefix.aaa == 0 {
                active
            } else {
                let shifted = ctx.alloc_vreg();
                let cond = ctx.alloc_vreg();
                let fallback = if prefix.zeroing {
                    zero
                } else {
                    let fallback = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VExtractLane {
                            dst: fallback,
                            vec: src1,
                            lane,
                            elem,
                            sign: SignExtend::Zero,
                        },
                    ));
                    fallback
                };
                let selected = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: shifted,
                        src: VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: cond,
                        src1: shifted,
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
                        cond,
                        src_true: active,
                        src_false: fallback,
                        width: match elem {
                            VecElementType::I8 => OpWidth::W8,
                            VecElementType::I16 => OpWidth::W16,
                            VecElementType::I32 => OpWidth::W32,
                            VecElementType::I64 => OpWidth::W64,
                            _ => unreachable!("validated EVEX mask-blend element"),
                        },
                    },
                ));
                selected
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: raw,
                    vec: raw,
                    scalar: selected,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: raw,
                width: prefix.width,
            },
        ));
        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed),
        ))
    }
}
