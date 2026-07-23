//! VEX-encoded AVX / AVX2 instruction lifting

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
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl X86_64Lifter {
    pub(crate) fn lift_vex_permute2x128(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.width != VecWidth::V256
            || prefix.w
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
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&imm) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V256,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.ymm(modrm.rm)
        };
        let src1 = self.ymm(prefix.vvvv);
        let mut selected = Vec::new();
        for (output_half, control_shift, zero_bit) in [(0u8, 0u8, 3u8), (1, 4, 7)] {
            if (imm >> zero_bit) & 1 != 0 {
                continue;
            }
            let control = (imm >> control_shift) & 3;
            let source = if control < 2 { src1 } else { src2 };
            let source_half = control & 1;
            for lane_in_half in 0u8..2 {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: source_half * 2 + lane_in_half,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                selected.push((output_half * 2 + lane_in_half, scalar));
            }
        }
        let output =
            self.append_zero_vector(VecWidth::V256, VecElementType::I64, pc, ctx, &mut ops);
        for (lane, scalar) in selected {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar,
                    lane,
                    elem: VecElementType::I64,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst: self.ymm(modrm.reg),
                src: output,
                width: VecWidth::V256,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vex_vnni_dot_ext(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let decoded = match (opcode, prefix.pp) {
            // AVX-VNNI-INT8: SS, SU, and UU byte products.
            (0x50 | 0x51, X86SsePrefix::Repne) => Some((VecElementType::I8, true, true)),
            (0x50 | 0x51, X86SsePrefix::Rep) => Some((VecElementType::I8, true, false)),
            (0x50 | 0x51, X86SsePrefix::None) => Some((VecElementType::I8, false, false)),
            // AVX-VNNI-INT16: SU, US, and UU word products.
            (0xD2 | 0xD3, X86SsePrefix::Rep) => Some((VecElementType::I16, true, false)),
            (0xD2 | 0xD3, X86SsePrefix::OpSize) => Some((VecElementType::I16, false, true)),
            (0xD2 | 0xD3, X86SsePrefix::None) => Some((VecElementType::I16, false, false)),
            _ => None,
        };
        let Some((src_elem, src1_signed, src2_signed)) = decoded else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        if prefix.encoding != VecEncodingKind::Vex || prefix.w {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                prefix.width.bytes(),
                ctx,
            );
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        let dst = self.vec_reg(modrm.reg, prefix.width);
        let src1 = self.vec_reg(prefix.vvvv, prefix.width);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VDotProductExt {
                dst,
                acc: dst,
                src1,
                src2,
                src_elem,
                acc_elem: VecElementType::I32,
                width: prefix.width,
                src1_signed,
                src2_signed,
                saturate: opcode & 1 != 0,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_maskmovdqu(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mut ops = Vec::new();
        self.append_maskmov(
            self.xmm(modrm.reg),
            self.xmm(modrm.rm),
            16,
            prefix.address_size_override,
            prefix.segment_override,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_masked_memory(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || (matches!(opcode, 0x2C..=0x2F) && prefix.w)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let load = matches!(opcode, 0x2C | 0x2D | 0x8C);
        let elem = match opcode {
            0x2C | 0x2E => VecElementType::F32,
            0x2D | 0x2F => VecElementType::F64,
            0x8C | 0x8E if prefix.w => VecElementType::I64,
            0x8C | 0x8E => VecElementType::I32,
            _ => unreachable!(),
        };
        let mem_width = if elem.bytes() == 4 {
            MemWidth::B4
        } else {
            MemWidth::B8
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
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mask = self.vec_reg(prefix.vvvv, prefix.width);
        let lanes = prefix.width.lanes(elem) as u8;
        let mut active = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let mask_lane = ctx.alloc_vreg();
            let condition = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: mask_lane,
                    vec: mask,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: condition,
                    src: mask_lane,
                    amount: SrcOperand::Imm(i64::from(elem.bytes() * 8 - 1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            active.push(condition);
        }

        if load {
            let dst = self.vec_reg(modrm.reg, prefix.width);
            let loaded = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
            for (lane, condition) in active.into_iter().enumerate() {
                let scalar = ctx.alloc_vreg();
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
                        cond: condition,
                        addr: Address::base_off(base, lane as i64 * i64::from(elem.bytes())),
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
                        lane: lane as u8,
                        elem,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: loaded,
                    width: prefix.width,
                },
            ));
        } else {
            let data = self.vec_reg(modrm.reg, prefix.width);
            let mut values = Vec::with_capacity(lanes as usize);
            for lane in 0..lanes {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: data,
                        lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                values.push(scalar);
            }
            for (lane, (condition, scalar)) in active.into_iter().zip(values).enumerate() {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredStore {
                        src: SrcOperand::Reg(scalar),
                        cond: condition,
                        addr: Address::base_off(base, lane as i64 * i64::from(elem.bytes())),
                        width: mem_width,
                    },
                ));
            }
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_andn_0f38(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::None
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::AndNot {
                dst: self.gpr(modrm.reg),
                src1: src2,
                src2: SrcOperand::Reg(self.gpr(prefix.vvvv)),
                width,
                flags: FlagUpdate::Specific(
                    FlagSet::CF
                        .union(FlagSet::ZF)
                        .union(FlagSet::SF)
                        .union(FlagSet::OF),
                ),
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bls_0f38(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::None
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let kind = match (modrm.byte >> 3) & 0x07 {
            1 => X86BlsKind::Blsr,
            2 => X86BlsKind::Blsmsk,
            3 => X86BlsKind::Blsi,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes[..prefix.bytes + 1 + modrm.bytes_consumed].to_vec(),
                });
            }
        };
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Bls {
                dst: self.gpr(prefix.vvvv),
                src,
                width,
                kind,
                flags: x86_bls_flags(),
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bzhi_bextr_0f38(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::None
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        let dst = self.gpr(modrm.reg);
        let control = self.gpr(prefix.vvvv);
        let kind = match opcode {
            0xF5 => OpKind::Bzhi {
                dst,
                src,
                index: control,
                width,
                flags: x86_bzhi_flags(),
            },
            0xF7 => OpKind::Bextr {
                dst,
                src,
                control,
                width,
                flags: x86_bextr_flags(),
            },
            _ => unreachable!("VEX BZHI/BEXTR only dispatches F5/F7"),
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_pdep_pext_0f38(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        let dst = self.gpr(modrm.reg);
        let src = self.gpr(prefix.vvvv);
        let op = match prefix.pp {
            X86SsePrefix::Rep => OpKind::Pext {
                dst,
                src,
                mask,
                width,
            },
            X86SsePrefix::Repne => OpKind::Pdep {
                dst,
                src,
                mask,
                width,
            },
            _ => unreachable!("PDEP/PEXT are only dispatched for F2/F3 VEX prefixes"),
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_mulx_0f38(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::Repne
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            rep_prefix: Some(0xF2),
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::MulU {
                dst_lo: self.gpr(prefix.vvvv),
                dst_hi: Some(self.gpr(modrm.reg)),
                src1: self.gpr(2),
                src2: SrcOperand::Reg(src2),
                width,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bmi2_shift_0f38(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || !matches!(
                prefix.pp,
                X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
            )
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            rep_prefix: match prefix.pp {
                X86SsePrefix::OpSize => Some(0x66),
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let next_pc = pc + prefix.bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        let dst = self.gpr(modrm.reg);
        let count = self.gpr(prefix.vvvv);
        let masked_count = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::And {
                dst: masked_count,
                src1: count,
                src2: SrcOperand::Imm((width.bits() - 1) as i64),
                width,
                flags: FlagUpdate::None,
            },
        ));

        let amount = SrcOperand::Reg(masked_count);
        let op = match prefix.pp {
            X86SsePrefix::Rep => OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags: FlagUpdate::None,
            },
            X86SsePrefix::Repne => OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags: FlagUpdate::None,
            },
            X86SsePrefix::OpSize => OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags: FlagUpdate::None,
            },
            _ => unreachable!("BMI2 VEX shifts require 66/F2/F3 prefix encodings"),
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_bmi2_rorx_0f3a(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.pp != X86SsePrefix::Repne
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            rep_prefix: Some(0xF2),
            cursor: prefix.bytes + 1,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[prefix.bytes + 1..], &modrm_prefix, pc)?;
        let imm_offset = prefix.bytes + 1 + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }

        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Ror {
                dst: self.gpr(modrm.reg),
                src,
                amount: SrcOperand::Imm(bytes[imm_offset] as i64),
                width,
                flags: FlagUpdate::None,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.bytes + 1 + modrm.bytes_consumed + 1,
        ))
    }

    pub(crate) fn lift_vex_integer_compare(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, cond) = match opcode {
            0x64 => (VecElementType::I8, VecCmpCond::Gt),
            0x65 => (VecElementType::I16, VecCmpCond::Gt),
            0x66 => (VecElementType::I32, VecCmpCond::Gt),
            0x74 => (VecElementType::I8, VecCmpCond::Eq),
            0x75 => (VecElementType::I16, VecCmpCond::Eq),
            0x76 => (VecElementType::I32, VecCmpCond::Eq),
            0x29 => (VecElementType::I64, VecCmpCond::Eq),
            0x37 => (VecElementType::I64, VecCmpCond::Gt),
            _ => unreachable!(),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let after_opcode = &bytes[cursor..];
        let modrm = decode_modrm(after_opcode, &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src1: self.vec_reg(prefix.vvvv, prefix.width),
                src2,
                cond,
                elem,
                lanes: prefix.width.lanes(elem) as u8,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_integer_unpack(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x60 | 0x68 => VecElementType::I8,
            0x61 | 0x69 => VecElementType::I16,
            0x62 | 0x6A => VecElementType::I32,
            0x6C | 0x6D => VecElementType::I64,
            _ => unreachable!(),
        };
        let high = matches!(opcode, 0x68 | 0x69 | 0x6A | 0x6D);
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        self.append_integer_interleave(
            self.vec_reg(modrm.reg, prefix.width),
            self.vec_reg(prefix.vvvv, prefix.width),
            src2,
            elem,
            prefix.width,
            high,
            self.vec_hint(prefix, opcode),
            pc,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_integer_pack(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let src_elem = match opcode {
            0x63 | 0x67 => VecElementType::I16,
            0x6B | 0x2B => VecElementType::I32,
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
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        let src_lanes = prefix.width.lanes(src_elem) as u8;
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPackSat {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src1: src2,
                src2: self.vec_reg(prefix.vvvv, prefix.width),
                src_elem,
                to_unsigned: matches!(opcode, 0x67 | 0x2B),
                src_lanes,
                block_lanes: (16 / src_elem.bytes()) as u8,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_pshufb(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
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
        let mut ops = Vec::new();
        let control = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VByteShuffle {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src: self.vec_reg(prefix.vvvv, prefix.width),
                control,
                lanes: prefix.width.lanes(VecElementType::I8) as u8,
                block_lanes: 16,
            },
            self.vec_hint(prefix, 0x00),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_horizontal_integer(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if matches!(opcode, 0x02 | 0x06) {
            VecElementType::I32
        } else {
            VecElementType::I16
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        let lanes = prefix.width.lanes(elem) as u8;
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VHorizontalBin {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src1: self.vec_reg(prefix.vvvv, prefix.width),
                src2,
                elem,
                lanes,
                block_lanes: (16 / elem.bytes()) as u8,
                subtract: matches!(opcode, 0x05..=0x07),
                saturating: matches!(opcode, 0x03 | 0x07),
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_pmaddubsw(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
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
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        let kind = OpKind::VDotProduct {
            dst: self.vec_reg(modrm.reg, prefix.width),
            acc: VReg::Imm(0),
            src1: self.vec_reg(prefix.vvvv, prefix.width),
            src2,
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I16,
            width: prefix.width,
            src1_unsigned: true,
            saturate: true,
            zeroing: false,
        };
        if modrm.is_memory {
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                self.vec_hint(prefix, 0x04),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_psign(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x08 => VecElementType::I8,
            0x09 => VecElementType::I16,
            0x0A => VecElementType::I32,
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
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let control = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        let dst = self.vec_reg(modrm.reg, prefix.width);
        let value = self.vec_reg(prefix.vvvv, prefix.width);
        if modrm.is_memory {
            self.append_packed_sign(dst, value, control, elem, prefix.width, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1: value,
                    src2: control,
                    elem,
                    lanes: prefix.width.lanes(elem) as u8,
                    op: VLaneOp::Sign,
                    signed: true,
                    set_ovf: false,
                },
                self.vec_hint(prefix, opcode),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_pmulhrsw(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
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
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        let kind = OpKind::VMulShiftSat {
            dst: self.vec_reg(modrm.reg, prefix.width),
            src1: self.vec_reg(prefix.vvvv, prefix.width),
            src2,
            src_elem: VecElementType::I16,
            lanes: prefix.width.lanes(VecElementType::I16) as u8,
            signed1: true,
            signed2: true,
            shift_left: 0,
            round: true,
            sat_bits: 0,
            out_shift: 15,
        };
        if modrm.is_memory {
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                self.vec_hint(prefix, 0x0B),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_pabs(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x1C => VecElementType::I8,
            0x1D => VecElementType::I16,
            0x1E => VecElementType::I32,
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
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VUnary {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src,
                elem,
                lanes: prefix.width.lanes(elem) as u8,
                op: VecUnaryOp::Abs,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_ptest(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
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
        let mut ops = Vec::new();
        let second = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        self.append_ptest_flags(
            self.vec_reg(modrm.reg, prefix.width),
            second,
            prefix.width,
            None,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_testp(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || prefix.vvvv != 0
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
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
        let mut ops = Vec::new();
        let second = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        self.append_ptest_flags(
            self.vec_reg(modrm.reg, prefix.width),
            second,
            prefix.width,
            Some(if opcode == 0x0E {
                0x8000_0000_8000_0000
            } else {
                0x8000_0000_0000_0000
            }),
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_phminposuw(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || prefix.width != VecWidth::V128
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
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(modrm.rm, VecWidth::V128)
        };
        let dst = self.vec_reg(modrm.reg, VecWidth::V128);
        if modrm.is_memory {
            let raw = ctx.alloc_vreg();
            self.append_phminposuw(raw, src, pc, ctx, &mut ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: raw,
                    width: VecWidth::V128,
                },
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86Phminposuw { dst, src },
                self.vec_hint(prefix, 0x41),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_sha512(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::Repne
            || prefix.width != VecWidth::V256
            || prefix.w
            || (opcode != 0xCB && prefix.vvvv != 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let dst = self.ymm(modrm.reg);
        let rm = modrm.rm;
        let kind = match opcode {
            0xCC => OpKind::X86Sha512Msg1 {
                dst,
                src: self.xmm(rm),
            },
            0xCD => OpKind::X86Sha512Msg2 {
                dst,
                src: self.ymm(rm),
            },
            0xCB => OpKind::X86Sha512Rounds2 {
                dst,
                state: self.ymm(prefix.vvvv),
                wk: self.xmm(rm),
            },
            _ => unreachable!(),
        };
        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, kind)],
            cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_vex_sm3_message(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.width != VecWidth::V128
            || prefix.w
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let src1 = self.xmm(prefix.vvvv);
        let kind = if prefix.pp == X86SsePrefix::None {
            OpKind::X86Sm3Msg1 { dst, src1, src2 }
        } else {
            OpKind::X86Sm3Msg2 { dst, src1, src2 }
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_sm3_rounds2(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.width != VecWidth::V128
            || prefix.w
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
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
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let words = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Sm3Rounds2 {
                dst: self.xmm(modrm.reg),
                state: self.xmm(prefix.vvvv),
                words,
                imm: bytes[imm_offset],
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vex_sm4(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
            || prefix.w
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Sm4 {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src1: self.vec_reg(prefix.vvvv, prefix.width),
                src2,
                width: prefix.width,
                key_schedule: prefix.pp == X86SsePrefix::Rep,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_ne_convert(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let broadcast = opcode == 0xB1;
        let valid_prefix = if broadcast {
            matches!(prefix.pp, X86SsePrefix::OpSize | X86SsePrefix::Rep)
        } else {
            matches!(
                prefix.pp,
                X86SsePrefix::None | X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
            )
        };
        if prefix.encoding != VecEncodingKind::Vex
            || !valid_prefix
            || !matches!(prefix.width, VecWidth::V128 | VecWidth::V256)
            || prefix.w
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        let src = ctx.alloc_vreg();
        if broadcast {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: src,
                    addr,
                    width: MemWidth::B2,
                    sign: SignExtend::Zero,
                },
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: src,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
        }

        let fp16 = if broadcast {
            prefix.pp == X86SsePrefix::OpSize
        } else {
            matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
        };
        let odd = !broadcast && matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::Repne);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Convert16ToFp32 {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src,
                width: prefix.width,
                fp16,
                odd,
                broadcast,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vex_dot_product(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = if opcode == 0x40 {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || (elem == VecElementType::F32
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (elem == VecElementType::F64 && prefix.width != VecWidth::V128)
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
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86DotProduct {
                dst: self.vec_reg(modrm.reg, prefix.width),
                src1: self.vec_reg(prefix.vvvv, prefix.width),
                src2,
                elem,
                width: prefix.width,
                imm: bytes[imm_offset],
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vex_round(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = matches!(opcode, 0x0A | 0x0B);
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || (!scalar && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (!scalar && prefix.vvvv != 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if matches!(opcode, 0x08 | 0x0A) {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
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
        let imm = bytes[imm_offset];
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if scalar {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: loaded,
                        addr,
                        width: if elem == VecElementType::F32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        },
                        sign: SignExtend::Zero,
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
            self.vec_reg(modrm.rm, width)
        };
        let mode = if imm & 4 != 0 {
            FpRoundMode::Dynamic
        } else {
            match imm & 3 {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        };
        let dst = self.vec_reg(modrm.reg, width);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Round {
                dst,
                merge: if scalar { self.xmm(prefix.vvvv) } else { dst },
                src,
                elem,
                width,
                lanes: if scalar { 1 } else { width.lanes(elem) as u8 },
                scalar_source: scalar,
                zero_upper: true,
                mode,
                suppress_precision: imm & 8 != 0,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
