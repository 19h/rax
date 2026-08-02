//! misc.rs

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
    pub(crate) fn packed_unsigned_average_kind(
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        elem: VecElementType,
    ) -> OpKind {
        OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes: width.lanes(elem) as u8,
            op: VLaneOp::AvgRnd,
            signed: false,
            set_ovf: false,
        }
    }

    /// Construct a VEX scalar XMM result: lane 0 comes from `low_scalar`, the
    /// remaining 128-bit lanes come from `upper_src`, and all state above bit
    /// 127 is cleared. Extract upper lanes before clearing `dst` so aliases are
    /// exact (for example, `vaddss xmm0,xmm0,xmm1`).
    pub(crate) fn append_vex_scalar_result(
        &self,
        dst: VReg,
        upper_src: VReg,
        low_scalar: VReg,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let xmm_lanes = VecWidth::V128.lanes(elem) as u8;
        let mut upper = Vec::with_capacity((xmm_lanes - 1) as usize);
        for lane in 1..xmm_lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: upper_src,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            upper.push((lane, scalar));
        }

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
                dst,
                scalar: zero,
                elem,
                lanes: 1,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInsertLane {
                dst,
                vec: dst,
                scalar: low_scalar,
                lane: 0,
                elem,
            },
        ));
        for (lane, scalar) in upper {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
    }

    /// Emit the exact integer sign-bit mask used by the (V)MOVMSK families.
    pub(crate) fn append_sse_movmask(
        &self,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        lanes: u8,
        dst_width: OpWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let accumulated = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: accumulated,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            let sign = ctx.alloc_vreg();
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
                    dst: sign,
                    src: scalar,
                    amount: SrcOperand::Imm(i64::from(elem.bytes() * 8 - 1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            let positioned = if lane == 0 {
                sign
            } else {
                let shifted = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shl {
                        dst: shifted,
                        src: sign,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                shifted
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: accumulated,
                    src1: accumulated,
                    src2: SrcOperand::Reg(positioned),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(accumulated),
                width: dst_width,
            },
        ));
    }

    pub(crate) fn lift_vec_pmovmskb(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
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
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let ops = vec![SmirOp::with_hint(
            OpId(0),
            pc,
            OpKind::X86MovMask {
                dst: self.gpr(modrm.reg),
                src: self.vec_reg(modrm.rm, prefix.width),
                elem: VecElementType::I8,
                lanes: prefix.width.lanes(VecElementType::I8) as u8,
                dst_width: OpWidth::W32,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD7,
                width: prefix.width,
                w: prefix.w,
            },
        )];
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_half_move(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.l_bits != 0
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.w != (prefix.pp == X86SsePrefix::OpSize)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let store = matches!(opcode, 0x13 | 0x17);
        if (store && !modrm.is_memory)
            || (!store && prefix.pp == X86SsePrefix::OpSize && !modrm.is_memory)
            || (store && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let lane = if matches!(opcode, 0x16 | 0x17) { 1 } else { 0 };
        let reg_ext = if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
            16
        } else {
            0
        };
        let mut ops = Vec::new();
        if store {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 8, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: self.xmm(modrm.reg + reg_ext),
                    lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: scalar,
                    addr,
                    width: MemWidth::B8,
                },
            ));
        } else {
            let dst = self.xmm(modrm.reg + reg_ext);
            let merge = self.xmm(
                prefix.vvvv
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                        16
                    } else {
                        0
                    },
            );
            // VEX/EVEX XMM destinations clear all shared vector state above
            // bit 127. Snapshot the merge lane before clearing `dst` so every
            // dst/src1/src2 alias remains exact.
            let preserved_lane = 1 - lane;
            let preserved = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: preserved,
                    vec: merge,
                    lane: preserved_lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                    self.vec_disp8_addr_to_smir(
                        prefix,
                        modrm.addr.as_ref().unwrap(),
                        next_pc,
                        8,
                        ctx,
                    )
                } else {
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
                };
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
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
                scalar
            } else {
                let source = self.xmm(
                    modrm.rm
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                            16
                        } else {
                            0
                        },
                );
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: if opcode == 0x12 { 1 } else { 0 },
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst,
                    scalar: zero,
                    elem: VecElementType::I64,
                    lanes: 1,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar: preserved,
                    lane: preserved_lane,
                    elem: VecElementType::I64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem: VecElementType::I64,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_pinsrw_pextrw(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (opcode == 0xC5 && (prefix.vvvv != 0 || prefix.v_high))
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
        if (opcode == 0xC5 && modrm.is_memory)
            || (prefix.encoding == VecEncodingKind::Evex
                && ((opcode == 0xC5 && prefix.reg_high)
                    || (opcode == 0xC4 && !modrm.is_memory && prefix.rm_high)))
        {
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
        let lane = bytes[imm_offset] & 0x07;
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();

        if opcode == 0xC5 {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: self.xmm(
                        modrm.rm
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                16
                            } else {
                                0
                            },
                    ),
                    lane,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.reg),
                    src: SrcOperand::Reg(scalar),
                    width: OpWidth::W32,
                },
            ));
        } else {
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    2,
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
            self.append_insert_scalar_lane(
                self.xmm(
                    modrm.reg
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                            16
                        } else {
                            0
                        },
                ),
                self.xmm(
                    prefix.vvvv
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                            16
                        } else {
                            0
                        },
                ),
                scalar,
                VecElementType::I16,
                lane,
                pc,
                ctx,
                &mut ops,
            );
        }

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
