//! shuffle.rs

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};

impl X86_64Lifter {
    pub(crate) fn append_conflict_masked_memory_source(
        &self,
        addr: Address,
        elem: VecElementType,
        width: VecWidth,
        broadcast: bool,
        mask: VReg,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let lanes = width.lanes(elem) as u8;
        let loaded = self.append_zero_vector(width, elem, pc, ctx, ops);
        let bounded_mask = ctx.alloc_vreg();
        let valid_mask = if lanes == 64 {
            u64::MAX
        } else {
            (1u64 << lanes) - 1
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::And {
                dst: bounded_mask,
                src1: mask,
                src2: SrcOperand::Imm(valid_mask as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = if elem == VecElementType::I32 {
            MemWidth::B4
        } else {
            MemWidth::B8
        };
        for lane in 0..lanes {
            let required = ctx.alloc_vreg();
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: required,
                    src: bounded_mask,
                    amount: SrcOperand::Imm(i64::from(lane)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            let mut folded = required;
            for shift in [32, 16, 8, 4, 2, 1] {
                let upper = ctx.alloc_vreg();
                let combined = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: upper,
                        src: folded,
                        amount: SrcOperand::Imm(shift),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Or {
                        dst: combined,
                        src1: folded,
                        src2: SrcOperand::Reg(upper),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                folded = combined;
            }
            let required_bit = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: required_bit,
                    src1: folded,
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::PredLoad {
                    dst: scalar,
                    cond: required_bit,
                    addr: Address::base_off(
                        base,
                        if broadcast {
                            0
                        } else {
                            i64::from(lane) * i64::from(elem.bytes())
                        },
                    ),
                    width: mem_width,
                    signed: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: loaded,
                    vec: loaded,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        loaded
    }

    pub(crate) fn lift_evex_permute_two_table(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let overwrite_table = matches!(opcode, 0x7D..=0x7F);
        let elem = match (opcode, prefix.w) {
            (0x75 | 0x7D, false) => VecElementType::I8,
            (0x75 | 0x7D, true) => VecElementType::I16,
            (0x76 | 0x7E, false) => VecElementType::I32,
            (0x76 | 0x7E, true) => VecElementType::I64,
            (0x77 | 0x7F, false) => VecElementType::F32,
            (0x77 | 0x7F, true) => VecElementType::F64,
            _ => unreachable!(),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast_allowed = matches!(
            elem,
            VecElementType::I32 | VecElementType::I64 | VecElementType::F32 | VecElementType::F64
        );
        if prefix.b && (!modrm.is_memory || !broadcast_allowed) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let broadcast = prefix.b;
        let mut ops = Vec::new();
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let vvvv = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let (table1, indices) = if overwrite_table {
            (dst, vvvv)
        } else {
            (vvvv, dst)
        };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let direct_vbmi =
            !modrm.is_memory && matches!(elem, VecElementType::I8 | VecElementType::I16);
        let raw = if modrm.is_memory {
            let scale = if broadcast {
                elem.bytes()
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
            self.append_two_table_permute_memory_result(
                table1,
                addr,
                indices,
                prefix.width,
                elem,
                broadcast,
                mask,
                overwrite_table,
                pc,
                ctx,
                &mut ops,
            )
        } else {
            let table2 = self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width);
            let raw = if direct_vbmi { dst } else { ctx.alloc_vreg() };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                if direct_vbmi {
                    OpKind::X86PermuteBytesWords {
                        dst: raw,
                        table1,
                        table2: Some(table2),
                        indices,
                        mask,
                        elem,
                        width: prefix.width,
                        overwrite_table,
                        zeroing: prefix.zeroing,
                    }
                } else {
                    OpKind::VPermute {
                        dst: raw,
                        src1: table1,
                        src2: Some(table2),
                        indices,
                        elem,
                        width: prefix.width,
                        overwrite_table,
                    }
                },
            ));
            raw
        };
        if !direct_vbmi {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_vpconflict(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
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
        let elem = if prefix.w {
            VecElementType::I64
        } else {
            VecElementType::I32
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.b && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src = if modrm.is_memory {
            let scale = if prefix.b {
                elem.bytes()
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
            if let Some(mask) = mask {
                self.append_conflict_masked_memory_source(
                    addr,
                    elem,
                    prefix.width,
                    prefix.b,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if prefix.b {
                self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
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
        let raw = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VConflict {
                dst,
                src,
                mask,
                elem,
                width: prefix.width,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_vpshufbitqmb(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || prefix.l_bits == 3
            || prefix.zeroing
            || prefix.b
            || prefix.reg_high
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if modrm.reg >= 8 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let indices = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                prefix.width.bytes(),
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I8,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
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
        let dst = VReg::Arch(ArchReg::X86(X86Reg::K(modrm.reg)));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffleBitQM {
                dst,
                src: self.vec_reg(
                    prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                    prefix.width,
                ),
                indices,
                mask,
                width: prefix.width,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_ternary_logic(
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
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
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
        let src3 = if modrm.is_memory {
            let broadcast = prefix.b;
            let tuple_bytes = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                tuple_bytes,
                ctx,
            );
            ops.extend(pre_ops);
            if prefix.aaa != 0 {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    broadcast,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::I32 {
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
                        dst: vector,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                vector
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
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let src2 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86TernaryLogic {
                dst,
                src1: dst,
                src2,
                src3,
                mask: (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                imm: bytes[imm_offset],
                width: prefix.width,
                elem,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

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
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
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
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::I32 {
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
                        dst: vector,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                vector
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

    pub(crate) fn lift_evex_multishift_qb(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || !prefix.w
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
        if prefix.b && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let source = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b { 8 } else { prefix.width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            if prefix.b {
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B8,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar,
                        elem: VecElementType::I64,
                        lanes: prefix.width.lanes(VecElementType::I64) as u8,
                    },
                ));
                vector
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
        let control = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86MultiShiftQB {
                dst,
                control,
                source,
                mask: (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                width: prefix.width,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_chunk_extract_insert(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let extract = matches!(opcode, 0x19 | 0x1B | 0x39 | 0x3B);
        let half_chunk = matches!(opcode, 0x1A | 0x1B | 0x3A | 0x3B);
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.zeroing && prefix.aaa == 0)
            || (half_chunk && prefix.width != VecWidth::V512)
            || (!half_chunk && !matches!(prefix.width, VecWidth::V256 | VecWidth::V512))
            || (extract && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = match (opcode < 0x30, prefix.w) {
            (true, false) => VecElementType::F32,
            (true, true) => VecElementType::F64,
            (false, false) => VecElementType::I32,
            (false, true) => VecElementType::I64,
        };
        let chunk_width = if half_chunk {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if extract && modrm.is_memory && prefix.zeroing {
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
        let imm = bytes[imm_offset];
        let chunk_lanes = chunk_width.lanes(elem) as u8;
        let chunks = (prefix.width.bytes() / chunk_width.bytes()) as u8;
        let chunk = imm & (chunks - 1);
        let first_lane = chunk * chunk_lanes;
        let reg_index = modrm.reg + if prefix.reg_high { 16 } else { 0 };
        let rm_index = modrm.rm + if prefix.rm_high { 16 } else { 0 };
        let mut ops = Vec::new();

        if extract {
            let source = self.vec_reg(reg_index, prefix.width);
            let raw = self.append_zero_vector(chunk_width, elem, pc, ctx, &mut ops);
            for lane in 0..chunk_lanes {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: first_lane + lane,
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

            if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    chunk_width.bytes(),
                    ctx,
                );
                ops.extend(pre_ops);
                if prefix.aaa == 0 {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VStore {
                            src: raw,
                            addr,
                            width: chunk_width,
                        },
                    ));
                } else {
                    // Type E6NF does not suppress memory faults. Materialize the
                    // complete destination, merge active elements, and write the
                    // complete chunk even when every writemask bit is clear.
                    let merged = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: merged,
                            addr: addr.clone(),
                            width: chunk_width,
                        },
                    ));
                    self.append_evex_vector_mask_result_width(
                        prefix,
                        merged,
                        raw,
                        elem,
                        chunk_width,
                        pc,
                        ctx,
                        &mut ops,
                    );
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VStore {
                            src: merged,
                            addr,
                            width: chunk_width,
                        },
                    ));
                }
            } else {
                self.append_evex_vector_mask_result_width(
                    prefix,
                    self.vec_reg(rm_index, chunk_width),
                    raw,
                    elem,
                    chunk_width,
                    pc,
                    ctx,
                    &mut ops,
                );
            }
        } else {
            let source2 = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    chunk_width.bytes(),
                    ctx,
                );
                ops.extend(pre_ops);
                let loaded = ctx.alloc_vreg();
                // E6NF requires the complete memory source to be accessed even
                // when the destination writemask contains no active elements.
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: chunk_width,
                    },
                ));
                loaded
            } else {
                self.vec_reg(rm_index, chunk_width)
            };
            let source1 = self.vec_reg(
                prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                prefix.width,
            );
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VAnd {
                    dst: raw,
                    src1: source1,
                    src2: source1,
                    width: prefix.width,
                },
            ));
            for lane in 0..chunk_lanes {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source2,
                        lane,
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
                        lane: first_lane + lane,
                        elem,
                    },
                ));
            }
            self.append_evex_vector_mask_result(
                prefix,
                self.vec_reg(reg_index, prefix.width),
                raw,
                elem,
                pc,
                ctx,
                &mut ops,
            );
        }

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_evex_shuffle_128_chunks(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V256 | VecWidth::V512)
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match (opcode, prefix.w) {
            (0x23, false) => VecElementType::F32,
            (0x23, true) => VecElementType::F64,
            (0x43, false) => VecElementType::I32,
            (0x43, true) => VecElementType::I64,
            _ => unreachable!(),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
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
        let imm = bytes[imm_offset];
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.b {
                self.vec_scalar_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    elem,
                    ctx,
                )
            } else {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            // E4NF requires the complete full tuple, or the scalar broadcast
            // tuple, to be accessed irrespective of the destination writemask.
            if prefix.b {
                self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
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
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
        let chunks = (prefix.width.bytes() / 16) as u8;
        let chunk_lanes = (16 / elem.bytes()) as u8;
        for dst_chunk in 0..chunks {
            let (source, selector) = if chunks == 2 {
                if dst_chunk == 0 {
                    (src1, imm & 1)
                } else {
                    (src2, (imm >> 1) & 1)
                }
            } else if dst_chunk < 2 {
                (src1, (imm >> (dst_chunk * 2)) & 3)
            } else {
                (src2, (imm >> (dst_chunk * 2)) & 3)
            };
            for chunk_lane in 0..chunk_lanes {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: selector * chunk_lanes + chunk_lane,
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
                        lane: dst_chunk * chunk_lanes + chunk_lane,
                        elem,
                    },
                ));
            }
        }
        self.append_evex_vector_mask_result(
            prefix,
            self.vec_reg(
                modrm.reg + if prefix.reg_high { 16 } else { 0 },
                prefix.width,
            ),
            raw,
            elem,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_evex_pshufb(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
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
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let lanes = prefix.width.lanes(VecElementType::I8) as u8;
        let mut ops = Vec::new();
        let control = if modrm.is_memory {
            let (addr, pre_ops) =
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            if prefix.aaa == 0 {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
            } else {
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
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar: zero,
                        elem: VecElementType::I8,
                        lanes,
                    },
                ));
                let base = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Lea { dst: base, addr },
                ));
                let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
                for lane in 0..lanes {
                    let shifted = ctx.alloc_vreg();
                    let active = ctx.alloc_vreg();
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Shr {
                            dst: shifted,
                            src: mask,
                            amount: SrcOperand::Imm(i64::from(lane)),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: active,
                            src1: shifted,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond: active,
                            addr: Address::base_off(base, i64::from(lane)),
                            width: MemWidth::B1,
                            signed: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst: loaded,
                            vec: loaded,
                            scalar,
                            lane,
                            elem: VecElementType::I8,
                        },
                    ));
                }
            }
            loaded
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
            OpKind::VByteShuffle {
                dst: raw,
                src: self.vec_reg(
                    prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                    prefix.width,
                ),
                control,
                lanes,
                block_lanes: 16,
            },
            self.vec_hint(prefix, 0x00),
        ));
        if prefix.aaa != 0 {
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I8,
                pc,
                ctx,
                &mut ops,
            );
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

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
            _ => unreachable!(),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
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
            if prefix.aaa != 0 {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    prefix.b,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if prefix.b {
                self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
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
                            _ => unreachable!(),
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
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
