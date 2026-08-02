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
        let modrm_prefix = prefix.modrm_prefix(cursor);
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
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            ..prefix.modrm_prefix(cursor)
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
        let modrm_prefix = prefix.modrm_prefix(cursor);
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
