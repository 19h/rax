//! fp.rs

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86FmaOp, X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign,
    X86VecMap, X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource,
    X86X87Constant, X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth,
    X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};

impl X86_64Lifter {
    pub(crate) fn append_fp_estimate_result(
        &self,
        dst: VReg,
        merge: VReg,
        src: VReg,
        opcode: u8,
        scalar: bool,
        width: VecWidth,
        legacy: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let raw = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VUnary {
                dst: raw,
                src,
                elem: VecElementType::F32,
                lanes: if scalar {
                    1
                } else {
                    width.lanes(VecElementType::F32) as u8
                },
                op: if opcode == 0x53 {
                    VecUnaryOp::FRecipEstimate
                } else {
                    VecUnaryOp::FRsqrtEstimate
                },
            },
        ));
        if scalar {
            let low = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: low,
                    vec: raw,
                    lane: 0,
                    elem: VecElementType::F32,
                    sign: SignExtend::Zero,
                },
            ));
            if legacy {
                let result = ctx.alloc_vreg();
                self.append_vex_scalar_result(
                    result,
                    merge,
                    low,
                    VecElementType::F32,
                    pc,
                    ctx,
                    ops,
                );
                self.append_legacy_packed_result(dst, result, VecElementType::F32, pc, ctx, ops);
            } else {
                self.append_vex_scalar_result(dst, merge, low, VecElementType::F32, pc, ctx, ops);
            }
        } else if legacy {
            self.append_legacy_packed_result(dst, raw, VecElementType::F32, pc, ctx, ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: raw,
                    width,
                },
            ));
        }
    }

    pub(crate) fn lift_vec_fp_estimate(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = prefix.pp == X86SsePrefix::Rep;
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::Rep)
            || (!scalar && prefix.vvvv != 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = prefix.modrm_prefix(cursor);
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if scalar {
                let value = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: value,
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar: value,
                        elem: VecElementType::F32,
                        lanes: 1,
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
                        width,
                    },
                ));
                vector
            }
        } else {
            self.vec_reg(modrm.rm, width)
        };
        self.append_fp_estimate_result(
            self.vec_reg(modrm.reg, width),
            self.xmm(prefix.vvvv),
            src,
            opcode,
            scalar,
            width,
            false,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn append_fp_addsub_horizontal(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        opcode: u8,
        elem: VecElementType,
        width: VecWidth,
        hint: Option<X86OpHint>,
        pc: u64,
        ops: &mut Vec<SmirOp>,
    ) {
        let kind = OpKind::X86FpBinary {
            dst,
            src1,
            src2,
            mask: None,
            elem,
            lanes: width.lanes(elem) as u8,
            op: match opcode {
                0xD0 => X86FpBinaryOp::AddSub,
                0x7C => X86FpBinaryOp::HorizontalAdd,
                0x7D => X86FpBinaryOp::HorizontalSub,
                _ => unreachable!("validated SSE3 horizontal/add-sub opcode"),
            },
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        };
        let id = OpId(ops.len() as u16);
        ops.push(match hint {
            Some(hint) => SmirOp::with_hint(id, pc, kind, hint),
            None => SmirOp::new(id, pc, kind),
        });
    }

    pub(crate) fn lift_vec_addsub_horizontal(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F
            || !matches!(prefix.pp, X86SsePrefix::OpSize | X86SsePrefix::Repne)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.pp == X86SsePrefix::Repne {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = prefix.modrm_prefix(cursor);
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
        self.append_fp_addsub_horizontal(
            self.vec_reg(modrm.reg, prefix.width),
            self.vec_reg(prefix.vvvv, prefix.width),
            src2,
            opcode,
            elem,
            prefix.width,
            if modrm.is_memory {
                Some(self.vec_hint(prefix, opcode))
            } else {
                None
            },
            pc,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_fma3(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let fp16 = prefix.encoding == VecEncodingKind::Evex
            && prefix.map == X86VecMap::Map6
            && prefix.pp == X86SsePrefix::OpSize
            && !prefix.w;
        if !fp16 && (prefix.map != X86VecMap::Map0F38 || prefix.pp != X86SsePrefix::OpSize) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let low = opcode & 0x0F;
        let scalar = matches!(low, 0x09 | 0x0B | 0x0D | 0x0F);
        if prefix.encoding == VecEncodingKind::Evex && (prefix.zeroing && prefix.aaa == 0) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if fp16 {
            VecElementType::F16
        } else if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = prefix.modrm_prefix(cursor);
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let embedded_rounding =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        if (scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_rounding && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let operation_width = if scalar {
            VecWidth::V128
        } else if embedded_rounding {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar {
            1
        } else {
            operation_width.lanes(elem) as u8
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
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            operation_width,
        );
        let old_dst = dst;
        let vex_src = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            operation_width,
        );
        let mask_cond = if scalar {
            self.append_evex_mask_condition(prefix, pc, ctx, &mut ops)
        } else {
            None
        };
        let rm_src = if modrm.is_memory {
            let (addr, pre_ops) = if scalar {
                self.vec_scalar_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    elem,
                    ctx,
                )
            } else if prefix.encoding == VecEncodingKind::Evex && prefix.b {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    elem.bytes(),
                    ctx,
                )
            } else {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if scalar {
                let scalar_value = ctx.alloc_vreg();
                let mem_width = match elem {
                    VecElementType::F16 => MemWidth::B2,
                    VecElementType::F32 => MemWidth::B4,
                    VecElementType::F64 => MemWidth::B8,
                    _ => unreachable!(),
                };
                if let Some(cond) = mask_cond {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar_value,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar_value,
                            cond,
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
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar: scalar_value,
                        elem,
                        lanes: 1,
                    },
                ));
                vector
            } else if prefix.encoding == VecEncodingKind::Evex && prefix.b {
                if prefix.aaa == 0 {
                    self.append_broadcast_memory_source(
                        addr,
                        elem,
                        operation_width,
                        pc,
                        ctx,
                        &mut ops,
                    )
                } else {
                    self.append_masked_broadcast_memory_source(
                        addr,
                        elem,
                        operation_width,
                        VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                        pc,
                        ctx,
                        &mut ops,
                    )
                }
            } else if prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0 {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    operation_width,
                    false,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: vector,
                        addr,
                        width: operation_width,
                    },
                ));
                vector
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                operation_width,
            )
        };

        let order = match opcode >> 4 {
            0x09 => X86FmaOrder::Order132,
            0x0A => X86FmaOrder::Order213,
            0x0B => X86FmaOrder::Order231,
            _ => unreachable!(),
        };
        let kind = match low {
            0x06 => X86FmaKind::AddSub,
            0x07 => X86FmaKind::SubAdd,
            0x08 | 0x09 => X86FmaKind::Add,
            0x0A | 0x0B => X86FmaKind::Sub,
            0x0C | 0x0D => X86FmaKind::NegativeMultiplyAdd,
            0x0E | 0x0F => X86FmaKind::NegativeMultiplySub,
            _ => unreachable!(),
        };
        let raw = ctx.alloc_vreg();
        let fma_hint = match self.vec_hint(prefix, opcode) {
            X86OpHint::EvexOp { map, pp, w, .. } if embedded_rounding => X86OpHint::EvexOp {
                map,
                pp,
                opcode,
                width: operation_width,
                w,
            },
            hint => hint,
        };
        if fp16 {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86FP16Fma {
                    dst: raw,
                    src1: old_dst,
                    src2: vex_src,
                    src3: rm_src,
                    mask: (prefix.aaa != 0)
                        .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                    kind,
                    order,
                    round,
                    lanes,
                },
                fma_hint,
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86Fma(X86FmaOp {
                    dst: raw,
                    src1: old_dst,
                    src2: vex_src,
                    src3: rm_src,
                    mask: (prefix.aaa != 0)
                        .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                    elem,
                    kind,
                    order,
                    round,
                    lanes,
                }),
                fma_hint,
            ));
        }

        let result = raw;

        if scalar {
            let low_result = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: low_result,
                    vec: result,
                    lane: 0,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            let low_result = self.append_evex_scalar_select(
                prefix, mask_cond, dst, low_result, elem, pc, ctx, &mut ops,
            );
            self.append_vex_scalar_result(dst, old_dst, low_result, elem, pc, ctx, &mut ops);
        } else {
            if prefix.encoding == VecEncodingKind::Evex {
                self.append_evex_vector_mask_result_width(
                    prefix,
                    dst,
                    result,
                    elem,
                    operation_width,
                    pc,
                    ctx,
                    &mut ops,
                );
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst,
                        src: result,
                        width: operation_width,
                    },
                ));
            }
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_scalar_fp_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        from: VecElementType,
        to: VecElementType,
    ) -> Result<LiftResult, LiftError> {
        let expected_w = matches!((from, to), (VecElementType::F64, _));
        if !from.is_float()
            || !to.is_float()
            || from == to
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(
                    (from, to),
                    (VecElementType::F32, VecElementType::F64)
                        | (VecElementType::F64, VecElementType::F32)
                ))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.w != expected_w || (prefix.zeroing && prefix.aaa == 0)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let opcode = bytes[prefix.bytes];
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let mut ops = Vec::new();
        let mask_condition = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                from,
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            let width = match from {
                VecElementType::F16 => MemWidth::B2,
                VecElementType::F32 => MemWidth::B4,
                VecElementType::F64 => MemWidth::B8,
                _ => unreachable!(),
            };
            if let Some(cond) = mask_condition {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: value,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: value,
                        cond,
                        addr,
                        width,
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
                        width,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            value
        } else {
            self.xmm(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
            )
        };
        let dst = self.xmm(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
        );
        let merge = self.xmm(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
        );
        let embedded_rounding =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && from.bytes() > to.bytes();
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
            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask,
                from,
                to,
                mask_zeroing: prefix.zeroing,
                round,
                suppress_exceptions: prefix.encoding == VecEncodingKind::Evex && prefix.b,
                zero_upper: true,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_packed_fp16_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        from: VecElementType,
        to: VecElementType,
    ) -> Result<LiftResult, LiftError> {
        let expected_w = from == VecElementType::F64;
        if !matches!(
            from,
            VecElementType::F16 | VecElementType::F32 | VecElementType::F64
        ) || !matches!(
            to,
            VecElementType::F16 | VecElementType::F32 | VecElementType::F64
        ) || from == to
            || (prefix.encoding == VecEncodingKind::Vex
                && (from != VecElementType::F16 || to != VecElementType::F32 || prefix.w))
            || prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.v_high || prefix.w != expected_w || (prefix.zeroing && prefix.aaa == 0)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let opcode = bytes[prefix.bytes];
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let register_sae_or_er =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        if prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && modrm.is_memory
            && prefix.map == X86VecMap::Map0F38
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if prefix.encoding == VecEncodingKind::Evex && !register_sae_or_er && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        // Widening register-source SAE implies 512 bits and ignores L'L
        // (Intel SDM revision 092, Table 2-43).

        let instruction_width = if register_sae_or_er {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = (instruction_width.bytes() / from.bytes().max(to.bytes())) as u8;
        let src_bytes = u32::from(lanes) * from.bytes();
        let dst_bytes = u32::from(lanes) * to.bytes();
        let container_width = |bytes: u32| match bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let src_width = container_width(src_bytes);
        let dst_width = container_width(dst_bytes);
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast { from.bytes() } else { src_bytes },
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            let zero = ctx.alloc_vreg();
            if prefix.encoding != VecEncodingKind::Vex {
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
                        dst: value,
                        scalar: zero,
                        elem: from,
                        lanes,
                    },
                ));
            }
            let mem_width = match from {
                VecElementType::F16 => MemWidth::B2,
                VecElementType::F32 => MemWidth::B4,
                VecElementType::F64 => MemWidth::B8,
                _ => unreachable!(),
            };
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
                if let Some(mask_reg) = mask {
                    let cond = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: cond,
                            src1: mask_reg,
                            src2: SrcOperand::Imm(((1u64 << lanes) - 1) as i64),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond,
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
                            dst: scalar,
                            addr,
                            width: mem_width,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: value,
                        scalar,
                        elem: from,
                        lanes,
                    },
                ));
            } else if mask.is_some() || src_bytes < 8 {
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
                    if let Some(mask_reg) = mask {
                        let shifted = ctx.alloc_vreg();
                        let cond = ctx.alloc_vreg();
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
                            OpKind::PredLoad {
                                dst: scalar,
                                cond,
                                addr: Address::base_off(
                                    base,
                                    i64::from(lane) * i64::from(from.bytes()),
                                ),
                                width: mem_width,
                                signed: SignExtend::Zero,
                            },
                        ));
                    } else {
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Load {
                                dst: scalar,
                                addr: Address::base_off(
                                    base,
                                    i64::from(lane) * i64::from(from.bytes()),
                                ),
                                width: mem_width,
                                sign: SignExtend::Zero,
                            },
                        ));
                    }
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst: value,
                            vec: value,
                            scalar,
                            lane,
                            elem: from,
                        },
                    ));
                }
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: src_width,
                    },
                ));
            }
            value
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                src_width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            dst_width,
        );
        let round = if register_sae_or_er && from.bytes() > to.bytes() {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        let hint = if register_sae_or_er {
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: instruction_width,
                w: prefix.w,
            }
        } else {
            self.vec_hint(prefix, opcode)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from,
                to,
                lanes,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: register_sae_or_er,
                report_fp16_denormal: from == VecElementType::F16
                    && (to == VecElementType::F64 || broadcast),
            },
            hint,
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_packed_int_fp_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let opcode = bytes[prefix.bytes];
        let conversion = if prefix.encoding == VecEncodingKind::Vex {
            match (opcode, prefix.pp) {
                (0x5B, X86SsePrefix::None) => {
                    Some((true, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::OpSize) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::Rep) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, true))
                }
                (0xE6, X86SsePrefix::Rep) => {
                    Some((true, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::Repne) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::OpSize) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, true))
                }
                _ => None,
            }
        } else {
            match (opcode, prefix.pp, prefix.w) {
                (0x5B, X86SsePrefix::None, false) => {
                    Some((true, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::None, true) => {
                    Some((true, VecElementType::I64, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::Rep, false) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, true))
                }
                (0xE6, X86SsePrefix::Rep, false) => {
                    Some((true, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::Rep, true) => {
                    Some((true, VecElementType::I64, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::Repne, true) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, true))
                }
                (0x7A, X86SsePrefix::Rep, false) => {
                    Some((true, VecElementType::I32, VecElementType::F64, false, false))
                }
                (0x7A, X86SsePrefix::Rep, true) => {
                    Some((true, VecElementType::I64, VecElementType::F64, false, false))
                }
                (0x7A, X86SsePrefix::Repne, false) => {
                    Some((true, VecElementType::I32, VecElementType::F32, false, false))
                }
                (0x7A, X86SsePrefix::Repne, true) => {
                    Some((true, VecElementType::I64, VecElementType::F32, false, false))
                }
                (0x7B, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I64, VecElementType::F32, true, false))
                }
                (0x7B, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I64, VecElementType::F64, true, false))
                }
                (0x7A, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I64, VecElementType::F32, true, true))
                }
                (0x7A, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I64, VecElementType::F64, true, true))
                }
                (0x79, X86SsePrefix::None, false) => Some((
                    false,
                    VecElementType::I32,
                    VecElementType::F32,
                    false,
                    false,
                )),
                (0x79, X86SsePrefix::None, true) => Some((
                    false,
                    VecElementType::I32,
                    VecElementType::F64,
                    false,
                    false,
                )),
                (0x78, X86SsePrefix::None, false) => {
                    Some((false, VecElementType::I32, VecElementType::F32, false, true))
                }
                (0x78, X86SsePrefix::None, true) => {
                    Some((false, VecElementType::I32, VecElementType::F64, false, true))
                }
                (0x79, X86SsePrefix::OpSize, false) => Some((
                    false,
                    VecElementType::I64,
                    VecElementType::F32,
                    false,
                    false,
                )),
                (0x79, X86SsePrefix::OpSize, true) => Some((
                    false,
                    VecElementType::I64,
                    VecElementType::F64,
                    false,
                    false,
                )),
                (0x78, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I64, VecElementType::F32, false, true))
                }
                (0x78, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I64, VecElementType::F64, false, true))
                }
                _ => None,
            }
        };
        let Some((int_to_fp, int_elem, fp_elem, signed, truncate)) = conversion else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        if prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex && prefix.v_high)
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            ..prefix.modrm_prefix(cursor)
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let register_b = prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        let ignores_embedded_rounding =
            int_to_fp && int_elem == VecElementType::I32 && fp_elem == VecElementType::F64;
        let embedded_control = register_b && !ignores_embedded_rounding;
        if !register_b && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        // VCVTDQ2PD and VCVTUDQ2PD ignore an attempted register-source ER
        // encoding, including L'L, while still implying a 512-bit operation
        // (Intel SDM revision 092, Table 2-43 and instruction descriptions).
        let operation_width = if register_b {
            VecWidth::V512
        } else {
            prefix.width
        };
        let src_elem = if int_to_fp { int_elem } else { fp_elem };
        let dst_elem = if int_to_fp { fp_elem } else { int_elem };
        let (lanes, src_bytes, dst_bytes) = if dst_elem.bytes() >= src_elem.bytes() {
            let lanes = operation_width.bytes() / dst_elem.bytes();
            (lanes, lanes * src_elem.bytes(), operation_width.bytes())
        } else {
            let lanes = operation_width.bytes() / src_elem.bytes();
            (lanes, operation_width.bytes(), lanes * dst_elem.bytes())
        };
        let exact_width = |bytes: u32| match bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let register_width = |bytes: u32| match bytes {
            0..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let src_width = exact_width(src_bytes);
        let dst_width = register_width(dst_bytes);
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    src_elem.bytes()
                } else {
                    src_bytes
                },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask_reg) = mask {
                self.append_evex_masked_vector_source(
                    addr, src_elem, src_width, broadcast, mask_reg, pc, ctx, &mut ops,
                )
            } else if broadcast {
                self.append_broadcast_memory_source(addr, src_elem, src_width, pc, ctx, &mut ops)
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
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                src_width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            dst_width,
        );
        let round = if !int_to_fp && truncate {
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
        let hint = if register_b {
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: operation_width,
                w: prefix.w,
            }
        } else {
            self.vec_hint(prefix, opcode)
        };
        let kind = if int_to_fp {
            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem,
                fp_elem,
                signed,
                lanes: lanes as u8,
                src_width,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: embedded_control,
            }
        } else {
            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                signed,
                truncate,
                lanes: lanes as u8,
                src_width,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: embedded_control,
            }
        };
        ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
