//! EVEX-encoded AVX-512 / AVX10 instruction lifting

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


    /// Materialize an EVEX full-width memory source with one fault-suppressing
    /// load per destination element. For broadcasts, every active lane reads
    /// the same scalar address; otherwise lane `n` reads element `n`.
    pub(crate) fn append_evex_masked_vector_source(
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
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = match elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 | VecElementType::F16 => MemWidth::B2,
            VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
            VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
            _ => unreachable!(),
        };
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


    /// Store active EVEX destination elements in architectural lane order.
    /// Inactive lanes perform no memory access; a fault after earlier active
    /// lanes have committed preserves those earlier stores.
    pub(crate) fn append_evex_masked_vector_store(
        &self,
        addr: Address,
        src: VReg,
        elem: VecElementType,
        width: VecWidth,
        mask: VReg,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = match elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 | VecElementType::F16 => MemWidth::B2,
            VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
            VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
            _ => unreachable!(),
        };
        for lane in 0..width.lanes(elem) as u8 {
            let active = self.append_mask_bit_condition(Some(mask), lane, pc, ctx, ops);
            let scalar = ctx.alloc_vreg();
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
                OpKind::PredStore {
                    src: SrcOperand::Reg(scalar),
                    cond: active,
                    addr: Address::base_off(base, i64::from(lane) * i64::from(elem.bytes())),
                    width: mem_width,
                },
            ));
        }
    }


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


    pub(crate) fn append_evex_mask_condition(
        &self,
        prefix: VecPrefix,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> Option<VReg> {
        if prefix.encoding != VecEncodingKind::Evex || prefix.aaa == 0 {
            return None;
        }
        let cond = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::And {
                dst: cond,
                src1: VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        Some(cond)
    }


    pub(crate) fn append_evex_scalar_select(
        &self,
        prefix: VecPrefix,
        cond: Option<VReg>,
        dst: VReg,
        value: VReg,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let Some(cond) = cond else {
            return value;
        };
        let fallback = ctx.alloc_vreg();
        let width = match elem {
            VecElementType::F16 => OpWidth::W16,
            VecElementType::F32 => OpWidth::W32,
            VecElementType::F64 => OpWidth::W64,
            _ => unreachable!("EVEX scalar selection requires a floating-point element"),
        };
        if prefix.zeroing {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: fallback,
                    src: SrcOperand::Imm(0),
                    width,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: fallback,
                    vec: dst,
                    lane: 0,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
        }
        let selected = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Select {
                dst: selected,
                cond,
                src_true: value,
                src_false: fallback,
                width,
            },
        ));
        selected
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


    pub(crate) fn lift_evex_vpopcnt(
        &self,
        prefix: VecPrefix,
        opcode: u8,
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
        let elem = match (opcode, prefix.w) {
            (0x54, false) => VecElementType::I8,
            (0x54, true) => VecElementType::I16,
            (0x55, false) => VecElementType::I32,
            (0x55, true) => VecElementType::I64,
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
        let broadcast_allowed = opcode == 0x55;
        if prefix.b && (!modrm.is_memory || !broadcast_allowed) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let broadcast = prefix.b;
        let mut ops = Vec::new();
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src = if modrm.is_memory {
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
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPopcnt {
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


    pub(crate) fn lift_evex_vplzcnt(
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
                self.append_evex_masked_vector_source(
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
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VLeadingZeros {
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
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
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
                    VecElementType::F32,
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
                    VecElementType::F32,
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


    pub(crate) fn append_mask_bit_condition(
        &self,
        mask: Option<VReg>,
        lane: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        if let Some(mask) = mask {
            let shifted = ctx.alloc_vreg();
            let active = ctx.alloc_vreg();
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
            active
        } else {
            let active = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: active,
                    src: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                },
            ));
            active
        }
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


    pub(crate) fn lift_evex_integer_narrow(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mode = match opcode >> 4 {
            1 => X86NarrowMode::UnsignedSaturate,
            2 => X86NarrowMode::SignedSaturate,
            3 => X86NarrowMode::Truncate,
            _ => unreachable!(),
        };
        let (src_elem, dst_elem) = match opcode & 0x0F {
            0 => (VecElementType::I16, VecElementType::I8),
            1 => (VecElementType::I32, VecElementType::I8),
            2 => (VecElementType::I64, VecElementType::I8),
            3 => (VecElementType::I32, VecElementType::I16),
            4 => (VecElementType::I64, VecElementType::I16),
            5 => (VecElementType::I64, VecElementType::I32),
            _ => unreachable!(),
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::Rep
            || prefix.w
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
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if modrm.is_memory && prefix.zeroing {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let lanes = prefix.width.lanes(src_elem) as u8;
        let output_bytes = u32::from(lanes) * dst_elem.bytes();
        let output_width = if output_bytes <= 16 {
            VecWidth::V128
        } else {
            VecWidth::V256
        };
        let mut ops = Vec::new();
        if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                output_bytes,
                ctx,
            );
            ops.extend(pre_ops);
            let base = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Lea { dst: base, addr },
            ));
            let dst_bits = dst_elem.bytes() * 8;
            let dst_mask = (1u64 << dst_bits) - 1;
            for lane in 0..lanes {
                let active = self.append_mask_bit_condition(mask, lane, pc, ctx, &mut ops);
                let raw = ctx.alloc_vreg();
                let narrowed = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: raw,
                        vec: src,
                        lane,
                        elem: src_elem,
                        sign: SignExtend::Sign,
                    },
                ));
                match mode {
                    X86NarrowMode::Truncate => ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: narrowed,
                            src1: raw,
                            src2: SrcOperand::Imm(dst_mask as i64),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    )),
                    X86NarrowMode::SignedSaturate | X86NarrowMode::UnsignedSaturate => {
                        // Use a one-lane VNarrow operation so register and memory
                        // forms share exactly the same saturation semantics.
                        let wide =
                            self.append_zero_vector(VecWidth::V128, src_elem, pc, ctx, &mut ops);
                        let packed = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VInsertLane {
                                dst: wide,
                                vec: wide,
                                scalar: raw,
                                lane: 0,
                                elem: src_elem,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::X86NarrowInt {
                                dst: packed,
                                src: wide,
                                mask: None,
                                src_elem,
                                dst_elem,
                                width: match src_elem {
                                    VecElementType::I64 => VecWidth::V64,
                                    _ => VecWidth::V128,
                                },
                                mode,
                                zeroing: true,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VExtractLane {
                                dst: narrowed,
                                vec: packed,
                                lane: 0,
                                elem: dst_elem,
                                sign: SignExtend::Zero,
                            },
                        ));
                    }
                }
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredStore {
                        src: SrcOperand::Reg(narrowed),
                        cond: active,
                        addr: Address::base_off(
                            base,
                            i64::from(lane) * i64::from(dst_elem.bytes()),
                        ),
                        width: match dst_elem.bytes() {
                            1 => MemWidth::B1,
                            2 => MemWidth::B2,
                            4 => MemWidth::B4,
                            _ => unreachable!(),
                        },
                    },
                ));
            }
        } else {
            let dst = self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, output_width);
            ops.push(SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86NarrowInt {
                    dst,
                    src,
                    mask,
                    src_elem,
                    dst_elem,
                    width: prefix.width,
                    mode,
                    zeroing: prefix.zeroing,
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
        if modrm.is_memory
            || (mask_to_vector && (modrm.rm >= 8 || prefix.rm_high))
            || (!mask_to_vector && (modrm.reg >= 8 || prefix.reg_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let lanes = prefix.width.lanes(elem) as u8;
        let bits = elem.bytes() * 8;
        let mut ops = Vec::new();
        if mask_to_vector {
            let src = VReg::Arch(ArchReg::X86(X86Reg::K(modrm.rm)));
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


    /// Expand MASKMOVQ/MASKMOVDQU/VMASKMOVDQU into independently predicated
    /// byte stores. The predicate for byte `n` is bit 7 of mask byte `n`.
    /// A false predicate performs no memory access, matching the instruction's
    /// byte-selective store semantics and its permitted all-zero-mask behavior.
    pub(crate) fn append_maskmov(
        &self,
        data: VReg,
        mask: VReg,
        lanes: u8,
        address_size_override: bool,
        segment_override: Option<u8>,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let base = if address_size_override {
            let truncated = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: truncated,
                    src1: self.gpr(7),
                    src2: SrcOperand::Imm(0xFFFF_FFFF),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            truncated
        } else {
            self.gpr(7)
        };

        for lane in 0..lanes {
            let mask_byte = ctx.alloc_vreg();
            let active = ctx.alloc_vreg();
            let data_byte = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: mask_byte,
                    vec: mask,
                    lane,
                    elem: VecElementType::I8,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: active,
                    src: mask_byte,
                    amount: SrcOperand::Imm(7),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: data_byte,
                    vec: data,
                    lane,
                    elem: VecElementType::I8,
                    sign: SignExtend::Zero,
                },
            ));
            let disp = i64::from(lane);
            let addr = match segment_override {
                Some(0x64) => Address::SegmentRel {
                    segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                    base: Some(base),
                    index: None,
                    scale: 1,
                    disp,
                },
                Some(0x65) => Address::SegmentRel {
                    segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                    base: Some(base),
                    index: None,
                    scale: 1,
                    disp,
                },
                _ => Address::base_off(base, disp),
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::PredStore {
                    src: SrcOperand::Reg(data_byte),
                    cond: active,
                    addr,
                    width: MemWidth::B1,
                },
            ));
        }
    }


    pub(crate) fn append_vsib_lane_address(
        &self,
        x86_addr: &X86Address,
        index: VReg,
        lane: u8,
        index_elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> Address {
        let width = x86_addr.address_width;
        let mut offset = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: offset,
                vec: index,
                lane,
                elem: index_elem,
                sign: SignExtend::Sign,
            },
        ));
        if x86_addr.scale != 1 {
            let scaled = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shl {
                    dst: scaled,
                    src: offset,
                    amount: SrcOperand::Imm(i64::from(x86_addr.scale.trailing_zeros())),
                    width,
                    flags: FlagUpdate::None,
                },
            ));
            offset = scaled;
        }
        if let Some(base) = x86_addr.base {
            let sum = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Add {
                    dst: sum,
                    src1: self.gpr(base),
                    src2: SrcOperand::Reg(offset),
                    width,
                    flags: FlagUpdate::None,
                },
            ));
            offset = sum;
        }
        if x86_addr.disp != 0 {
            let sum = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Add {
                    dst: sum,
                    src1: offset,
                    src2: SrcOperand::Imm(x86_addr.disp),
                    width,
                    flags: FlagUpdate::None,
                },
            ));
            offset = sum;
        }
        match x86_addr.segment {
            Some(segment) => Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(segment)),
                base: Some(offset),
                index: None,
                scale: 1,
                disp: 0,
            },
            None => Address::Direct(offset),
        }
    }


    /// Lift the AVX-512PF sparse gather/scatter prefetch families. Intel
    /// defines each requested prefetch as an optional hint, leaves the opmask
    /// unchanged, and permits neither FP nor memory faults. Consequently an
    /// empty fallthrough is one architecturally valid implementation after
    /// the complete E12NP encoding boundary has been validated.
    pub(crate) fn lift_evex_sparse_prefetch(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        let fixed_zero = prefix
            .bytes
            .checked_sub(3)
            .and_then(|index| bytes.get(index))
            .is_some_and(|p0| p0 & 0x08 == 0);
        let fixed_one = prefix
            .bytes
            .checked_sub(2)
            .and_then(|index| bytes.get(index))
            .is_some_and(|p1| p1 & 0x04 != 0);
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || !matches!(opcode, 0xC6 | 0xC7)
            || prefix.l_bits != 2
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.aaa == 0
            || prefix.zeroing
            || prefix.b
            || !fixed_zero
            || !fixed_one
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
        let group = (modrm.byte >> 3) & 7;
        if !modrm.is_memory || modrm.byte & 7 != 4 || !matches!(group, 1 | 2 | 5 | 6) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        Ok(LiftResult::fallthrough(
            Vec::new(),
            cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_evex_scatter(
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
            || prefix.b
            || prefix.zeroing
            || prefix.l_bits == 3
            || prefix.aaa == 0
            || prefix.vvvv != 0
            || !matches!(opcode, 0xA0..=0xA3)
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
        if !modrm.is_memory || modrm.byte & 7 != 4 || bytes.len() <= cursor + 1 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let sib = bytes[cursor + 1];
        let index_number =
            ((sib >> 3) & 7) | modrm_prefix.rex_x() | if prefix.v_high { 16 } else { 0 };
        let source_number = modrm.reg + if prefix.reg_high { 16 } else { 0 };
        if source_number == index_number {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let data_elem = if prefix.w {
            VecElementType::I64
        } else {
            VecElementType::I32
        };
        let index_elem = if opcode & 1 == 0 {
            VecElementType::I32
        } else {
            VecElementType::I64
        };
        let lanes = prefix
            .width
            .lanes(data_elem)
            .min(prefix.width.lanes(index_elem)) as u8;
        let width_for = |bits: usize| match bits {
            64 => VecWidth::V64,
            128 => VecWidth::V128,
            256 => VecWidth::V256,
            512 => VecWidth::V512,
            _ => unreachable!("invalid scatter vector width"),
        };
        let source_width = width_for(usize::from(lanes) * data_elem.bytes() as usize * 8);
        let index_width = width_for(usize::from(lanes) * index_elem.bytes() as usize * 8);
        let source = self.vec_reg(source_number, source_width);
        let index = self.vec_reg(index_number, index_width);
        let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
        let snapshot = ctx.alloc_vreg();
        let valid_mask = (1u64 << lanes) - 1;
        let mut ops = vec![
            SmirOp::new(
                OpId(0),
                pc,
                OpKind::Mov {
                    dst: snapshot,
                    src: SrcOperand::Reg(mask),
                    width: OpWidth::W64,
                },
            ),
            SmirOp::new(
                OpId(1),
                pc,
                OpKind::And {
                    dst: mask,
                    src1: mask,
                    src2: SrcOperand::Imm(valid_mask as i64),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ];
        let mut x86_addr = modrm.addr.unwrap();
        x86_addr.index = None;
        if x86_addr.disp_size == DispSize::Disp8 {
            x86_addr.disp *= i64::from(data_elem.bytes());
        }
        let mem_width = if data_elem == VecElementType::I32 {
            MemWidth::B4
        } else {
            MemWidth::B8
        };
        for lane in 0..lanes {
            let shifted = ctx.alloc_vreg();
            let cond = ctx.alloc_vreg();
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: shifted,
                    src: snapshot,
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
                OpKind::VExtractLane {
                    dst: value,
                    vec: source,
                    lane,
                    elem: data_elem,
                    sign: SignExtend::Zero,
                },
            ));
            let addr = self
                .append_vsib_lane_address(&x86_addr, index, lane, index_elem, pc, ctx, &mut ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::PredStore {
                    src: SrcOperand::Reg(value),
                    cond,
                    addr,
                    width: mem_width,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: mask,
                    src1: mask,
                    src2: SrcOperand::Imm(!(1i64 << lane)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn append_evex_vector_mask_result(
        &self,
        prefix: VecPrefix,
        dst: VReg,
        raw: VReg,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        self.append_evex_vector_mask_result_width(
            prefix,
            dst,
            raw,
            elem,
            prefix.width,
            pc,
            ctx,
            ops,
        );
    }


    pub(crate) fn append_evex_vector_mask_result_width(
        &self,
        prefix: VecPrefix,
        dst: VReg,
        raw: VReg,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        if prefix.aaa == 0 {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: raw,
                    width,
                },
            ));
            return;
        }

        let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
        let old = if prefix.zeroing {
            None
        } else {
            let old = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst: old,
                    src: dst,
                    width,
                },
            ));
            Some(old)
        };
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
            let shifted = ctx.alloc_vreg();
            let cond = ctx.alloc_vreg();
            let active = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
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
                OpKind::VExtractLane {
                    dst: active,
                    vec: raw,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            let inactive = if let Some(old) = old {
                let inactive = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: inactive,
                        vec: old,
                        lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                inactive
            } else {
                zero
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: selected,
                    cond,
                    src_true: active,
                    src_false: inactive,
                    width: match elem {
                        VecElementType::I8 => OpWidth::W8,
                        VecElementType::I16 | VecElementType::F16 => OpWidth::W16,
                        VecElementType::I32 | VecElementType::F32 => OpWidth::W32,
                        VecElementType::I64 | VecElementType::F64 => OpWidth::W64,
                        _ => unreachable!(),
                    },
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: if lane == 0 { raw } else { dst },
                    scalar: selected,
                    lane,
                    elem,
                },
            ));
        }
    }


    pub(crate) fn append_vdbpsadbw(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        imm: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        // First apply the immediate-controlled dword shuffle to SRC2 within
        // each independent 128-bit lane.
        let dwords = width.lanes(VecElementType::I32) as u8;
        let mut shuffled = self.append_zero_vector(width, VecElementType::I32, pc, ctx, ops);
        for lane in 0..dwords {
            let block_base = lane & !3;
            let selector = (imm >> (2 * (lane & 3))) & 3;
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: src2,
                    lane: block_base + selector,
                    elem: VecElementType::I32,
                    sign: SignExtend::Zero,
                },
            ));
            let inserted = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: inserted,
                    vec: shuffled,
                    scalar,
                    lane,
                    elem: VecElementType::I32,
                },
            ));
            shuffled = inserted;
        }

        // VDBPSADBW's four result pairs are projections of four ordinary
        // MPSADBW computations over the shuffled SRC2 and stationary SRC1.
        // Repeating each imm3 in bits 5:3 applies the same selector to every
        // 128-bit block at all vector lengths.
        let mut partials = Vec::with_capacity(4);
        for selector in [0u8, 1, 6, 7] {
            let partial = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMpsadbw {
                    dst: partial,
                    src1: shuffled,
                    src2: src1,
                    mask: None,
                    width,
                    imm: selector | (selector << 3),
                    zeroing: false,
                },
            ));
            partials.push(partial);
        }

        let words = width.lanes(VecElementType::I16) as u8;
        let mut result = self.append_zero_vector(width, VecElementType::I16, pc, ctx, ops);
        for lane in 0..words {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: partials[usize::from((lane & 7) / 2)],
                    lane,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                },
            ));
            let inserted = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: inserted,
                    vec: result,
                    scalar,
                    lane,
                    elem: VecElementType::I16,
                },
            ));
            result = inserted;
        }
        result
    }


    pub(crate) fn lift_evex_packed_rotate_variable(
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
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
            || !matches!(opcode, 0x14 | 0x15)
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
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let count = if modrm.is_memory {
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
        let src = self.vec_reg(
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
            OpKind::X86PackedRotate {
                dst,
                src,
                count: Some(count),
                mask: (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                amount: 0,
                width: prefix.width,
                elem,
                left: opcode == 0x15,
                zeroing: prefix.zeroing,
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


    pub(crate) fn lift_evex_packed_funnel_shift(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let variable = prefix.map == X86VecMap::Map0F38;
        if prefix.encoding != VecEncodingKind::Evex
            || !matches!(prefix.map, X86VecMap::Map0F38 | X86VecMap::Map0F3A)
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
            || !matches!(opcode, 0x70..=0x73)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match (opcode & 1, prefix.w) {
            (0, true) => VecElementType::I16,
            (1, false) => VecElementType::I32,
            (1, true) => VecElementType::I64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
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
        if prefix.b && (!modrm.is_memory || elem == VecElementType::I16) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let end = cursor + modrm.bytes_consumed;
        if !variable && bytes.len() <= end {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: end + 1,
            });
        }
        let bytes_consumed = end + usize::from(!variable);
        let next_pc = pc + bytes_consumed as u64;
        let mut ops = Vec::new();
        let rm_operand = if modrm.is_memory {
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
        let vvvv = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedFunnelShift {
                dst,
                src: if variable { dst } else { vvvv },
                fill: if variable { vvvv } else { rm_operand },
                count: variable.then_some(rm_operand),
                mask: (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                amount: if variable { 0 } else { bytes[end] },
                width: prefix.width,
                elem,
                left: opcode <= 0x71,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
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


    pub(crate) fn append_gf2p8_mul_vector(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let elem = VecElementType::I8;
        let lanes = width.lanes(elem) as u8;
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let one = self.append_vector_splat_imm(1, width, elem, pc, ctx, ops);
        let reduction_polynomial = self.append_vector_splat_imm(0x1B, width, elem, pc, ctx, ops);
        let mut result = zero;
        let mut multiplicand = src1;
        let mut multiplier = src2;

        // Russian-peasant multiplication in GF(2^8). Each byte lane is
        // independent and reduction uses x^8 + x^4 + x^3 + x + 1 (0x11B).
        for _ in 0..8 {
            let multiplier_lsb = self.append_vector_and(multiplier, one, width, pc, ctx, ops);
            let multiplier_mask =
                self.append_vector_sub(zero, multiplier_lsb, elem, lanes, pc, ctx, ops);
            let contribution =
                self.append_vector_and(multiplicand, multiplier_mask, width, pc, ctx, ops);
            result = self.append_vector_xor(result, contribution, width, pc, ctx, ops);

            let carry =
                self.append_vector_shift(multiplicand, 7, ShiftOp::Lsr, elem, lanes, pc, ctx, ops);
            let carry_mask = self.append_vector_sub(zero, carry, elem, lanes, pc, ctx, ops);
            let reduction =
                self.append_vector_and(carry_mask, reduction_polynomial, width, pc, ctx, ops);
            let shifted =
                self.append_vector_shift(multiplicand, 1, ShiftOp::Lsl, elem, lanes, pc, ctx, ops);
            multiplicand = self.append_vector_xor(shifted, reduction, width, pc, ctx, ops);
            multiplier =
                self.append_vector_shift(multiplier, 1, ShiftOp::Lsr, elem, lanes, pc, ctx, ops);
        }
        result
    }


    pub(crate) fn append_evex_whole_tuple_128(
        &self,
        addr: Address,
        mask: Option<VReg>,
        applicable_mask: i64,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let Some(mask_reg) = mask else {
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
            ));
            return loaded;
        };

        // Tuple1_4X is an all-or-none 16-byte access. PredVLoad consumes
        // condition bit zero, so map any applicable writemask bit to one
        // canonical Boolean without changing architectural flags.
        let applicable = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::And {
                dst: applicable,
                src1: mask_reg,
                src2: SrcOperand::Imm(applicable_mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let negated = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Neg {
                dst: negated,
                src: applicable,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let sign = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Or {
                dst: sign,
                src1: applicable,
                src2: SrcOperand::Reg(negated),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let active = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Shr {
                dst: active,
                src: sign,
                amount: SrcOperand::Imm(63),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        let loaded = self.append_zero_vector(VecWidth::V128, VecElementType::I32, pc, ctx, ops);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::PredVLoad {
                dst: loaded,
                cond: active,
                addr,
                width: VecWidth::V128,
            },
        ));
        loaded
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
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }


    pub(crate) fn append_gf2p8_inverse_vector(
        &self,
        src: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        // In GF(2^8), x^-1 = x^254 for x != 0; the same addition chain maps
        // zero to zero, matching the architectural inverse table.
        let mut power = self.append_gf2p8_mul_vector(src, src, width, pc, ctx, ops);
        let mut result = power;
        for _ in 0..6 {
            power = self.append_gf2p8_mul_vector(power, power, width, pc, ctx, ops);
            result = self.append_gf2p8_mul_vector(result, power, width, pc, ctx, ops);
        }
        result
    }


    pub(crate) fn append_gf2p8_affine_vector(
        &self,
        src: VReg,
        matrix: VReg,
        width: VecWidth,
        imm: u8,
        inverse: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let elem = VecElementType::I8;
        let lanes = width.lanes(elem) as u8;
        let input = if inverse {
            self.append_gf2p8_inverse_vector(src, width, pc, ctx, ops)
        } else {
            src
        };
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let one = self.append_vector_splat_imm(1, width, elem, pc, ctx, ops);
        let mut result = zero;

        for output_bit in 0..8u8 {
            let control =
                self.append_vector_splat_imm(u64::from(7 - output_bit), width, elem, pc, ctx, ops);
            let matrix_row = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VByteShuffle {
                    dst: matrix_row,
                    src: matrix,
                    control,
                    lanes,
                    block_lanes: 8,
                },
            ));
            let mut parity = self.append_vector_and(matrix_row, input, width, pc, ctx, ops);
            for shift in [4, 2, 1] {
                let high = self.append_vector_shift(
                    parity,
                    shift,
                    ShiftOp::Lsr,
                    elem,
                    lanes,
                    pc,
                    ctx,
                    ops,
                );
                parity = self.append_vector_xor(parity, high, width, pc, ctx, ops);
            }
            parity = self.append_vector_and(parity, one, width, pc, ctx, ops);
            if output_bit != 0 {
                parity = self.append_vector_shift(
                    parity,
                    output_bit,
                    ShiftOp::Lsl,
                    elem,
                    lanes,
                    pc,
                    ctx,
                    ops,
                );
            }
            result = self.append_vector_or(result, parity, width, pc, ctx, ops);
        }

        let constant = self.append_vector_splat_imm(u64::from(imm), width, elem, pc, ctx, ops);
        self.append_vector_xor(result, constant, width, pc, ctx, ops)
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


    pub(crate) fn lift_evex_get_mantissa(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = opcode == 0x27;
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F3A
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
            || (prefix.pp == X86SsePrefix::None && prefix.w)
            || (prefix.zeroing && prefix.aaa == 0)
            || (!scalar && (prefix.vvvv != 0 || prefix.v_high))
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
            _ => unreachable!("validated VGETMANT encoding"),
        };
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
            OpKind::X86GetMantissa {
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
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
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
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
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
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
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
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_evex_pabs(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match opcode {
            0x1C => VecElementType::I8,
            0x1D => VecElementType::I16,
            0x1E => VecElementType::I32,
            0x1F => VecElementType::I64,
            _ => unreachable!(),
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
            || (matches!(opcode, 0x1C | 0x1D) && prefix.b)
            || (opcode == 0x1E && prefix.w)
            || (opcode == 0x1F && !prefix.w)
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
        if prefix.b && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let lanes = prefix.width.lanes(elem) as u8;
        let mut ops = Vec::new();
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
            let mem_width = match elem {
                VecElementType::I8 => MemWidth::B1,
                VecElementType::I16 => MemWidth::B2,
                VecElementType::I32 => MemWidth::B4,
                VecElementType::I64 => MemWidth::B8,
                _ => unreachable!(),
            };
            if prefix.b {
                let scalar = ctx.alloc_vreg();
                if prefix.aaa == 0 {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar,
                            addr,
                            width: mem_width,
                            sign: SignExtend::Zero,
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
                    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
                    let mut active = zero;
                    for lane in 0..lanes {
                        let shifted = ctx.alloc_vreg();
                        let bit = ctx.alloc_vreg();
                        let combined = ctx.alloc_vreg();
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
                            OpKind::Or {
                                dst: combined,
                                src1: active,
                                src2: SrcOperand::Reg(bit),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        active = combined;
                    }
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
                            addr,
                            width: mem_width,
                            signed: SignExtend::Zero,
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
                        elem,
                        lanes,
                    },
                ));
                loaded
            } else if prefix.aaa == 0 {
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
                let loaded = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
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
                            addr: Address::base_off(
                                base,
                                i64::from(lane) * i64::from(elem.bytes()),
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
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let masked = prefix.aaa != 0;
        let raw = if masked { ctx.alloc_vreg() } else { dst };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VUnary {
                dst: raw,
                src,
                elem,
                lanes,
                op: VecUnaryOp::Abs,
            },
            self.vec_hint(prefix, opcode),
        ));
        if masked {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
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


    pub(crate) fn lift_evex_integer_pack(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let src_elem = match opcode {
            0x63 | 0x67 => VecElementType::I16,
            0x6B | 0x2B => VecElementType::I32,
            _ => unreachable!(),
        };
        let dst_elem = if src_elem == VecElementType::I16 {
            VecElementType::I8
        } else {
            VecElementType::I16
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
            || (src_elem == VecElementType::I32 && prefix.w)
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
        let broadcast = prefix.b && modrm.is_memory && src_elem == VecElementType::I32;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let src_lanes = prefix.width.lanes(src_elem) as u8;
        let block_lanes = (16 / src_elem.bytes()) as u8;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let scale = if broadcast {
                src_elem.bytes()
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
            let loaded = ctx.alloc_vreg();
            if prefix.aaa == 0 {
                if broadcast {
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar,
                            addr,
                            width: MemWidth::B4,
                            sign: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VBroadcast {
                            dst: loaded,
                            scalar,
                            elem: src_elem,
                            lanes: src_lanes,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: loaded,
                            addr,
                            width: prefix.width,
                        },
                    ));
                }
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
                let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
                if broadcast {
                    // PredLoad tests condition bit 0, so reduce all output
                    // mask bits that consume the broadcast memory operand to
                    // one canonical Boolean rather than passing a positioned
                    // bitmask through directly.
                    let mut active = zero;
                    for block_base in (0..src_lanes).step_by(block_lanes as usize) {
                        let output_base = block_base * 2 + block_lanes;
                        for lane in 0..block_lanes {
                            let shifted = ctx.alloc_vreg();
                            let bit = ctx.alloc_vreg();
                            let combined = ctx.alloc_vreg();
                            ops.push(SmirOp::new(
                                OpId(ops.len() as u16),
                                pc,
                                OpKind::Shr {
                                    dst: shifted,
                                    src: mask,
                                    amount: SrcOperand::Imm(i64::from(output_base + lane)),
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
                                OpKind::Or {
                                    dst: combined,
                                    src1: active,
                                    src2: SrcOperand::Reg(bit),
                                    width: OpWidth::W64,
                                    flags: FlagUpdate::None,
                                },
                            ));
                            active = combined;
                        }
                    }
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
                            cond: active,
                            addr,
                            width: MemWidth::B4,
                            signed: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VBroadcast {
                            dst: loaded,
                            scalar,
                            elem: src_elem,
                            lanes: src_lanes,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VBroadcast {
                            dst: loaded,
                            scalar: zero,
                            elem: src_elem,
                            lanes: src_lanes,
                        },
                    ));
                    let base = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Lea { dst: base, addr },
                    ));
                    let mem_width = if src_elem == VecElementType::I16 {
                        MemWidth::B2
                    } else {
                        MemWidth::B4
                    };
                    for block_base in (0..src_lanes).step_by(block_lanes as usize) {
                        let output_base = block_base * 2 + block_lanes;
                        for lane in 0..block_lanes {
                            let source_lane = block_base + lane;
                            let shifted = ctx.alloc_vreg();
                            let active = ctx.alloc_vreg();
                            let scalar = ctx.alloc_vreg();
                            ops.push(SmirOp::new(
                                OpId(ops.len() as u16),
                                pc,
                                OpKind::Shr {
                                    dst: shifted,
                                    src: mask,
                                    amount: SrcOperand::Imm(i64::from(output_base + lane)),
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
                                    addr: Address::base_off(
                                        base,
                                        i64::from(source_lane) * i64::from(src_elem.bytes()),
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
                                    lane: source_lane,
                                    elem: src_elem,
                                },
                            ));
                        }
                    }
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
            OpKind::VPackSat {
                dst: raw,
                src1: src2,
                src2: self.vec_reg(
                    prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                    prefix.width,
                ),
                src_elem,
                to_unsigned: matches!(opcode, 0x67 | 0x2B),
                src_lanes,
                block_lanes,
            },
            self.vec_hint(prefix, opcode),
        ));
        if prefix.aaa != 0 {
            self.append_evex_vector_mask_result(prefix, dst, raw, dst_elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_evex_integer_unpack(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match opcode {
            0x60 | 0x68 => VecElementType::I8,
            0x61 | 0x69 => VecElementType::I16,
            0x62 | 0x6A => VecElementType::I32,
            0x6C | 0x6D => VecElementType::I64,
            _ => unreachable!(),
        };
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.zeroing && prefix.aaa == 0)
            || matches!(elem, VecElementType::I32) && prefix.w
            || matches!(elem, VecElementType::I64) && !prefix.w
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
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
        let raw = if prefix.aaa == 0 {
            dst
        } else {
            ctx.alloc_vreg()
        };
        self.append_integer_interleave(
            raw,
            self.vec_reg(
                prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                prefix.width,
            ),
            src2,
            elem,
            prefix.width,
            high,
            self.vec_hint(prefix, opcode),
            pc,
            &mut ops,
        );
        if prefix.aaa != 0 {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_evex_pair_intersect(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::Repne
            || prefix.l_bits == 3
            || prefix.aaa != 0
            || prefix.zeroing
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
        if modrm.reg >= 8 || prefix.reg_high || (prefix.b && !modrm.is_memory) {
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
        let lanes = prefix.width.lanes(elem) as u8;
        let mask1 = ctx.alloc_vreg();
        let mask2 = ctx.alloc_vreg();
        let zero = ctx.alloc_vreg();
        for dst in [mask1, mask2, zero] {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
        }
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            let broadcast = ctx.alloc_vreg();
            let compared = ctx.alloc_vreg();
            let matches = ctx.alloc_vreg();
            let bit = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: src1,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: broadcast,
                    scalar,
                    elem,
                    lanes,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VCmp {
                    dst: compared,
                    src1: broadcast,
                    src2,
                    cond: VecCmpCond::Eq,
                    elem,
                    lanes,
                },
            ));
            self.append_sse_movmask(
                matches,
                compared,
                elem,
                lanes,
                OpWidth::W64,
                pc,
                ctx,
                &mut ops,
            );
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: bit,
                    src: SrcOperand::Imm(1i64 << lane),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: selected,
                    cond: matches,
                    src_true: bit,
                    src_false: zero,
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: mask1,
                    src1: mask1,
                    src2: SrcOperand::Reg(selected),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: mask2,
                    src1: mask2,
                    src2: SrcOperand::Reg(matches),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
        }
        let base = modrm.reg & !1;
        for (register, value) in [(base, mask1), (base + 1, mask2)] {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(register))),
                    src: SrcOperand::Reg(value),
                    width: OpWidth::W64,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_evex_integer_test_mask(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || !matches!(prefix.pp, X86SsePrefix::OpSize | X86SsePrefix::Rep)
            || prefix.l_bits == 3
            || prefix.zeroing
            || !matches!(opcode, 0x26 | 0x27)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match (opcode, prefix.w) {
            (0x26, false) => VecElementType::I8,
            (0x26, true) => VecElementType::I16,
            (0x27, false) => VecElementType::I32,
            (0x27, true) => VecElementType::I64,
            _ => unreachable!(),
        };
        let inverted = prefix.pp == X86SsePrefix::Rep;
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if modrm.reg >= 8 || prefix.reg_high {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let broadcast = prefix.b
            && modrm.is_memory
            && matches!(elem, VecElementType::I32 | VecElementType::I64);
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let lanes = prefix.width.lanes(elem) as u8;
        let writemask =
            (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    elem.bytes()
                } else {
                    prefix.width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = writemask {
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
        let anded = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VAnd {
                dst: anded,
                src1: self.vec_reg(
                    prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                    prefix.width,
                ),
                src2,
                width: prefix.width,
            },
        ));
        let zero = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
        let compared = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: compared,
                src1: anded,
                src2: zero,
                cond: if inverted {
                    VecCmpCond::Eq
                } else {
                    VecCmpCond::Ne
                },
                elem,
                lanes,
            },
        ));
        let raw_mask = ctx.alloc_vreg();
        self.append_sse_movmask(
            raw_mask,
            compared,
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
            if let Some(mask) = writemask {
                OpKind::And {
                    dst,
                    src1: raw_mask,
                    src2: SrcOperand::Reg(mask),
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
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_evex_integer_compare(
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
            || prefix.zeroing
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, fixed_cond, signed) = match (prefix.map, opcode) {
            (X86VecMap::Map0F, 0x64) => (VecElementType::I8, Some(VecCmpCond::Gt), true),
            (X86VecMap::Map0F, 0x65) => (VecElementType::I16, Some(VecCmpCond::Gt), true),
            (X86VecMap::Map0F, 0x66) => (VecElementType::I32, Some(VecCmpCond::Gt), true),
            (X86VecMap::Map0F, 0x74) => (VecElementType::I8, Some(VecCmpCond::Eq), true),
            (X86VecMap::Map0F, 0x75) => (VecElementType::I16, Some(VecCmpCond::Eq), true),
            (X86VecMap::Map0F, 0x76) => (VecElementType::I32, Some(VecCmpCond::Eq), true),
            (X86VecMap::Map0F38, 0x29) => (VecElementType::I64, Some(VecCmpCond::Eq), true),
            (X86VecMap::Map0F38, 0x37) => (VecElementType::I64, Some(VecCmpCond::Gt), true),
            (X86VecMap::Map0F3A, 0x1E) => (
                if prefix.w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                None,
                false,
            ),
            (X86VecMap::Map0F3A, 0x1F) => (
                if prefix.w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                None,
                true,
            ),
            (X86VecMap::Map0F3A, 0x3E) => (
                if prefix.w {
                    VecElementType::I16
                } else {
                    VecElementType::I8
                },
                None,
                false,
            ),
            (X86VecMap::Map0F3A, 0x3F) => (
                if prefix.w {
                    VecElementType::I16
                } else {
                    VecElementType::I8
                },
                None,
                true,
            ),
            _ => unreachable!(),
        };
        if prefix.map == X86VecMap::Map0F38 && !prefix.w {
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
        if modrm.reg >= 8 || prefix.reg_high {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let broadcast = prefix.b
            && modrm.is_memory
            && matches!(elem, VecElementType::I32 | VecElementType::I64);
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let imm_offset = cursor + modrm.bytes_consumed;
        let immediate = fixed_cond.is_none();
        let imm = if immediate {
            if bytes.len() <= imm_offset {
                return Err(LiftError::Incomplete {
                    addr: pc,
                    have: bytes.len(),
                    need: imm_offset + 1,
                });
            }
            Some(bytes[imm_offset] & 0x07)
        } else {
            None
        };
        let (cond, constant) = if let Some(cond) = fixed_cond {
            (Some(cond), None)
        } else {
            match imm.unwrap() {
                0 => (Some(VecCmpCond::Eq), None),
                1 => (
                    Some(if signed {
                        VecCmpCond::Lt
                    } else {
                        VecCmpCond::Ltu
                    }),
                    None,
                ),
                2 => (
                    Some(if signed {
                        VecCmpCond::Le
                    } else {
                        VecCmpCond::Leu
                    }),
                    None,
                ),
                3 => (None, Some(false)),
                4 => (Some(VecCmpCond::Ne), None),
                5 => (
                    Some(if signed {
                        VecCmpCond::Ge
                    } else {
                        VecCmpCond::Geu
                    }),
                    None,
                ),
                6 => (
                    Some(if signed {
                        VecCmpCond::Gt
                    } else {
                        VecCmpCond::Gtu
                    }),
                    None,
                ),
                7 => (None, Some(true)),
                _ => unreachable!(),
            }
        };

        let bytes_consumed = imm_offset + usize::from(immediate);
        let next_pc = pc + bytes_consumed as u64;
        let lanes = prefix.width.lanes(elem) as u8;
        let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
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
            let loaded = ctx.alloc_vreg();
            if let Some(mask_reg) = mask {
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
                        elem,
                        lanes,
                    },
                ));
                let base = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Lea { dst: base, addr },
                ));
                let mem_width = match elem {
                    VecElementType::I8 => MemWidth::B1,
                    VecElementType::I16 => MemWidth::B2,
                    VecElementType::I32 => MemWidth::B4,
                    VecElementType::I64 => MemWidth::B8,
                    _ => unreachable!(),
                };
                for lane in 0..lanes {
                    let shifted = ctx.alloc_vreg();
                    let active = ctx.alloc_vreg();
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Shr {
                            dst: shifted,
                            src: mask_reg,
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
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let mem_width = if elem == VecElementType::I32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: mem_width,
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
                        lanes,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
            }
            loaded
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let raw_mask = ctx.alloc_vreg();
        if let Some(cond) = cond {
            let compared = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VCmp {
                    dst: compared,
                    src1: self.vec_reg(
                        prefix.vvvv + if prefix.v_high { 16 } else { 0 },
                        prefix.width,
                    ),
                    src2,
                    cond,
                    elem,
                    lanes,
                },
            ));
            self.append_sse_movmask(
                raw_mask,
                compared,
                elem,
                lanes,
                OpWidth::W64,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            let all_lanes = if constant.unwrap() {
                if lanes == 64 {
                    -1
                } else {
                    ((1u64 << lanes) - 1) as i64
                }
            } else {
                0
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: raw_mask,
                    src: SrcOperand::Imm(all_lanes),
                    width: OpWidth::W64,
                },
            ));
        }
        let dst = VReg::Arch(ArchReg::X86(X86Reg::K(modrm.reg)));
        if let Some(mask_reg) = mask {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst,
                    src1: raw_mask,
                    src2: SrcOperand::Reg(mask_reg),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(raw_mask),
                    width: OpWidth::W64,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
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


    pub(crate) fn lift_evex_packed_fp_arithmetic(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
            || !matches!(opcode, 0x58 | 0x59 | 0x5C..=0x5F)
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.pp == X86SsePrefix::None {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        if (elem == VecElementType::F32 && prefix.w) || (elem == VecElementType::F64 && !prefix.w) {
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
        if prefix.b && !modrm.is_memory && prefix.width != VecWidth::V512 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let broadcast = prefix.b && modrm.is_memory;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
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
                    broadcast,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
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
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = ctx.alloc_vreg();
        let lanes = prefix.width.lanes(elem) as u8;
        let kind = match opcode {
            0x58 => OpKind::VAdd {
                dst: raw,
                src1,
                src2,
                elem,
                lanes,
            },
            0x59 => OpKind::VMul {
                dst: raw,
                src1,
                src2,
                elem,
                lanes,
            },
            0x5C => OpKind::VSub {
                dst: raw,
                src1,
                src2,
                elem,
                lanes,
            },
            0x5D | 0x5F => OpKind::VX86MinMax {
                dst: raw,
                src1,
                src2,
                elem,
                lanes,
                min: opcode == 0x5D,
            },
            0x5E => OpKind::VDiv {
                dst: raw,
                src1,
                src2,
                elem,
                lanes,
            },
            _ => unreachable!(),
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            self.vec_hint(prefix, opcode),
        ));
        self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_evex_mask_broadcast(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::Rep
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || prefix.l_bits == 3
            || !matches!((opcode, prefix.w), (0x2A, true) | (0x3A, false))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            rep_prefix: Some(0xF3),
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if modrm.is_memory || modrm.rm >= 8 || prefix.rm_high {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, source_mask) = if opcode == 0x2A {
            (VecElementType::I64, 0xFF)
        } else {
            (VecElementType::I32, 0xFFFF)
        };
        let mut ops = Vec::new();
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(0),
            pc,
            OpKind::And {
                dst: scalar,
                src1: VReg::Arch(ArchReg::X86(X86Reg::K(modrm.rm))),
                src2: SrcOperand::Imm(source_mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        ops.push(SmirOp::new(
            OpId(1),
            pc,
            OpKind::VBroadcast {
                dst: self.vec_reg(
                    modrm.reg + if prefix.reg_high { 16 } else { 0 },
                    prefix.width,
                ),
                scalar,
                elem,
                lanes: prefix.width.lanes(elem) as u8,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_evex_fp16_flag_compare(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.pp != X86SsePrefix::None
            || prefix.w
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.aaa != 0
            || prefix.zeroing
            || !matches!(opcode, 0x2E | 0x2F)
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
        let src1 = self.xmm(modrm.reg + if prefix.reg_high { 16 } else { 0 });
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                VecElementType::F16,
                ctx,
            );
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            let vector = ctx.alloc_vreg();
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem: VecElementType::F16,
                    lanes: 1,
                },
            ));
            vector
        } else {
            self.xmm(modrm.rm + if prefix.rm_high { 16 } else { 0 })
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpCompare {
                src1,
                src2,
                elem: VecElementType::F16,
                signaling: opcode == 0x2F,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
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


    pub(crate) fn lift_evex_word_move(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Evex
            || prefix.map != X86VecMap::Map5
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || !matches!(opcode, 0x6E | 0x7E)
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
        if !modrm.is_memory && prefix.rm_high {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let xmm = self.xmm(modrm.reg + if prefix.reg_high { 16 } else { 0 });
        let mut ops = Vec::new();
        if opcode == 0x6E {
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    MemWidth::B2.bytes(),
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
                self.gpr(modrm.rm)
            };
            self.append_scalar_zeroed_xmm_result(
                xmm,
                scalar,
                VecElementType::I16,
                true,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: xmm,
                    lane: 0,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                },
            ));
            if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    MemWidth::B2.bytes(),
                    ctx,
                );
                ops.extend(pre_ops);
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: scalar,
                        addr,
                        width: MemWidth::B2,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: self.gpr(modrm.rm),
                        src: SrcOperand::Reg(scalar),
                        width: OpWidth::W32,
                    },
                ));
            }
        }

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
            if prefix.aaa != 0 {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    width,
                    broadcast,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
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
                zeroing: prefix.zeroing,
            },
        ));
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


    pub(crate) fn unsupported_evex_map_opcode(
        &self,
        map: X86VecMap,
        opcode: u8,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        Err(LiftError::Unsupported {
            addr: pc,
            mnemonic: format!("EVEX {map:?} opcode 0x{opcode:02X}"),
        })
    }
}
