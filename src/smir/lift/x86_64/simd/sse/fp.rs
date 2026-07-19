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
    pub(crate) fn lift_sse_sqrt(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, 0x51],
            });
        }
        let prefix_kind = if prefix.rep_prefix == Some(0xF3) {
            X86SsePrefix::Rep
        } else if prefix.rep_prefix == Some(0xF2) {
            X86SsePrefix::Repne
        } else if prefix.operand_size_override {
            X86SsePrefix::OpSize
        } else {
            X86SsePrefix::None
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let dst = self.xmm(modrm.reg);

        if matches!(prefix_kind, X86SsePrefix::Rep | X86SsePrefix::Repne) {
            let elem = if prefix_kind == X86SsePrefix::Rep {
                VecElementType::F32
            } else {
                VecElementType::F64
            };
            let src = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::F32 {
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
                        lanes: 1,
                    },
                ));
                vector
            } else {
                self.xmm(modrm.rm)
            };
            let vector_result = ctx.alloc_vreg();
            let scalar_result = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VUnary {
                    dst: vector_result,
                    src,
                    elem,
                    lanes: 1,
                    op: VecUnaryOp::FSqrt,
                },
                X86OpHint::SseOp {
                    prefix: prefix_kind,
                    opcode: 0x51,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar_result,
                    vec: vector_result,
                    lane: 0,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar: scalar_result,
                    lane: 0,
                    elem,
                },
            ));
        } else {
            let elem = if prefix_kind == X86SsePrefix::OpSize {
                VecElementType::F64
            } else {
                VecElementType::F32
            };
            let src = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: vector,
                        addr,
                        width: VecWidth::V128,
                    },
                ));
                vector
            } else {
                self.xmm(modrm.rm)
            };
            let result = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VUnary {
                    dst: result,
                    src,
                    elem,
                    lanes: VecWidth::V128.lanes(elem) as u8,
                    op: VecUnaryOp::FSqrt,
                },
                X86OpHint::SseOp {
                    prefix: prefix_kind,
                    opcode: 0x51,
                },
            ));
            self.append_legacy_packed_result(dst, result, elem, pc, ctx, &mut ops);
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_fp_to_int(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match prefix.rep_prefix {
            Some(0xF3) => VecElementType::F32,
            Some(0xF2) => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![0x0F, opcode],
                });
            }
        };
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, opcode],
            });
        }
        let int_width = if prefix.rex_w() {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: scalar,
                    addr,
                    width: if elem == VecElementType::F32 {
                        MemWidth::B4
                    } else {
                        MemWidth::B8
                    },
                    sign: SignExtend::Zero,
                },
            ));
            scalar
        } else {
            self.xmm(modrm.rm)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpToInt {
                dst: self.gpr(modrm.reg),
                src,
                elem,
                int_width,
                signed: true,
                truncate: opcode == 0x2C,
                round: if opcode == 0x2C {
                    FpRoundMode::RoundTowardZero
                } else {
                    FpRoundMode::Dynamic
                },
                suppress_exceptions: false,
            },
            X86OpHint::SseOp {
                prefix: if elem == VecElementType::F32 {
                    X86SsePrefix::Rep
                } else {
                    X86SsePrefix::Repne
                },
                opcode,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_int_to_fp(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match prefix.rep_prefix {
            Some(0xF3) => VecElementType::F32,
            Some(0xF2) => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![0x0F, 0x2A],
                });
            }
        };
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, 0x2A],
            });
        }
        let int_width = if prefix.rex_w() {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let mem_width = if int_width == OpWidth::W64 {
            MemWidth::B8
        } else {
            MemWidth::B4
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: value,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Sign,
                },
            ));
            value
        } else {
            self.gpr(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86IntToFp {
                dst,
                merge: dst,
                src,
                elem,
                int_width,
                signed: true,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: false,
            },
            X86OpHint::SseOp {
                prefix: if elem == VecElementType::F32 {
                    X86SsePrefix::Rep
                } else {
                    X86SsePrefix::Repne
                },
                opcode: 0x2A,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_scalar_fp_convert(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, 0x5A],
            });
        }
        if prefix.rep_prefix.is_none() {
            let (from, to, prefix_kind, src_width) = if prefix.operand_size_override {
                (
                    VecElementType::F64,
                    VecElementType::F32,
                    X86SsePrefix::OpSize,
                    VecWidth::V128,
                )
            } else {
                (
                    VecElementType::F32,
                    VecElementType::F64,
                    X86SsePrefix::None,
                    VecWidth::V64,
                )
            };
            let modrm = decode_modrm(bytes, prefix, pc)?;
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let mut ops = Vec::new();
            let src = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
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
            } else {
                self.xmm(modrm.rm)
            };
            let dst = self.xmm(modrm.reg);
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86PackedFpConvert {
                    dst,
                    src,
                    mask: None,
                    from,
                    to,
                    lanes: 2,
                    dst_width: VecWidth::V128,
                    mask_zeroing: false,
                    zero_upper: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    report_fp16_denormal: false,
                },
                X86OpHint::SseOp {
                    prefix: prefix_kind,
                    opcode: 0x5A,
                },
            ));
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        let (from, to, prefix_kind) = match prefix.rep_prefix {
            Some(0xF3) => (VecElementType::F32, VecElementType::F64, X86SsePrefix::Rep),
            Some(0xF2) => (
                VecElementType::F64,
                VecElementType::F32,
                X86SsePrefix::Repne,
            ),
            _ => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: value,
                    addr,
                    width: if from == VecElementType::F32 {
                        MemWidth::B4
                    } else {
                        MemWidth::B8
                    },
                    sign: SignExtend::Zero,
                },
            ));
            value
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpConvert {
                dst,
                merge: dst,
                src,
                mask: None,
                from,
                to,
                mask_zeroing: false,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: false,
            },
            X86OpHint::SseOp {
                prefix: prefix_kind,
                opcode: 0x5A,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_fp_estimate(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = prefix.rep_prefix == Some(0xF3) && !prefix.operand_size_override;
        if prefix.lock
            || prefix.rex2.is_some()
            || prefix.rep_prefix == Some(0xF2)
            || prefix.operand_size_override
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !scalar {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
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
                        width: VecWidth::V128,
                    },
                ));
                vector
            }
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        self.append_fp_estimate_result(
            dst,
            dst,
            src,
            opcode,
            scalar,
            VecWidth::V128,
            true,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_fp_unpack(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rep_prefix.is_some() || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.operand_size_override {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CheckAlignment {
                    addr: addr.clone(),
                    alignment: 16,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Aligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_unpack_shuffle(
            raw,
            dst,
            src2,
            elem,
            VecWidth::V128,
            opcode == 0x15,
            pc,
            ctx,
            &mut ops,
        );
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_fp_compare(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, scalar, prefix_kind) = match (prefix.operand_size_override, prefix.rep_prefix) {
            (false, None) => (VecElementType::F32, false, X86SsePrefix::None),
            (true, None) => (VecElementType::F64, false, X86SsePrefix::OpSize),
            (false, Some(0xF3)) => (VecElementType::F32, true, X86SsePrefix::Rep),
            (false, Some(0xF2)) => (VecElementType::F64, true, X86SsePrefix::Repne),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if bytes.len() <= modrm.bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + modrm.bytes_consumed,
                need: prefix.cursor + modrm.bytes_consumed + 1,
            });
        }
        let predicate = bytes[modrm.bytes_consumed];
        if predicate & !7 != 0 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..=modrm.bytes_consumed].to_vec(),
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            if scalar {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: value,
                        addr,
                        width: if elem == VecElementType::F32 {
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
                        scalar: value,
                        elem,
                        lanes: 1,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: VecWidth::V128,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Aligned),
                ));
            }
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86VectorFpCompare {
                dst,
                src1: dst,
                src2,
                mask: None,
                elem,
                width: VecWidth::V128,
                lanes: if scalar {
                    1
                } else {
                    VecWidth::V128.lanes(elem) as u8
                },
                predicate,
                scalar,
                mask_destination: false,
                zero_upper: false,
                suppress_exceptions: false,
            },
            X86OpHint::SseOp {
                prefix: prefix_kind,
                opcode: 0xC2,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }

    pub(crate) fn lift_sse_round(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
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
        let scalar = matches!(opcode, 0x0A | 0x0B);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + imm_offset,
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
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
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: VecWidth::V128,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Aligned),
                ));
                loaded
            }
        } else {
            self.xmm(modrm.rm)
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
        let dst = self.xmm(modrm.reg);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Round {
                dst,
                merge: dst,
                src,
                elem,
                width: VecWidth::V128,
                lanes: if scalar {
                    1
                } else {
                    VecWidth::V128.lanes(elem) as u8
                },
                scalar_source: scalar,
                zero_upper: false,
                mode,
                suppress_precision: imm & 8 != 0,
            },
        ));
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }

    pub(crate) fn lift_sse_packed_int_fp_convert(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let conversion = match (opcode, prefix.operand_size_override, prefix.rep_prefix) {
            (0x5B, false, None) => Some((true, VecElementType::I32, VecElementType::F32, false)),
            (0x5B, true, None) => Some((false, VecElementType::I32, VecElementType::F32, false)),
            (0x5B, false, Some(0xF3)) => {
                Some((false, VecElementType::I32, VecElementType::F32, true))
            }
            (0xE6, false, Some(0xF3)) => {
                Some((true, VecElementType::I32, VecElementType::F64, false))
            }
            (0xE6, false, Some(0xF2)) => {
                Some((false, VecElementType::I32, VecElementType::F64, false))
            }
            (0xE6, true, None) => Some((false, VecElementType::I32, VecElementType::F64, true)),
            _ => None,
        };
        let Some((int_to_fp, int_elem, fp_elem, truncate)) = conversion else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let (lanes, src_width) = match (int_to_fp, int_elem, fp_elem) {
            (true, VecElementType::I32, VecElementType::F64) => (2, VecWidth::V64),
            (false, VecElementType::I32, VecElementType::F64) => (2, VecWidth::V128),
            _ => (4, VecWidth::V128),
        };
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let kind = if int_to_fp {
            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask: None,
                int_elem,
                fp_elem,
                signed: true,
                lanes,
                src_width,
                dst_width: VecWidth::V128,
                mask_zeroing: false,
                zero_upper: false,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
            }
        } else {
            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask: None,
                fp_elem,
                int_elem,
                signed: true,
                truncate,
                lanes,
                src_width,
                dst_width: VecWidth::V128,
                mask_zeroing: false,
                zero_upper: false,
                round: if truncate {
                    FpRoundMode::RoundTowardZero
                } else {
                    FpRoundMode::Dynamic
                },
                suppress_exceptions: false,
            }
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
