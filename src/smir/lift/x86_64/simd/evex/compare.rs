//! compare.rs

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
}
