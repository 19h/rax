//! fp.rs

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
    /// Materialize the register or memory source used by masked EVEX unary
    /// floating-point operations. Memory forms preserve EVEX
    /// fault suppression at element granularity; scalar broadcasts aggregate
    /// their lane predicates and perform at most one architectural read.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn materialize_evex_unary_fp_source(
        &self,
        prefix: VecPrefix,
        modrm: &ModRm,
        next_pc: u64,
        elem: VecElementType,
        width: VecWidth,
        scalar: bool,
        mask: Option<VReg>,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> (VReg, Vec<SmirOp>) {
        if !modrm.is_memory {
            return (
                self.vec_reg(
                    modrm.rm + if prefix.rm_high { 16 } else { 0 },
                    if scalar { VecWidth::V128 } else { width },
                ),
                Vec::new(),
            );
        }

        let broadcast = !scalar && prefix.b;
        let scale = if scalar || broadcast {
            elem.bytes()
        } else {
            width.bytes()
        };
        let (addr, mut ops) =
            self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, scale, ctx);
        if scalar {
            let scalar_value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar_value,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            let mem_width = match elem {
                VecElementType::F16 => MemWidth::B2,
                VecElementType::F32 => MemWidth::B4,
                VecElementType::F64 => MemWidth::B8,
                _ => unreachable!("validated unary FP element"),
            };
            if let Some(mask_reg) = mask {
                let active = self.append_mask_bit_condition(Some(mask_reg), 0, pc, ctx, &mut ops);
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: scalar_value,
                        cond: active,
                        addr,
                        width: mem_width,
                        signed: SignExtend::Zero,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar_value,
                        addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            let source = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: source,
                    scalar: scalar_value,
                    elem,
                    lanes: 1,
                },
            ));
            return (source, ops);
        }

        let source = if broadcast {
            if let Some(mask_reg) = mask {
                self.append_masked_broadcast_memory_source(
                    addr, elem, width, mask_reg, pc, ctx, &mut ops,
                )
            } else {
                self.append_broadcast_memory_source(addr, elem, width, pc, ctx, &mut ops)
            }
        } else if let Some(mask_reg) = mask {
            self.append_evex_masked_vector_source(
                addr, elem, width, false, mask_reg, pc, ctx, &mut ops,
            )
        } else {
            let source = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: source,
                    addr,
                    width,
                },
            ));
            source
        };
        (source, ops)
    }

    /// Materialize the exact FP16 tuple used by packed FP16-to-integer
    /// conversions. Quarter tuples can be only four bytes wide, so the lane
    /// count is explicit instead of being inferred from the minimum V64
    /// architectural register region. Full tuples use per-lane loads for EVEX
    /// fault suppression without over-reading a four-byte memory operand;
    /// broadcasts aggregate the opmask and perform at most one scalar read.
    pub(crate) fn append_evex_fp16_to_int_source(
        &self,
        addr: Address,
        lanes: u8,
        src_width: VecWidth,
        broadcast: bool,
        mask: Option<VReg>,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        if broadcast {
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
            if let Some(mask) = mask {
                let active = ctx.alloc_vreg();
                let lane_mask = if lanes == 64 {
                    u64::MAX
                } else {
                    (1u64 << lanes) - 1
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: active,
                        src1: mask,
                        src2: SrcOperand::Imm(lane_mask as i64),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: scalar,
                        cond: active,
                        addr,
                        width: MemWidth::B2,
                        signed: SignExtend::Zero,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B2,
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
                    scalar,
                    elem: VecElementType::F16,
                    lanes,
                },
            ));
            return loaded;
        }

        let loaded = self.append_zero_vector(src_width, VecElementType::F16, pc, ctx, ops);
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        for lane in 0..lanes {
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
            let lane_addr = Address::base_off(base, i64::from(lane) * 2);
            if let Some(mask) = mask {
                let active = self.append_mask_bit_condition(Some(mask), lane, pc, ctx, ops);
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: scalar,
                        cond: active,
                        addr: lane_addr,
                        width: MemWidth::B2,
                        signed: SignExtend::Zero,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr: lane_addr,
                        width: MemWidth::B2,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: loaded,
                    vec: loaded,
                    scalar,
                    lane,
                    elem: VecElementType::F16,
                },
            ));
        }
        loaded
    }

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
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst: reg,
                    src: raw,
                    width: prefix.width,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_mask_vector_convert(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mask_to_vector = matches!(opcode, 0x28 | 0x38);
        let elem = match (opcode, prefix.w) {
            (0x28 | 0x29, false) => VecElementType::I8,
            (0x28 | 0x29, true) => VecElementType::I16,
            (0x38 | 0x39, false) => VecElementType::I32,
            (0x38 | 0x39, true) => VecElementType::I64,
            _ => unreachable!(),
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::Rep
            || prefix.l_bits == 3
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || prefix.vvvv != 0
            || prefix.v_high
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
        if modrm.is_memory || (!mask_to_vector && (modrm.reg >= 8 || prefix.reg_high)) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let lanes = prefix.width.lanes(elem) as u8;
        let bits = elem.bytes() * 8;
        let mut ops = Vec::new();
        if mask_to_vector {
            // EVEX.X/B are ignored for a ModR/M.r/m K-register operand.
            let src = VReg::Arch(ArchReg::X86(X86Reg::K(modrm.rm & 0x07)));
            let dst = self.vec_reg(
                modrm.reg + if prefix.reg_high { 16 } else { 0 },
                prefix.width,
            );
            let result = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
            for lane in 0..lanes {
                let shifted = ctx.alloc_vreg();
                let bit = ctx.alloc_vreg();
                let expanded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: shifted,
                        src,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: bit,
                        src1: shifted,
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Neg {
                        dst: expanded,
                        src: bit,
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: result,
                        vec: result,
                        scalar: expanded,
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
                    src: result,
                    width: prefix.width,
                },
            ));
        } else {
            let src = self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width);
            let dst = VReg::Arch(ArchReg::X86(X86Reg::K(modrm.reg)));
            let mut result = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::Mov {
                    dst: result,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            for lane in 0..lanes {
                let scalar = ctx.alloc_vreg();
                let bit = ctx.alloc_vreg();
                let positioned = ctx.alloc_vreg();
                let combined = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: src,
                        lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: bit,
                        src: scalar,
                        amount: SrcOperand::Imm(i64::from(bits - 1)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shl {
                        dst: positioned,
                        src: bit,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Or {
                        dst: combined,
                        src1: result,
                        src2: SrcOperand::Reg(positioned),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                result = combined;
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(result),
                    width: OpWidth::W64,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_four_fma(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = matches!(opcode, 0x9B | 0xAB);
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::Repne
            || prefix.w
            || prefix.b
            || prefix.l_bits == 3
            || (!scalar && prefix.width != VecWidth::V512)
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

        let bytes_consumed = cursor + modrm.bytes_consumed;
        let next_pc = pc + bytes_consumed as u64;
        let (addr, mut ops) =
            self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 16, ctx);
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let memory = self.append_evex_whole_tuple_128(
            addr,
            mask,
            if scalar { 1 } else { 0xFFFF },
            pc,
            ctx,
            &mut ops,
        );

        let register_width = if scalar {
            VecWidth::V128
        } else {
            VecWidth::V512
        };
        let source_index = prefix.vvvv + if prefix.v_high { 16 } else { 0 };
        let source_base = source_index & !3;
        let source = |offset| self.vec_reg(source_base + offset, register_width);
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            register_width,
        );
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FourFma {
                dst,
                src0: source(0),
                src1: source(1),
                src2: source(2),
                src3: source(3),
                mem: memory,
                mask,
                scalar,
                negate_product: matches!(opcode, 0xAA | 0xAB),
                mask_zeroing: prefix.zeroing,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }

    pub(crate) fn append_evex_fp_class_vector(
        &self,
        src: VReg,
        elem: VecElementType,
        width: VecWidth,
        lanes: u8,
        imm: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let (bit_elem, exponent_mask, mantissa_mask, sign_mask, quiet_mask) = match elem {
            VecElementType::F16 => (VecElementType::I16, 0x7C00, 0x03FF, 0x8000, 0x0200),
            VecElementType::F32 => (
                VecElementType::I32,
                0x7F80_0000,
                0x007F_FFFF,
                0x8000_0000,
                0x0040_0000,
            ),
            VecElementType::F64 => (
                VecElementType::I64,
                0x7FF0_0000_0000_0000,
                0x000F_FFFF_FFFF_FFFF,
                0x8000_0000_0000_0000,
                0x0008_0000_0000_0000,
            ),
            _ => unreachable!(),
        };
        let zero = self.append_zero_vector(width, bit_elem, pc, ctx, ops);
        let exponent_constant =
            self.append_vector_splat_imm(exponent_mask, width, bit_elem, pc, ctx, ops);
        let mantissa_constant =
            self.append_vector_splat_imm(mantissa_mask, width, bit_elem, pc, ctx, ops);
        let sign_constant = self.append_vector_splat_imm(sign_mask, width, bit_elem, pc, ctx, ops);
        let quiet_constant =
            self.append_vector_splat_imm(quiet_mask, width, bit_elem, pc, ctx, ops);

        let exponent = self.append_vector_and(src, exponent_constant, width, pc, ctx, ops);
        let mantissa = self.append_vector_and(src, mantissa_constant, width, pc, ctx, ops);
        let sign_bits = self.append_vector_and(src, sign_constant, width, pc, ctx, ops);
        let quiet_bits = self.append_vector_and(src, quiet_constant, width, pc, ctx, ops);

        let exponent_all_ones = self.append_vector_compare(
            exponent,
            exponent_constant,
            VecCmpCond::Eq,
            bit_elem,
            lanes,
            pc,
            ctx,
            ops,
        );
        let exponent_all_zeros = self.append_vector_compare(
            exponent,
            zero,
            VecCmpCond::Eq,
            bit_elem,
            lanes,
            pc,
            ctx,
            ops,
        );
        let mantissa_all_zeros = self.append_vector_compare(
            mantissa,
            zero,
            VecCmpCond::Eq,
            bit_elem,
            lanes,
            pc,
            ctx,
            ops,
        );
        let negative = self.append_vector_compare(
            sign_bits,
            zero,
            VecCmpCond::Ne,
            bit_elem,
            lanes,
            pc,
            ctx,
            ops,
        );
        let quiet = self.append_vector_compare(
            quiet_bits,
            zero,
            VecCmpCond::Ne,
            bit_elem,
            lanes,
            pc,
            ctx,
            ops,
        );

        let zero_number = if elem == VecElementType::F16 {
            // FP16 classification is a raw exponent/mantissa test and does not
            // apply MXCSR.DAZ.
            self.append_vector_and(exponent_all_zeros, mantissa_all_zeros, width, pc, ctx, ops)
        } else {
            // Ordered equality against zero supplies the SDM's DAZ-aware
            // ZeroNumber predicate for binary32/binary64. SAE is required
            // because FPCLASS has no SIMD exceptions or MXCSR status effects.
            let zero_number = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86VectorFpCompare {
                    dst: zero_number,
                    src1: src,
                    src2: zero,
                    mask: None,
                    elem,
                    width,
                    lanes,
                    predicate: 0,
                    scalar: false,
                    mask_destination: false,
                    zero_upper: false,
                    suppress_exceptions: true,
                },
            ));
            zero_number
        };

        let nan =
            self.append_vector_and_not(mantissa_all_zeros, exponent_all_ones, width, pc, ctx, ops);
        let qnan = self.append_vector_and(nan, quiet, width, pc, ctx, ops);
        let snan = self.append_vector_and_not(quiet, nan, width, pc, ctx, ops);
        let positive_zero = self.append_vector_and_not(negative, zero_number, width, pc, ctx, ops);
        let negative_zero = self.append_vector_and(negative, zero_number, width, pc, ctx, ops);
        let infinity =
            self.append_vector_and(exponent_all_ones, mantissa_all_zeros, width, pc, ctx, ops);
        let positive_infinity = self.append_vector_and_not(negative, infinity, width, pc, ctx, ops);
        let negative_infinity = self.append_vector_and(negative, infinity, width, pc, ctx, ops);
        let denormal =
            self.append_vector_and_not(zero_number, exponent_all_zeros, width, pc, ctx, ops);
        let negative_non_infinite =
            self.append_vector_and_not(exponent_all_ones, negative, width, pc, ctx, ops);
        let negative_finite =
            self.append_vector_and_not(zero_number, negative_non_infinite, width, pc, ctx, ops);
        let classes = [
            qnan,
            positive_zero,
            negative_zero,
            positive_infinity,
            negative_infinity,
            denormal,
            negative_finite,
            snan,
        ];
        let mut result = zero;
        for (bit, class) in classes.into_iter().enumerate() {
            if imm & (1 << bit) != 0 {
                result = self.append_vector_or(result, class, width, pc, ctx, ops);
            }
        }
        result
    }

    pub(crate) fn lift_evex_get_exponent(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x43;
        if prefix.encoding != VecEncodingKind::Evex
            || !matches!(prefix.map, X86VecMap::Map0F38 | X86VecMap::Map6)
            || prefix.pp != X86SsePrefix::OpSize
            || (prefix.map == X86VecMap::Map6 && prefix.w)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = match (prefix.map, prefix.w) {
            (X86VecMap::Map6, false) => VecElementType::F16,
            (X86VecMap::Map0F38, false) => VecElementType::F32,
            (X86VecMap::Map0F38, true) => VecElementType::F64,
            _ => unreachable!("validated VGETEXP encoding"),
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
        let embedded_sae = prefix.b && !modrm.is_memory;
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_sae && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_sae {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            if scalar { VecWidth::V128 } else { width },
        );
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86GetExponent {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_round_scale(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = matches!(opcode, 0x0A | 0x0B);
        let elem = match (opcode, prefix.pp, prefix.w) {
            (0x08, X86SsePrefix::None, false) | (0x0A, X86SsePrefix::None, false) => {
                VecElementType::F16
            }
            (0x08, X86SsePrefix::OpSize, false) | (0x0A, X86SsePrefix::OpSize, false) => {
                VecElementType::F32
            }
            (0x09, X86SsePrefix::OpSize, true) | (0x0B, X86SsePrefix::OpSize, true) => {
                VecElementType::F64
            }
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F3A
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
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
        let embedded_sae = prefix.b && !modrm.is_memory;
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_sae && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_sae {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + imm_offset as u64 + 1;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            if scalar { VecWidth::V128 } else { width },
        );
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86RoundScale {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_evex_reduce(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x57;
        let elem = match (opcode, prefix.pp, prefix.w) {
            (0x56 | 0x57, X86SsePrefix::None, false) => VecElementType::F16,
            (0x56 | 0x57, X86SsePrefix::OpSize, false) => VecElementType::F32,
            (0x56 | 0x57, X86SsePrefix::OpSize, true) => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F3A
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
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
        let embedded_sae = prefix.b && !modrm.is_memory;
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_sae && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_sae {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + imm_offset as u64 + 1;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            if scalar { VecWidth::V128 } else { width },
        );
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Reduce {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_evex_scale_f(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x2D;
        if prefix.encoding != VecEncodingKind::Evex
            || !matches!(prefix.map, X86VecMap::Map0F38 | X86VecMap::Map6)
            || prefix.pp != X86SsePrefix::OpSize
            || (prefix.map == X86VecMap::Map6 && prefix.w)
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match (prefix.map, prefix.w) {
            (X86VecMap::Map6, false) => VecElementType::F16,
            (X86VecMap::Map0F38, false) => VecElementType::F32,
            (X86VecMap::Map0F38, true) => VecElementType::F64,
            _ => unreachable!("validated VSCALEF encoding"),
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
        let embedded_rounding = prefix.b && !modrm.is_memory;
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_rounding && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let width = if scalar {
            VecWidth::V128
        } else if embedded_rounding {
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
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src2, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let register_width = if scalar { VecWidth::V128 } else { width };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            register_width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            register_width,
        );
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86ScaleF {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
                round,
                suppress_exceptions: embedded_rounding,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_range(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x51;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F3A
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0x50 | 0x51)
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
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
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&imm) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };
        let embedded_sae = prefix.b && !modrm.is_memory;
        if imm & 0xF0 != 0
            || (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_sae && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_sae {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + imm_offset as u64 + 1;
        let (src2, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let register_width = if scalar { VecWidth::V128 } else { width };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            register_width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            register_width,
        );
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Range {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_evex_fixup_imm(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x55;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F3A
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0x54 | 0x55)
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
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
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&imm) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };

        // Packed register EVEX.b is SAE and fixes the vector length at 512
        // bits. Packed memory EVEX.b is broadcast. Scalar EVEX.b is SAE for
        // both register and scalar-memory encodings.
        let embedded_sae = prefix.b && (scalar || !modrm.is_memory);
        if !scalar && !embedded_sae && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_sae {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + imm_offset as u64 + 1;
        let (src2, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let register_width = if scalar { VecWidth::V128 } else { width };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            register_width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            register_width,
        );
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FixupImm {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_evex_exp2(
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
            || opcode != 0xC8
            || prefix.vvvv != 0
            || prefix.v_high
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
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
        let embedded_sae = prefix.b && !modrm.is_memory;
        // EVEX.b selects SAE for a register source and scalar broadcast for a
        // memory source. Without register SAE, VEXP2 is strictly EVEX.512;
        // under SAE, EVEX.L'L is ignored by the architectural encoding.
        if !embedded_sae && prefix.l_bits != 2 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = VecWidth::V512;
        let lanes = width.lanes(elem) as u8;
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, false, mask, pc, ctx,
        );
        let dst = self.zmm(modrm.reg + if prefix.reg_high { 16 } else { 0 });
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Exp2 {
                dst,
                src,
                mask,
                elem,
                width,
                lanes,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_approx14(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode & 1 != 0;
        let rsqrt = opcode >= 0x4E;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0x4C..=0x4F)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
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
        // EVEX.b selects scalar memory broadcast for packed forms. Register
        // sources and all scalar forms reserve EVEX.b; packed L'L=3 is also
        // reserved, while scalar L'L is ignored.
        if (prefix.b && (!modrm.is_memory || scalar)) || (!scalar && prefix.l_bits == 3) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        let kind = if rsqrt {
            OpKind::X86Rsqrt14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        } else {
            OpKind::X86Recip14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_fp16_approx(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode & 1 != 0;
        let rsqrt = opcode >= 0x4E;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map6
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || !matches!(opcode, 0x4C..=0x4F)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
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
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        // Packed memory forms use EVEX.b for m16 broadcast. Register sources
        // and both scalar forms reserve EVEX.b. Packed L'L=3 is reserved;
        // scalar L'L is ignored by the encoding.
        if (prefix.b && (!modrm.is_memory || scalar)) || (!scalar && prefix.l_bits == 3) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let lanes = if scalar {
            1
        } else {
            width.lanes(VecElementType::F16) as u8
        };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix,
            &modrm,
            next_pc,
            VecElementType::F16,
            width,
            scalar,
            mask,
            pc,
            ctx,
        );
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        let kind = if rsqrt {
            OpKind::X86RsqrtFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        } else {
            OpKind::X86RecipFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
            }
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: false,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_approx28(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode & 1 != 0;
        let rsqrt = opcode >= 0xCC;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0xCA..=0xCD)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let elem = if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
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
        // Packed register EVEX.b is SAE and makes L'L ignored; packed memory
        // EVEX.b is broadcast and remains strictly 512-bit. Scalar EVEX.b is
        // SAE for either register or memory sources and L'L is ignored.
        let embedded_sae = prefix.b && (scalar || !modrm.is_memory);
        if !scalar && !embedded_sae && prefix.l_bits != 2 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else {
            VecWidth::V512
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (src, mut ops) = self.materialize_evex_unary_fp_source(
            prefix, &modrm, next_pc, elem, width, scalar, mask, pc, ctx,
        );
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let merge = scalar.then(|| self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }));
        let kind = if rsqrt {
            OpKind::X86Rsqrt28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            }
        } else {
            OpKind::X86Recip28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing: prefix.zeroing,
                suppress_exceptions: embedded_sae,
            }
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_fp16_complex(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode & 1 != 0;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map6
            || !matches!(prefix.pp, X86SsePrefix::Rep | X86SsePrefix::Repne)
            || prefix.w
            || !matches!(opcode, 0x56 | 0x57 | 0xD6 | 0xD7)
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
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let embedded_rounding = prefix.b && !modrm.is_memory;
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_rounding && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_rounding {
            VecWidth::V512
        } else {
            prefix.width
        };
        let register_width = if scalar { VecWidth::V128 } else { width };
        let dst_index = modrm.reg + if prefix.reg_high { 16 } else { 0 };
        let src1_index = prefix.vvvv + if prefix.v_high { 16 } else { 0 };
        let src2_index = modrm.rm + if prefix.rm_high { 16 } else { 0 };
        if dst_index == src1_index || (!modrm.is_memory && dst_index == src2_index) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

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
        let pairs = if scalar { 1 } else { (width.bytes() / 4) as u8 };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if !modrm.is_memory {
            self.vec_reg(src2_index, register_width)
        } else {
            let broadcast = !scalar && prefix.b;
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if scalar || broadcast {
                    4
                } else {
                    width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            if scalar {
                let pair = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: pair,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                if let Some(mask_reg) = mask {
                    let active =
                        self.append_mask_bit_condition(Some(mask_reg), 0, pc, ctx, &mut ops);
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: pair,
                            cond: active,
                            addr,
                            width: MemWidth::B4,
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: pair,
                            addr,
                            width: MemWidth::B4,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                let source = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: source,
                        scalar: pair,
                        elem: VecElementType::I32,
                        lanes: 1,
                    },
                ));
                source
            } else if broadcast {
                if let Some(mask_reg) = mask {
                    self.append_masked_broadcast_memory_source(
                        addr,
                        VecElementType::I32,
                        width,
                        mask_reg,
                        pc,
                        ctx,
                        &mut ops,
                    )
                } else {
                    self.append_broadcast_memory_source(
                        addr,
                        VecElementType::I32,
                        width,
                        pc,
                        ctx,
                        &mut ops,
                    )
                }
            } else if let Some(mask_reg) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I32,
                    width,
                    false,
                    mask_reg,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let source = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: source,
                        addr,
                        width,
                    },
                ));
                source
            }
        };

        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FP16Complex {
                dst: self.vec_reg(dst_index, register_width),
                src1: self.vec_reg(src1_index, register_width),
                src2,
                mask,
                width,
                pairs,
                scalar,
                mask_zeroing: prefix.zeroing,
                accumulate: opcode & 0x80 == 0,
                conjugate: prefix.pp == X86SsePrefix::Repne,
                round,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_fp_class(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x67;
        if prefix.encoding != VecEncodingKind::Evex
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.zeroing
            || (prefix.pp == X86SsePrefix::None && prefix.w)
            || (!scalar && prefix.l_bits == 3)
            || (scalar && prefix.b)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match (prefix.pp, prefix.w) {
            (X86SsePrefix::None, false) => VecElementType::F16,
            (X86SsePrefix::OpSize, false) => VecElementType::F32,
            (X86SsePrefix::OpSize, true) => VecElementType::F64,
            _ => unreachable!(),
        };
        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
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
        if modrm.reg >= 8 || prefix.reg_high || (prefix.b && !modrm.is_memory) {
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
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast = !scalar && prefix.b && modrm.is_memory;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let scale = if scalar || broadcast {
                elem.bytes()
            } else {
                width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                scale,
                ctx,
            );
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
                    let active =
                        self.append_mask_bit_condition(Some(mask_reg), 0, pc, ctx, &mut ops);
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
                                VecElementType::F64 => MemWidth::B8,
                                _ => unreachable!(),
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
                                VecElementType::F64 => MemWidth::B8,
                                _ => unreachable!(),
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
                self.append_evex_masked_vector_source(
                    addr, elem, width, broadcast, mask_reg, pc, ctx, &mut ops,
                )
            } else if broadcast {
                self.append_broadcast_memory_source(addr, elem, width, pc, ctx, &mut ops)
            } else {
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
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, width)
        };
        let classified = self.append_evex_fp_class_vector(
            src,
            elem,
            width,
            lanes,
            bytes[imm_offset],
            pc,
            ctx,
            &mut ops,
        );
        let raw_mask = ctx.alloc_vreg();
        self.append_sse_movmask(
            raw_mask,
            classified,
            elem,
            lanes,
            OpWidth::W64,
            pc,
            ctx,
            &mut ops,
        );
        let dst = VReg::Arch(ArchReg::X86(X86Reg::K(modrm.reg)));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            if let Some(mask_reg) = mask {
                OpKind::And {
                    dst,
                    src1: raw_mask,
                    src2: SrcOperand::Reg(mask_reg),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                }
            } else {
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(raw_mask),
                    width: OpWidth::W64,
                }
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_evex_fp16_to_int(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.pp != X86SsePrefix::Rep
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.reg_high
            || prefix.aaa != 0
            || prefix.zeroing
            || !matches!(opcode, 0x2C | 0x2D | 0x78 | 0x79)
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
        if prefix.b && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                VecElementType::F16,
                ctx,
            );
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: scalar,
                    addr,
                    width: MemWidth::B2,
                    sign: SignExtend::Zero,
                },
            ));
            scalar
        } else {
            self.xmm(modrm.rm + if prefix.rm_high { 16 } else { 0 })
        };
        let signed = matches!(opcode, 0x2C | 0x2D);
        let truncate = matches!(opcode, 0x2C | 0x78);
        let round = if truncate {
            FpRoundMode::RoundTowardZero
        } else if prefix.b {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpToInt {
                dst: self.gpr(modrm.reg),
                src,
                elem: VecElementType::F16,
                int_width: if prefix.w { OpWidth::W64 } else { OpWidth::W32 },
                signed,
                truncate,
                round,
                suppress_exceptions: prefix.b,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_int_to_fp16(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.pp != X86SsePrefix::Rep
            || prefix.aaa != 0
            || prefix.zeroing
            || !matches!(opcode, 0x2A | 0x7B)
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
        if (prefix.b && modrm.is_memory) || (!modrm.is_memory && prefix.rm_high) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let signed = opcode == 0x2A;
        let int_width = if prefix.w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                mem_width.bytes(),
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: value,
                    addr,
                    width: mem_width,
                    sign: if signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                },
            ));
            value
        } else {
            self.gpr(modrm.rm)
        };
        let round = if prefix.b {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86IntToFp {
                dst: self.xmm(modrm.reg + if prefix.reg_high { 16 } else { 0 }),
                merge: self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 }),
                src,
                elem: VecElementType::F16,
                int_width,
                signed,
                round,
                suppress_exceptions: prefix.b,
                zero_upper: true,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_fp16_scalar_move(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.pp != X86SsePrefix::Rep
            || prefix.w
            || prefix.b
            || !matches!(opcode, 0x10 | 0x11)
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
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let reg_index = modrm.reg + if prefix.reg_high { 16 } else { 0 };
        let rm_index = modrm.rm + if prefix.rm_high { 16 } else { 0 };
        let merge_index = prefix.vvvv + if prefix.v_high { 16 } else { 0 };
        let mut ops = Vec::new();
        let mask_cond = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);

        if modrm.is_memory {
            if prefix.vvvv != 0 || prefix.v_high || (opcode == 0x11 && prefix.zeroing) {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                VecElementType::F16,
                ctx,
            );
            ops.extend(pre_ops);
            if opcode == 0x10 {
                let scalar = ctx.alloc_vreg();
                if let Some(cond) = mask_cond {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W16,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond,
                            addr,
                            width: MemWidth::B2,
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar,
                            addr,
                            width: MemWidth::B2,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                let dst = self.xmm(reg_index);
                let scalar = self.append_evex_scalar_select(
                    prefix,
                    mask_cond,
                    dst,
                    scalar,
                    VecElementType::F16,
                    pc,
                    ctx,
                    &mut ops,
                );
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst,
                        scalar,
                        elem: VecElementType::F16,
                        lanes: 1,
                    },
                ));
            } else {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: self.xmm(reg_index),
                        lane: 0,
                        elem: VecElementType::F16,
                        sign: SignExtend::Zero,
                    },
                ));
                if let Some(cond) = mask_cond {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredStore {
                            src: SrcOperand::Reg(scalar),
                            cond,
                            addr,
                            width: MemWidth::B2,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Store {
                            src: scalar,
                            addr,
                            width: MemWidth::B2,
                        },
                    ));
                }
            }
        } else {
            let (dst, low_src) = if opcode == 0x10 {
                (self.xmm(reg_index), self.xmm(rm_index))
            } else {
                (self.xmm(rm_index), self.xmm(reg_index))
            };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: low_src,
                    lane: 0,
                    elem: VecElementType::F16,
                    sign: SignExtend::Zero,
                },
            ));
            let scalar = self.append_evex_scalar_select(
                prefix,
                mask_cond,
                dst,
                scalar,
                VecElementType::F16,
                pc,
                ctx,
                &mut ops,
            );
            self.append_vex_scalar_result(
                dst,
                self.xmm(merge_index),
                scalar,
                VecElementType::F16,
                pc,
                ctx,
                &mut ops,
            );
        }

        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_fp16_scalar_arithmetic(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.pp != X86SsePrefix::Rep
            || prefix.w
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let op = match opcode {
            0x58 => Avx10FP16Op::Add,
            0x59 => Avx10FP16Op::Mul,
            0x5C => Avx10FP16Op::Sub,
            0x5D => Avx10FP16Op::Min,
            0x5E => Avx10FP16Op::Div,
            0x5F => Avx10FP16Op::Max,
            _ => unreachable!("MAP5 scalar FP16 arithmetic dispatch filtered opcode"),
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
        if prefix.b && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let round = if prefix.b {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let dst = self.xmm(modrm.reg + if prefix.reg_high { 16 } else { 0 });
        let merge = self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 });
        let mut ops = Vec::new();
        let mask_cond = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);

        let first_raw = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: first_raw,
                vec: merge,
                lane: 0,
                elem: VecElementType::F16,
                sign: SignExtend::Zero,
            },
        ));
        let first = if let Some(cond) = mask_cond {
            let zero = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: zero,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W16,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: selected,
                    cond,
                    src_true: first_raw,
                    src_false: zero,
                    width: OpWidth::W16,
                },
            ));
            selected
        } else {
            first_raw
        };

        // Inactive DIV lanes use 0/1 rather than 0/0 so a future MXCSR-aware
        // interpreter cannot report an exception for a masked-off operation.
        let inactive_second = if op == Avx10FP16Op::Div { 0x3C00 } else { 0 };
        let second = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                VecElementType::F16,
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            if let Some(cond) = mask_cond {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: value,
                        src: SrcOperand::Imm(inactive_second),
                        width: OpWidth::W16,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: value,
                        cond,
                        addr,
                        width: MemWidth::B2,
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
                        width: MemWidth::B2,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            value
        } else {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: raw,
                    vec: self.xmm(modrm.rm + if prefix.rm_high { 16 } else { 0 }),
                    lane: 0,
                    elem: VecElementType::F16,
                    sign: SignExtend::Zero,
                },
            ));
            if let Some(cond) = mask_cond {
                let inactive = ctx.alloc_vreg();
                let selected = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: inactive,
                        src: SrcOperand::Imm(inactive_second),
                        width: OpWidth::W16,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Select {
                        dst: selected,
                        cond,
                        src_true: raw,
                        src_false: inactive,
                        width: OpWidth::W16,
                    },
                ));
                selected
            } else {
                raw
            }
        };

        let first_vec = ctx.alloc_vreg();
        let second_vec = ctx.alloc_vreg();
        for (dst, scalar) in [(first_vec, first), (second_vec, second)] {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst,
                    scalar,
                    elem: VecElementType::F16,
                    lanes: 1,
                },
            ));
        }
        let raw = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VFP16Arith {
                dst: raw,
                src1: first_vec,
                src2: second_vec,
                mask: None,
                op,
                round,
                width: VecWidth::V128,
                lanes: 1,
                zeroing: false,
            },
        ));
        let low = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: low,
                vec: raw,
                lane: 0,
                elem: VecElementType::F16,
                sign: SignExtend::Zero,
            },
        ));
        let low = self.append_evex_scalar_select(
            prefix,
            mask_cond,
            dst,
            low,
            VecElementType::F16,
            pc,
            ctx,
            &mut ops,
        );
        self.append_vex_scalar_result(dst, merge, low, VecElementType::F16, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_fp16_sqrt(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = prefix.pp == X86SsePrefix::Rep;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::Rep)
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
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar
                && (prefix.vvvv != 0
                    || prefix.v_high
                    || (!embedded_rounding && prefix.l_bits == 3)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let width = if scalar {
            VecWidth::V128
        } else if embedded_rounding {
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
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let mut ops = Vec::new();

        if scalar {
            let dst = self.xmm(modrm.reg + if prefix.reg_high { 16 } else { 0 });
            let merge = self.xmm(prefix.vvvv + if prefix.v_high { 16 } else { 0 });
            let mask_cond = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);
            let source_scalar = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    VecElementType::F16,
                    ctx,
                );
                ops.extend(pre_ops);
                let value = ctx.alloc_vreg();
                if let Some(cond) = mask_cond {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: value,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W16,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: value,
                            cond,
                            addr,
                            width: MemWidth::B2,
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
                            width: MemWidth::B2,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                value
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: value,
                        vec: self.xmm(modrm.rm + if prefix.rm_high { 16 } else { 0 }),
                        lane: 0,
                        elem: VecElementType::F16,
                        sign: SignExtend::Zero,
                    },
                ));
                if let Some(cond) = mask_cond {
                    let zero = ctx.alloc_vreg();
                    let selected = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: zero,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W16,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Select {
                            dst: selected,
                            cond,
                            src_true: value,
                            src_false: zero,
                            width: OpWidth::W16,
                        },
                    ));
                    selected
                } else {
                    value
                }
            };
            let source = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: source,
                    scalar: source_scalar,
                    elem: VecElementType::F16,
                    lanes: 1,
                },
            ));
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VFP16Arith {
                    dst: raw,
                    src1: source,
                    src2: source,
                    mask: None,
                    op: Avx10FP16Op::Sqrt,
                    round,
                    width,
                    lanes: 1,
                    zeroing: false,
                },
            ));
            let low = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: low,
                    vec: raw,
                    lane: 0,
                    elem: VecElementType::F16,
                    sign: SignExtend::Zero,
                },
            ));
            let low = self.append_evex_scalar_select(
                prefix,
                mask_cond,
                dst,
                low,
                VecElementType::F16,
                pc,
                ctx,
                &mut ops,
            );
            self.append_vex_scalar_result(dst, merge, low, VecElementType::F16, pc, ctx, &mut ops);
            return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
        }

        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, width);
        let broadcast = prefix.b && modrm.is_memory;
        let source = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast { 2 } else { width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                if broadcast {
                    let lanes = width.lanes(VecElementType::F16) as u8;
                    let active = ctx.alloc_vreg();
                    let scalar = ctx.alloc_vreg();
                    let source = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: active,
                            src1: mask,
                            src2: SrcOperand::Imm((1i64 << lanes) - 1),
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
                            width: OpWidth::W16,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond: active,
                            addr,
                            width: MemWidth::B2,
                            signed: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VBroadcast {
                            dst: source,
                            scalar,
                            elem: VecElementType::F16,
                            lanes,
                        },
                    ));
                    source
                } else {
                    self.append_evex_masked_vector_source(
                        addr,
                        VecElementType::F16,
                        width,
                        false,
                        mask,
                        pc,
                        ctx,
                        &mut ops,
                    )
                }
            } else if broadcast {
                self.append_broadcast_memory_source(
                    addr,
                    VecElementType::F16,
                    width,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let source = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: source,
                        addr,
                        width,
                    },
                ));
                source
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, width)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VFP16Arith {
                dst,
                src1: source,
                src2: source,
                mask,
                op: Avx10FP16Op::Sqrt,
                round,
                width,
                lanes: width.lanes(VecElementType::F16) as u8,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_packed_int_to_fp16(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let opcode = bytes[prefix.bytes];
        let (int_elem, signed) = match (opcode, prefix.pp, prefix.w) {
            (0x5B, X86SsePrefix::None, false) => (VecElementType::I32, true),
            (0x5B, X86SsePrefix::None, true) => (VecElementType::I64, true),
            (0x7A, X86SsePrefix::Repne, false) => (VecElementType::I32, false),
            (0x7A, X86SsePrefix::Repne, true) => (VecElementType::I64, false),
            (0x7D, X86SsePrefix::Rep, false) => (VecElementType::I16, true),
            (0x7D, X86SsePrefix::Repne, false) => (VecElementType::I16, false),
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
        let embedded_rounding = prefix.b && !modrm.is_memory;
        if !embedded_rounding && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let src_width = if embedded_rounding {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = src_width.lanes(int_elem) as u8;
        let dst_bytes = u32::from(lanes) * VecElementType::F16.bytes();
        let dst_width = match dst_bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast = prefix.b && modrm.is_memory;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    int_elem.bytes()
                } else {
                    src_width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask_reg) = mask {
                self.append_evex_masked_vector_source(
                    addr, int_elem, src_width, broadcast, mask_reg, pc, ctx, &mut ops,
                )
            } else if broadcast {
                self.append_broadcast_memory_source(addr, int_elem, src_width, pc, ctx, &mut ops)
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: src_width,
                    },
                ));
                value
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, src_width)
        };
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, dst_width);
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
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedIntToFp16 {
                dst,
                src,
                mask,
                int_elem,
                signed,
                lanes,
                src_width,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: embedded_rounding,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: src_width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_evex_packed_fp16_to_int(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let opcode = bytes[prefix.bytes];
        let (int_elem, signed, truncate) = match (opcode, prefix.pp, prefix.w) {
            (0x5B, X86SsePrefix::OpSize, false) => (VecElementType::I32, true, false),
            (0x5B, X86SsePrefix::Rep, false) => (VecElementType::I32, true, true),
            (0x7B, X86SsePrefix::OpSize, false) => (VecElementType::I64, true, false),
            (0x7A, X86SsePrefix::OpSize, false) => (VecElementType::I64, true, true),
            (0x79, X86SsePrefix::None, false) => (VecElementType::I32, false, false),
            (0x78, X86SsePrefix::None, false) => (VecElementType::I32, false, true),
            (0x79, X86SsePrefix::OpSize, false) => (VecElementType::I64, false, false),
            (0x78, X86SsePrefix::OpSize, false) => (VecElementType::I64, false, true),
            (0x7D, X86SsePrefix::OpSize, false) => (VecElementType::I16, true, false),
            (0x7C, X86SsePrefix::OpSize, false) => (VecElementType::I16, true, true),
            (0x7D, X86SsePrefix::None, false) => (VecElementType::I16, false, false),
            (0x7C, X86SsePrefix::None, false) => (VecElementType::I16, false, true),
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
        if !embedded_control && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let dst_width = if embedded_control {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = dst_width.lanes(int_elem) as u8;
        let src_bytes = u32::from(lanes) * VecElementType::F16.bytes();
        let src_width = match src_bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast = prefix.b && modrm.is_memory;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast { 2 } else { src_bytes },
                ctx,
            );
            ops.extend(pre_ops);
            self.append_evex_fp16_to_int_source(
                addr, lanes, src_width, broadcast, mask, pc, ctx, &mut ops,
            )
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, src_width)
        };
        let dst = self.vec_reg(modrm.reg + if prefix.reg_high { 16 } else { 0 }, dst_width);
        let round = if truncate {
            FpRoundMode::RoundTowardZero
        } else if embedded_control {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedFp16ToInt {
                dst,
                src,
                mask,
                int_elem,
                signed,
                truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: embedded_control,
            },
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: dst_width,
                w: prefix.w,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
