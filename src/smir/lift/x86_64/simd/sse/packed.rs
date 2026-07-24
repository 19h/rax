//! packed.rs

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
    /// Lift packed and scalar legacy SSE/SSE2 floating-point arithmetic.
    pub(crate) fn lift_sse_packed_arith(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, opcode],
            });
        }
        let operation =
            x86_fp_binary_operation(opcode).ok_or_else(|| LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, opcode],
            })?;
        let prefix_kind = if prefix.rep_prefix == Some(0xF3) {
            X86SsePrefix::Rep
        } else if prefix.rep_prefix == Some(0xF2) {
            X86SsePrefix::Repne
        } else if prefix.operand_size_override {
            X86SsePrefix::OpSize
        } else {
            X86SsePrefix::None
        };
        if matches!(prefix_kind, X86SsePrefix::Rep | X86SsePrefix::Repne) {
            let elem = if prefix_kind == X86SsePrefix::Rep {
                VecElementType::F32
            } else {
                VecElementType::F64
            };
            let mem_width = if elem == VecElementType::F32 {
                MemWidth::B4
            } else {
                MemWidth::B8
            };
            let modrm = decode_modrm(bytes, prefix, pc)?;
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let mut ops = Vec::new();
            let dst = self.xmm(modrm.reg);
            let src2 = if modrm.is_memory {
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
                        width: mem_width,
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
            let kind = OpKind::X86FpBinary {
                dst: vector_result,
                src1: dst,
                src2,
                mask: None,
                elem,
                lanes: 1,
                op: operation,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
            };
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                X86OpHint::SseOp {
                    prefix: prefix_kind,
                    opcode,
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
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        let elem = match prefix_kind {
            X86SsePrefix::None => VecElementType::F32,
            X86SsePrefix::OpSize => VecElementType::F64,
            X86SsePrefix::Rep | X86SsePrefix::Repne => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: tmp,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            tmp
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let lanes = VecWidth::V128.lanes(elem) as u8;
        let raw = ctx.alloc_vreg();
        let kind = OpKind::X86FpBinary {
            dst: raw,
            src1: dst,
            src2,
            mask: None,
            elem,
            lanes,
            op: operation,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::SseOp {
                prefix: prefix_kind,
                opcode,
            },
        ));
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift MMX/XMM wrapping and saturating packed integer add/subtract.
    pub(crate) fn lift_sse_packed_add_sub(
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
        let mmx = !prefix.operand_size_override;
        let width = if mmx { VecWidth::V64 } else { VecWidth::V128 };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let elem = match opcode {
            0xD8 | 0xDC | 0xE8 | 0xEC | 0xF8 | 0xFC => VecElementType::I8,
            0xD9 | 0xDD | 0xE9 | 0xED | 0xF9 | 0xFD => VecElementType::I16,
            0xFA | 0xFE => VecElementType::I32,
            0xD4 | 0xFB => VecElementType::I64,
            _ => unreachable!(),
        };
        let saturating = matches!(
            opcode,
            0xD8 | 0xD9 | 0xDC | 0xDD | 0xE8 | 0xE9 | 0xEC | 0xED
        );
        let subtract = matches!(
            opcode,
            0xD8 | 0xD9 | 0xE8 | 0xE9 | 0xF8 | 0xF9 | 0xFA | 0xFB
        );
        let signed = matches!(opcode, 0xE8 | 0xE9 | 0xEC | 0xED);

        let hint = X86OpHint::SseOp {
            prefix: if mmx {
                X86SsePrefix::None
            } else {
                X86SsePrefix::OpSize
            },
            opcode,
        };

        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        if modrm.is_memory {
            let x86_addr = modrm.addr.as_ref().unwrap();
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            ops.extend(pre_ops);

            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: tmp,
                    addr,
                    width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            if mmx {
                // A faulting memory source must not enter MMX state.
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                ));
            }
            let kind = if saturating {
                OpKind::VAddSubSat {
                    dst,
                    src1: dst,
                    src2: tmp,
                    elem,
                    lanes: width.lanes(elem) as u8,
                    subtract,
                    signed,
                }
            } else if subtract {
                OpKind::VSub {
                    dst,
                    src1: dst,
                    src2: tmp,
                    elem,
                    lanes: width.lanes(elem) as u8,
                }
            } else {
                OpKind::VAdd {
                    dst,
                    src1: dst,
                    src2: tmp,
                    elem,
                    lanes: width.lanes(elem) as u8,
                }
            };
            ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
        } else {
            let src2 = if mmx {
                self.mm(modrm.rm)
            } else {
                self.xmm(modrm.rm)
            };
            if mmx {
                ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                ));
            }
            let kind = if saturating {
                OpKind::VAddSubSat {
                    dst,
                    src1: dst,
                    src2,
                    elem,
                    lanes: width.lanes(elem) as u8,
                    subtract,
                    signed,
                }
            } else if subtract {
                OpKind::VSub {
                    dst,
                    src1: dst,
                    src2,
                    elem,
                    lanes: width.lanes(elem) as u8,
                }
            } else {
                OpKind::VAdd {
                    dst,
                    src1: dst,
                    src2,
                    elem,
                    lanes: width.lanes(elem) as u8,
                }
            };
            ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift MMX/SSE2 PUNPCKL*/PUNPCKH* packed-integer interleaves.
    pub(crate) fn lift_sse_integer_unpack(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mmx_opcode = matches!(opcode, 0x60 | 0x61 | 0x62 | 0x68 | 0x69 | 0x6A);
        let mmx = !prefix.operand_size_override && mmx_opcode;
        if (!prefix.operand_size_override && !mmx)
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
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
        let width = if mmx { VecWidth::V64 } else { VecWidth::V128 };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            if mmx && !high {
                // MMX PUNPCKL* memory sources are m32 even though register
                // sources address a complete 64-bit MM register.
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
                        elem: VecElementType::I64,
                        lanes: 1,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width,
                    },
                ));
            }
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        if mmx {
            // A faulting memory source must not enter MMX state.
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        }
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        let result = if modrm.is_memory && !mmx {
            ctx.alloc_vreg()
        } else {
            dst
        };
        self.append_integer_interleave(
            result,
            dst,
            src2,
            elem,
            width,
            high,
            X86OpHint::SseOp {
                prefix: if mmx {
                    X86SsePrefix::None
                } else {
                    X86SsePrefix::OpSize
                },
                opcode,
            },
            pc,
            &mut ops,
        );
        if modrm.is_memory && !mmx {
            self.append_legacy_packed_result(dst, result, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_addsub_horizontal(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = if prefix.rep_prefix == Some(0xF2) && !prefix.operand_size_override {
            VecElementType::F32
        } else if prefix.rep_prefix.is_none() && prefix.operand_size_override {
            VecElementType::F64
        } else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        if prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_fp_addsub_horizontal(
            raw,
            dst,
            src2,
            opcode,
            elem,
            VecWidth::V128,
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

    /// Lift legacy MMX/XMM PACKSSWB/PACKUSWB/PACKSSDW/PACKUSDW.
    pub(crate) fn lift_sse_integer_pack(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mmx_opcode = matches!(opcode, 0x63 | 0x67 | 0x6B);
        let mmx = !prefix.operand_size_override && mmx_opcode;
        if (!prefix.operand_size_override && !mmx)
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
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
        let dst_elem = if src_elem == VecElementType::I16 {
            VecElementType::I8
        } else {
            VecElementType::I16
        };
        let width = if mmx { VecWidth::V64 } else { VecWidth::V128 };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
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
                    width,
                },
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        if mmx {
            // A faulting memory source must not enter MMX state.
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        }
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        let raw = if modrm.is_memory && !mmx {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let src_lanes = width.lanes(src_elem) as u8;
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPackSat {
                dst: raw,
                // VPackSat places src2 before src1; x86 places its first
                // (destructive destination) operand before the r/m operand.
                src1: src2,
                src2: dst,
                src_elem,
                to_unsigned: matches!(opcode, 0x67 | 0x2B),
                src_lanes,
                block_lanes: src_lanes,
            },
            X86OpHint::SseOp {
                prefix: if mmx {
                    X86SsePrefix::None
                } else {
                    X86SsePrefix::OpSize
                },
                opcode,
            },
        ));
        if modrm.is_memory && !mmx {
            self.append_legacy_packed_result(dst, raw, dst_elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_psign(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let elem = match opcode {
            0x08 => VecElementType::I8,
            0x09 => VecElementType::I16,
            0x0A => VecElementType::I32,
            _ => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let control = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1: dst,
                    src2: control,
                    elem,
                    lanes: VecWidth::V64.lanes(elem) as u8,
                    op: VLaneOp::Sign,
                    signed: true,
                    set_ovf: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else if modrm.is_memory {
            let raw = ctx.alloc_vreg();
            self.append_packed_sign(raw, dst, control, elem, VecWidth::V128, pc, ctx, &mut ops);
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1: dst,
                    src2: control,
                    elem,
                    lanes: VecWidth::V128.lanes(elem) as u8,
                    op: VLaneOp::Sign,
                    signed: true,
                    set_ovf: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_pabs(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let elem = match opcode {
            0x1C => VecElementType::I8,
            0x1D => VecElementType::I16,
            0x1E => VecElementType::I32,
            _ => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        let raw = if !mmx && modrm.is_memory {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let width = if mmx { VecWidth::V64 } else { VecWidth::V128 };
        let abs = OpKind::VUnary {
            dst: raw,
            src,
            elem,
            lanes: width.lanes(elem) as u8,
            op: VecUnaryOp::Abs,
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                abs,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                abs,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode,
                },
            ));
        }
        if !mmx && modrm.is_memory {
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_packed_extend(
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
        let (src_elem, dst_elem, signed) = Self::packed_extend_shape(opcode);
        let lanes = VecWidth::V128.lanes(dst_elem) as u8;
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            self.append_packed_extend_memory_source(addr, src_elem, lanes, None, pc, ctx, &mut ops)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_packed_extend(
            raw, src, src_elem, dst_elem, lanes, signed, pc, ctx, &mut ops,
        );
        self.append_legacy_packed_result(dst, raw, dst_elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_packed_minmax(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let original = matches!(opcode, 0xDA | 0xDE | 0xEA | 0xEE);
        let mmx = original && !prefix.operand_size_override;
        if (!prefix.operand_size_override && !original)
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, min, signed) = Self::packed_minmax_shape(opcode, false);
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1: dst,
                    src2,
                    elem,
                    lanes: VecWidth::V64.lanes(elem) as u8,
                    op: if min { VLaneOp::Min } else { VLaneOp::Max },
                    signed,
                    set_ovf: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else if modrm.is_memory {
            let raw = ctx.alloc_vreg();
            self.append_packed_minmax(
                raw,
                dst,
                src2,
                elem,
                VecWidth::V128,
                min,
                signed,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1: dst,
                    src2,
                    elem,
                    lanes: VecWidth::V128.lanes(elem) as u8,
                    op: if min { VLaneOp::Min } else { VLaneOp::Max },
                    signed,
                    set_ovf: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_packed_average(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let elem = if opcode == 0xE0 {
            VecElementType::I8
        } else {
            VecElementType::I16
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(dst, dst, src2, VecWidth::V64, elem),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else if modrm.is_memory {
            // Keep the computation detached from the architectural destination
            // until the aligned source load has completed successfully.
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(raw, dst, src2, VecWidth::V128, elem),
            ));
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(dst, dst, src2, VecWidth::V128, elem),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_phminposuw(
        &self,
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
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
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
        if modrm.is_memory {
            // Retain the explicit aligned load and generic reduction so a
            // fault cannot partially update the architectural destination.
            let raw = ctx.alloc_vreg();
            self.append_phminposuw(raw, src, pc, ctx, &mut ops);
            self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86Phminposuw { dst, src },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x41,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_mpsadbw(
        &self,
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
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + imm_offset,
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
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
        if modrm.is_memory {
            // Keep the memory form detached so alignment/load faults occur
            // before any architectural destination state is modified.
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMpsadbw {
                    dst: raw,
                    src1: dst,
                    src2,
                    mask: None,
                    width: VecWidth::V128,
                    imm: bytes[imm_offset],
                    zeroing: false,
                },
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMpsadbw {
                    dst,
                    src1: dst,
                    src2,
                    mask: None,
                    width: VecWidth::V128,
                    imm: bytes[imm_offset],
                    zeroing: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x42,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }

    pub(crate) fn lift_sse_psadbw(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VSadBytes {
                    dst,
                    src1: dst,
                    src2,
                    width: VecWidth::V64,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xF6,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else if modrm.is_memory {
            // Keep the computation detached from the architectural destination
            // until the aligned source load has completed successfully.
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VSadBytes {
                    dst: raw,
                    src1: dst,
                    src2,
                    width: VecWidth::V128,
                },
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VSadBytes {
                    dst,
                    src1: dst,
                    src2,
                    width: VecWidth::V128,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF6,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_packed_shift_count(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let (elem, shift, _) = Self::packed_shift_count_spec(opcode, false, false).unwrap();
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let width = if mmx { VecWidth::V64 } else { VecWidth::V128 };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let count_vec = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if !mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86CheckAlignment {
                        addr: addr.clone(),
                        alignment: 16,
                    },
                ));
            }
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width,
                },
                X86OpHint::VecAlign(if mmx {
                    X86VecAlign::Unaligned
                } else {
                    X86VecAlign::Aligned
                }),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let count = if modrm.is_memory {
            let count = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: count,
                    vec: count_vec,
                    lane: 0,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            count
        } else {
            count_vec
        };
        let dst = if mmx {
            self.mm(modrm.reg)
        } else {
            self.xmm(modrm.reg)
        };
        let raw = if mmx { dst } else { ctx.alloc_vreg() };
        let kind = OpKind::X86PackedShift {
            dst: raw,
            src: dst,
            count,
            width,
            elem,
            shift,
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                },
            ));
        } else {
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        }
        if mmx {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else {
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    pub(crate) fn lift_sse_packed_shift_imm(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.rep_prefix.is_some() || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let group = (modrm.byte >> 3) & 7;
        let (elem, shift, byte_lane) = match (opcode, group) {
            (0x71, 2) => (VecElementType::I16, ShiftOp::Lsr, false),
            (0x71, 4) => (VecElementType::I16, ShiftOp::Asr, false),
            (0x71, 6) => (VecElementType::I16, ShiftOp::Lsl, false),
            (0x72, 2) => (VecElementType::I32, ShiftOp::Lsr, false),
            (0x72, 4) => (VecElementType::I32, ShiftOp::Asr, false),
            (0x72, 6) => (VecElementType::I32, ShiftOp::Lsl, false),
            (0x73, 2) => (VecElementType::I64, ShiftOp::Lsr, false),
            (0x73, 3) => (VecElementType::I8, ShiftOp::Lsr, true),
            (0x73, 6) => (VecElementType::I64, ShiftOp::Lsl, false),
            (0x73, 7) => (VecElementType::I8, ShiftOp::Lsl, true),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        // PSRLDQ/PSLLDQ (/3 and /7) have no prefix-free MMX encoding.
        if mmx && byte_lane {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let dst = if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let raw = if mmx { dst } else { ctx.alloc_vreg() };
        let kind = OpKind::X86PackedShiftImm {
            dst: raw,
            src: dst,
            width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
            elem,
            shift,
            amount: bytes[imm_offset],
            byte_lane,
        };
        let mut ops = if mmx {
            vec![SmirOp::with_hint(
                OpId(0),
                pc,
                kind,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode,
                },
            )]
        } else {
            vec![SmirOp::new(OpId(0), pc, kind)]
        };
        if mmx {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else {
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }
}
