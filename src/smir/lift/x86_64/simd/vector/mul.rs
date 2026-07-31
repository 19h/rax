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
    /// Multiply the even dword lanes into full qword products.
    /// Every input is extracted before the output vector is initialized, which
    /// makes all architectural source/destination alias combinations safe.
    pub(crate) fn append_pmuldq(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(VecElementType::I64) as u8;
        let mut products = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let a = ctx.alloc_vreg();
            let b = ctx.alloc_vreg();
            let product = ctx.alloc_vreg();
            let source_lane = lane * 2;
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: a,
                    vec: src1,
                    lane: source_lane,
                    elem: VecElementType::I32,
                    sign: if signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: b,
                    vec: src2,
                    lane: source_lane,
                    elem: VecElementType::I32,
                    sign: if signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                if signed {
                    OpKind::MulS {
                        dst_lo: product,
                        dst_hi: None,
                        src1: a,
                        src2: SrcOperand::Reg(b),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    }
                } else {
                    OpKind::MulU {
                        dst_lo: product,
                        dst_hi: None,
                        src1: a,
                        src2: SrcOperand::Reg(b),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    }
                },
            ));
            products.push(product);
        }
        let output = self.append_zero_vector(width, VecElementType::I64, pc, ctx, ops);
        for (lane, product) in products.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar: product,
                    lane: lane as u8,
                    elem: VecElementType::I64,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width,
            },
        ));
    }

    pub(crate) fn pmaddwd_kind(dst: VReg, src1: VReg, src2: VReg, width: VecWidth) -> OpKind {
        // The zero accumulator selects the non-accumulating PMADDWD semantic
        // subset of VDotProduct. Narrowing to I32 performs the instruction's
        // unique 0x8000*0x8000 + 0x8000*0x8000 wrap to 0x8000_0000.
        OpKind::VDotProduct {
            dst,
            acc: VReg::Imm(0),
            src1,
            src2,
            mask: None,
            src_elem: VecElementType::I16,
            acc_elem: VecElementType::I32,
            width,
            src1_unsigned: false,
            saturate: false,
            zeroing: false,
        }
    }

    /// Carry-less multiply one selected qword pair in every independent
    /// 128-bit block. imm8[0] selects src1's qword and imm8[4] selects src2's;
    /// all other immediate bits are architecturally ignored.
    pub(crate) fn append_pclmulqdq(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        imm: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let blocks = (width.bytes() / 16) as u8;
        let select1 = imm & 1;
        let select2 = (imm >> 4) & 1;
        let mut products = Vec::with_capacity(blocks as usize);
        for block in 0..blocks {
            let lhs = ctx.alloc_vreg();
            let rhs = ctx.alloc_vreg();
            let lo = ctx.alloc_vreg();
            let hi = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: lhs,
                    vec: src1,
                    lane: block * 2 + select1,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: rhs,
                    vec: src2,
                    lane: block * 2 + select2,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::ClMul {
                    dst: lo,
                    dst_hi: Some(hi),
                    src1: SrcOperand::Reg(lhs),
                    src2: SrcOperand::Reg(rhs),
                    elem_bits: 64,
                    lanes: 1,
                    acc: false,
                },
            ));
            products.push((lo, hi));
        }

        let output = self.append_zero_vector(width, VecElementType::I64, pc, ctx, ops);
        for (block, (lo, hi)) in products.into_iter().enumerate() {
            for (offset, scalar) in [(0u8, lo), (1, hi)] {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: output,
                        vec: output,
                        scalar,
                        lane: block as u8 * 2 + offset,
                        elem: VecElementType::I64,
                    },
                ));
            }
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width,
            },
        ));
    }

    pub(crate) fn lift_vec_vpmadd52(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || !prefix.w
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
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
        if prefix.b && (prefix.encoding != VecEncodingKind::Evex || !modrm.is_memory) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b { 8 } else { prefix.width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I64,
                    prefix.width,
                    prefix.b,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if prefix.b {
                self.append_broadcast_memory_source(
                    addr,
                    VecElementType::I64,
                    prefix.width,
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
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMultiplyAdd52 {
                dst,
                acc: dst,
                src1,
                src2,
                mask,
                width: prefix.width,
                high: opcode == 0xB5,
                zeroing: prefix.encoding == VecEncodingKind::Evex && prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_vnni_dot(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
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
        if prefix.b && (prefix.encoding != VecEncodingKind::Evex || !modrm.is_memory) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b { 4 } else { prefix.width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I32,
                    prefix.width,
                    prefix.b,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if prefix.b {
                self.append_broadcast_memory_source(
                    addr,
                    VecElementType::I32,
                    prefix.width,
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
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VDotProduct {
                dst,
                acc: dst,
                src1,
                src2,
                mask,
                src_elem: if opcode < 0x52 {
                    VecElementType::I8
                } else {
                    VecElementType::I16
                },
                acc_elem: VecElementType::I32,
                width: prefix.width,
                src1_unsigned: opcode < 0x52,
                saturate: opcode & 1 != 0,
                zeroing: prefix.encoding == VecEncodingKind::Evex && prefix.zeroing,
            },
        ));
        let result = LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed);
        Ok(self.retain_evex_memory_apx_requirement(&modrm, pc, result))
    }

    pub(crate) fn lift_vec_pclmulqdq(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || prefix.aaa != 0
            || prefix.zeroing
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
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
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
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        self.append_pclmulqdq(dst, src1, src2, prefix.width, imm, pc, ctx, &mut ops);
        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, imm_offset + 1),
        ))
    }

    pub(crate) fn lift_vec_pmuldq(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && !prefix.w)
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
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
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast { 8 } else { prefix.width.bytes() },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I64,
                    prefix.width,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
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
                        dst: loaded,
                        scalar,
                        elem: VecElementType::I64,
                        lanes: prefix.width.lanes(VecElementType::I64) as u8,
                    },
                ));
                loaded
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
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_pmuldq(raw, src1, src2, prefix.width, signed, pc, ctx, &mut ops);
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I64,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            self.append_pmuldq(dst, src1, src2, prefix.width, signed, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_pmul_low(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0xD5 => VecElementType::I16,
            0x40 if prefix.encoding == VecEncodingKind::Evex && prefix.w => VecElementType::I64,
            0x40 => VecElementType::I32,
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
        let broadcast = opcode == 0x40
            && prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast {
                        elem.bytes()
                    } else {
                        prefix.width.bytes()
                    },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
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
                        dst: loaded,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
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
                        width: prefix.width,
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
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let masked = prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0;
        let raw = if masked { ctx.alloc_vreg() } else { dst };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMul {
                dst: raw,
                src1,
                src2,
                elem,
                lanes: prefix.width.lanes(elem) as u8,
            },
            self.vec_hint(prefix, opcode),
        ));
        if masked {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_pmul_high_word(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
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
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I16,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
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
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && mask.is_none() {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(dst, src1, src2, prefix.width, opcode == 0xE5),
                self.vec_hint(prefix, opcode),
            ));
        } else {
            let raw = if prefix.encoding == VecEncodingKind::Evex {
                ctx.alloc_vreg()
            } else {
                dst
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(raw, src1, src2, prefix.width, opcode == 0xE5),
            ));
            if prefix.encoding == VecEncodingKind::Evex {
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
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_pmaddwd(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
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
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
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
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && (prefix.encoding == VecEncodingKind::Vex || prefix.aaa == 0) {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(dst, src1, src2, prefix.width),
                self.vec_hint(prefix, 0xF5),
            ));
        } else if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(raw, src1, src2, prefix.width),
            ));
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I32,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(dst, src1, src2, prefix.width),
            ));
        }
        let result = LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed);
        Ok(if prefix.encoding == VecEncodingKind::Evex {
            self.retain_evex_memory_apx_requirement(&modrm, pc, result)
        } else {
            result
        })
    }
}
