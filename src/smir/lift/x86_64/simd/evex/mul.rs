//! mul.rs

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
    pub(crate) fn lift_evex_bf16_dot(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::Rep
            || prefix.w
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = prefix.modrm_prefix(cursor);
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
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b { 4 } else { prefix.width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            match (mask, prefix.b) {
                (Some(mask), true) => self.append_masked_broadcast_memory_source(
                    addr,
                    VecElementType::F32,
                    prefix.width,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (Some(mask), false) => self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::F32,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (None, true) => self.append_broadcast_memory_source(
                    addr,
                    VecElementType::F32,
                    prefix.width,
                    pc,
                    ctx,
                    &mut ops,
                ),
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
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VDotProductBF16 {
                dst,
                acc: dst,
                src1,
                src2,
                mask,
                width: prefix.width,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed),
        ))
    }

    pub(crate) fn lift_evex_four_dot_product(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::Repne
            || prefix.w
            || prefix.b
            || prefix.l_bits == 3
            || prefix.width != VecWidth::V512
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
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let bytes_consumed = cursor + modrm.bytes_consumed;
        let next_pc = pc + bytes_consumed as u64;
        let (addr, mut ops) =
            self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 16, ctx);
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let memory = self.append_evex_whole_tuple_128(addr, mask, 0xFFFF, pc, ctx, &mut ops);
        let source_index = prefix.vvvv + if prefix.v_high { 16 } else { 0 };
        let source_base = source_index & !3;
        let source = |offset| self.vec_reg(source_base + offset, VecWidth::V512);
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            VecWidth::V512,
        );
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FourDotProduct {
                dst,
                src0: source(0),
                src1: source(1),
                src2: source(2),
                src3: source(3),
                mem: memory,
                mask,
                saturating: opcode == 0x53,
                mask_zeroing: prefix.zeroing,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, bytes_consumed),
        ))
    }

    pub(crate) fn lift_evex_pmaddubsw(
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
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) =
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx);
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
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        if !modrm.is_memory && prefix.aaa == 0 {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VDotProduct {
                    dst,
                    acc: VReg::Imm(0),
                    src1,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: prefix.width,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
                self.vec_hint(prefix, 0x04),
            ));
        } else {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VDotProduct {
                    dst: raw,
                    acc: VReg::Imm(0),
                    src1,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: prefix.width,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
            ));
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I16,
                pc,
                ctx,
                &mut ops,
            );
        }
        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed),
        ))
    }

    pub(crate) fn lift_evex_pmulhrsw(
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
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let lanes = prefix.width.lanes(VecElementType::I16) as u8;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) =
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if prefix.aaa == 0 {
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
                let loaded =
                    self.append_zero_vector(prefix.width, VecElementType::I16, pc, ctx, &mut ops);
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
                            addr: Address::base_off(base, i64::from(lane) * 2),
                            width: MemWidth::B2,
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
                            elem: VecElementType::I16,
                        },
                    ));
                }
                loaded
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        if !modrm.is_memory && prefix.aaa == 0 {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMulShiftSat {
                    dst,
                    src1,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
                self.vec_hint(prefix, 0x0B),
            ));
        } else {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMulShiftSat {
                    dst: raw,
                    src1,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
            ));
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I16,
                pc,
                ctx,
                &mut ops,
            );
        }
        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed),
        ))
    }
}
