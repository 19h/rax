//! SSE / SSE2 / SSE3 / SSSE3 / SSE4 instruction lifting

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

    /// Lift MMX MOVQ and legacy SSE packed moves (0F 6F/7F and related forms).
    pub(crate) fn lift_sse_mov(
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
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let prefix_kind = if prefix.rep_prefix == Some(0xF3) {
            X86SsePrefix::Rep
        } else if prefix.rep_prefix == Some(0xF2) {
            X86SsePrefix::Repne
        } else if prefix.operand_size_override {
            X86SsePrefix::OpSize
        } else {
            X86SsePrefix::None
        };

        let hint = X86OpHint::SseMov {
            prefix: prefix_kind,
            opcode,
        };

        if prefix_kind == X86SsePrefix::None && matches!(opcode, 0x6F | 0x7F) {
            if prefix.rex2.is_some() {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let reg = self.mm(modrm.reg & 0x07);
            if opcode == 0x6F {
                if modrm.is_memory {
                    let (addr, pre_ops) =
                        self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                    ops.extend(pre_ops);
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: reg,
                            addr,
                            width: VecWidth::V64,
                        },
                        hint,
                    ));
                    // A faulting load must not enter MMX state.
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::X86X87Control {
                            kind: X86X87ControlKind::EnterMmx,
                            addr: None,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(0),
                        pc,
                        OpKind::X86X87Control {
                            kind: X86X87ControlKind::EnterMmx,
                            addr: None,
                        },
                    ));
                    ops.push(SmirOp::with_hint(
                        OpId(1),
                        pc,
                        OpKind::VMov {
                            dst: reg,
                            src: self.mm(modrm.rm & 0x07),
                            width: VecWidth::V64,
                        },
                        hint,
                    ));
                }
            } else if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VStore {
                        src: reg,
                        addr,
                        width: VecWidth::V64,
                    },
                    hint,
                ));
                // A faulting store must not enter MMX state.
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                ));
                ops.push(SmirOp::with_hint(
                    OpId(1),
                    pc,
                    OpKind::VMov {
                        dst: self.mm(modrm.rm & 0x07),
                        src: reg,
                        width: VecWidth::V64,
                    },
                    hint,
                ));
            }
            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }

        if matches!(prefix_kind, X86SsePrefix::Rep | X86SsePrefix::Repne)
            && matches!(opcode, 0x10 | 0x11)
        {
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
            let dst = self.xmm(modrm.reg);

            if opcode == 0x10 {
                if modrm.is_memory {
                    let (addr, pre_ops) =
                        self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                    ops.extend(pre_ops);
                    let scalar = ctx.alloc_vreg();
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
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst,
                            vec: dst,
                            scalar,
                            elem,
                            lane: 0,
                        },
                        hint,
                    ));
                    let xmm_lanes = if elem == VecElementType::F32 { 4 } else { 2 };
                    for lane in 1..xmm_lanes {
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VInsertLane {
                                dst,
                                vec: dst,
                                scalar: zero,
                                lane,
                                elem,
                            },
                        ));
                    }
                } else {
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VExtractLane {
                            dst: scalar,
                            vec: self.xmm(modrm.rm),
                            lane: 0,
                            elem,
                            sign: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst,
                            vec: dst,
                            scalar,
                            lane: 0,
                            elem,
                        },
                        hint,
                    ));
                }
            } else if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: dst,
                        lane: 0,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Store {
                        src: scalar,
                        addr,
                        width: mem_width,
                    },
                    hint,
                ));
            } else {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: dst,
                        lane: 0,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                let rm = self.xmm(modrm.rm);
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: rm,
                        vec: rm,
                        scalar,
                        lane: 0,
                        elem,
                    },
                    hint,
                ));
            }

            return Ok(LiftResult::fallthrough(
                ops,
                prefix.cursor + modrm.bytes_consumed,
            ));
        }
        if prefix_kind == X86SsePrefix::Repne
            || prefix_kind == X86SsePrefix::Rep && !matches!(opcode, 0x6F | 0x7F)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, opcode],
            });
        }

        match opcode {
            0x10 | 0x28 | 0x6F => {
                if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);
                    if matches!(opcode, 0x28)
                        || opcode == 0x6F && prefix_kind == X86SsePrefix::OpSize
                    {
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::X86CheckAlignment {
                                addr: addr.clone(),
                                alignment: 16,
                            },
                        ));
                    }
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: self.xmm(modrm.reg),
                            addr,
                            width: VecWidth::V128,
                        },
                        hint,
                    ));
                } else {
                    ops.push(SmirOp::with_hint(
                        OpId(0),
                        pc,
                        OpKind::VMov {
                            dst: self.xmm(modrm.reg),
                            src: self.xmm(modrm.rm),
                            width: VecWidth::V128,
                        },
                        hint,
                    ));
                }
            }
            0x11 | 0x29 | 0x7F => {
                if modrm.is_memory {
                    let x86_addr = modrm.addr.as_ref().unwrap();
                    let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
                    ops.extend(pre_ops);
                    if matches!(opcode, 0x29)
                        || opcode == 0x7F && prefix_kind == X86SsePrefix::OpSize
                    {
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::X86CheckAlignment {
                                addr: addr.clone(),
                                alignment: 16,
                            },
                        ));
                    }
                    ops.push(SmirOp::with_hint(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VStore {
                            src: self.xmm(modrm.reg),
                            addr,
                            width: VecWidth::V128,
                        },
                        hint,
                    ));
                } else {
                    ops.push(SmirOp::with_hint(
                        OpId(0),
                        pc,
                        OpKind::VMov {
                            dst: self.xmm(modrm.rm),
                            src: self.xmm(modrm.reg),
                            width: VecWidth::V128,
                        },
                        hint,
                    ));
                }
            }
            _ => {}
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift legacy MOVD/MOVQ transfers between MMX/XMM and GPR/memory operands
    /// (0F 6E/7E and 66 0F 6E/7E respectively).
    pub(crate) fn lift_sse_movd_q(
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
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let (elem, op_width, mem_width) = if prefix.rex_w() {
            (VecElementType::I64, OpWidth::W64, MemWidth::B8)
        } else {
            (VecElementType::I32, OpWidth::W32, MemWidth::B4)
        };
        let hint = X86OpHint::SseOp {
            prefix: if mmx {
                X86SsePrefix::None
            } else {
                X86SsePrefix::OpSize
            },
            opcode,
        };

        if !modrm.is_memory {
            let (dst, src, zero_upper) = if opcode == 0x6E {
                (
                    if mmx {
                        self.mm(modrm.reg)
                    } else {
                        self.xmm(modrm.reg)
                    },
                    self.gpr(modrm.rm),
                    false,
                )
            } else {
                (
                    self.gpr(modrm.rm),
                    if mmx {
                        self.mm(modrm.reg)
                    } else {
                        self.xmm(modrm.reg)
                    },
                    false,
                )
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
            }
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86MovdQ {
                    dst,
                    src,
                    width: op_width,
                    zero_upper,
                },
                hint,
            ));
        } else if opcode == 0x6E {
            let scalar = {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
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
                scalar
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
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86MovdQ {
                        dst: self.mm(modrm.reg),
                        src: scalar,
                        width: op_width,
                        zero_upper: false,
                    },
                    hint,
                ));
            } else {
                self.append_scalar_zeroed_xmm_result(
                    self.xmm(modrm.reg),
                    scalar,
                    elem,
                    false,
                    pc,
                    ctx,
                    &mut ops,
                );
            }
        } else {
            let scalar = ctx.alloc_vreg();
            if mmx {
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86MovdQ {
                        dst: scalar,
                        src: self.mm(modrm.reg),
                        width: op_width,
                        zero_upper: false,
                    },
                    hint,
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: self.xmm(modrm.reg),
                        lane: 0,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: scalar,
                    addr,
                    width: mem_width,
                },
            ));
            if mmx {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                ));
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift the scalar vector MOVQ forms: F3 0F 7E loads an XMM register from
    /// an XMM/m64 operand, while 66 0F D6 stores an XMM register to XMM/m64.
    /// Register destinations have bits 127:64 cleared; legacy encodings retain
    /// the shared architectural backing state above bit 127.
    pub(crate) fn lift_sse_movq_vec(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let valid_prefix = match opcode {
            0x7E => prefix.rep_prefix == Some(0xF3) && !prefix.operand_size_override,
            0xD6 => prefix.rep_prefix.is_none() && prefix.operand_size_override,
            _ => false,
        };
        if !valid_prefix || prefix.lock || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();

        if opcode == 0x7E {
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
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
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: self.xmm(modrm.rm),
                        lane: 0,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            };
            self.append_scalar_zeroed_xmm_result(
                self.xmm(modrm.reg),
                scalar,
                VecElementType::I64,
                false,
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
                    vec: self.xmm(modrm.reg),
                    lane: 0,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
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
                self.append_scalar_zeroed_xmm_result(
                    self.xmm(modrm.rm),
                    scalar,
                    VecElementType::I64,
                    false,
                    pc,
                    ctx,
                    &mut ops,
                );
            }
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


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
            let kind = match opcode {
                0x58 => OpKind::VAdd {
                    dst: vector_result,
                    src1: dst,
                    src2,
                    elem,
                    lanes: 1,
                },
                0x59 => OpKind::VMul {
                    dst: vector_result,
                    src1: dst,
                    src2,
                    elem,
                    lanes: 1,
                },
                0x5C => OpKind::VSub {
                    dst: vector_result,
                    src1: dst,
                    src2,
                    elem,
                    lanes: 1,
                },
                0x5E => OpKind::VDiv {
                    dst: vector_result,
                    src1: dst,
                    src2,
                    elem,
                    lanes: 1,
                },
                0x5D | 0x5F => OpKind::VX86MinMax {
                    dst: vector_result,
                    src1: dst,
                    src2,
                    elem,
                    lanes: 1,
                    min: opcode == 0x5D,
                },
                _ => unreachable!(),
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
        let kind = match opcode {
            0x58 => OpKind::VAdd {
                dst,
                src1: dst,
                src2,
                elem,
                lanes,
            },
            0x59 => OpKind::VMul {
                dst,
                src1: dst,
                src2,
                elem,
                lanes,
            },
            0x5C => OpKind::VSub {
                dst,
                src1: dst,
                src2,
                elem,
                lanes,
            },
            0x5E => OpKind::VDiv {
                dst,
                src1: dst,
                src2,
                elem,
                lanes,
            },
            0x5D | 0x5F => OpKind::VX86MinMax {
                dst,
                src1: dst,
                src2,
                elem,
                lanes,
                min: opcode == 0x5D,
            },
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![0x0F, opcode],
                });
            }
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
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


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


    pub(crate) fn lift_sse_comi(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rep_prefix.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, opcode],
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
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpCompare {
                src1: self.xmm(modrm.reg),
                src2,
                elem,
                signaling: opcode == 0x2F,
            },
            X86OpHint::SseOp {
                prefix: if elem == VecElementType::F64 {
                    X86SsePrefix::OpSize
                } else {
                    X86SsePrefix::None
                },
                opcode,
            },
        ));
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


    /// Lift packed legacy SSE/SSE2 bitwise AND/AND-NOT/OR/XOR.
    pub(crate) fn lift_sse_logic(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rep_prefix.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![0x0F, opcode],
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: tmp,
                    addr,
                    width: VecWidth::V128,
                },
            ));
            tmp
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let kind = match opcode {
            0x54 => OpKind::VAnd {
                dst,
                src1: dst,
                src2,
                width: VecWidth::V128,
            },
            0x55 => OpKind::VAndNot {
                dst,
                src1: dst,
                src2,
                width: VecWidth::V128,
            },
            0x56 => OpKind::VOr {
                dst,
                src1: dst,
                src2,
                width: VecWidth::V128,
            },
            0x57 => OpKind::VXor {
                dst,
                src1: dst,
                src2,
                width: VecWidth::V128,
            },
            _ => unreachable!(),
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::SseOp {
                prefix: if prefix.operand_size_override {
                    X86SsePrefix::OpSize
                } else {
                    X86SsePrefix::None
                },
                opcode,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift legacy MOVMSKPS/MOVMSKPD (0F 50 /r).
    pub(crate) fn lift_sse_movmask(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rep_prefix.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }
        let elem = if prefix.operand_size_override {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        let lanes = if elem == VecElementType::F32 { 4 } else { 2 };
        let ops = vec![SmirOp::with_hint(
            OpId(0),
            pc,
            OpKind::X86MovMask {
                dst: self.gpr(modrm.reg),
                src: self.xmm(modrm.rm),
                elem,
                lanes,
                dst_width: if prefix.rex_w() {
                    OpWidth::W64
                } else {
                    OpWidth::W32
                },
            },
            X86OpHint::SseOp {
                prefix: if prefix.operand_size_override {
                    X86SsePrefix::OpSize
                } else {
                    X86SsePrefix::None
                },
                opcode: 0x50,
            },
        )];
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_pmovmskb(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rep_prefix.is_some() || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let mut ops = Vec::with_capacity(if mmx { 2 } else { 1 });
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
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86MovMask {
                dst: self.gpr(modrm.reg),
                src: if mmx {
                    self.mm(modrm.rm)
                } else {
                    self.xmm(modrm.rm)
                },
                elem: VecElementType::I8,
                lanes: if mmx { 8 } else { 16 },
                dst_width: if mmx && prefix.rex_w() {
                    OpWidth::W64
                } else {
                    OpWidth::W32
                },
            },
            X86OpHint::SseOp {
                prefix: if mmx {
                    X86SsePrefix::None
                } else {
                    X86SsePrefix::OpSize
                },
                opcode: 0xD7,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift legacy LDDQU xmm, m128 (F2 0F F0 /r).
    pub(crate) fn lift_sse_lddqu(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock || prefix.rep_prefix != Some(0xF2) || prefix.operand_size_override {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VLoad {
                dst: self.xmm(modrm.reg),
                addr,
                width: VecWidth::V128,
            },
            X86OpHint::SseMov {
                prefix: X86SsePrefix::Repne,
                opcode: 0xF0,
            },
        ));
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift MMX/XMM PAND/PANDN/POR/PXOR (0F DB/DF/EB/EF).
    pub(crate) fn lift_sse_integer_logic(
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
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: value,
                    addr,
                    width,
                },
            ));
            value
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
        let kind = match opcode {
            0xDB => OpKind::VAnd {
                dst,
                src1: dst,
                src2,
                width,
            },
            0xDF => OpKind::VAndNot {
                dst,
                src1: dst,
                src2,
                width,
            },
            0xEB => OpKind::VOr {
                dst,
                src1: dst,
                src2,
                width,
            },
            0xEF => OpKind::VXor {
                dst,
                src1: dst,
                src2,
                width,
            },
            _ => unreachable!(),
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            kind,
            X86OpHint::SseOp {
                prefix: if mmx {
                    X86SsePrefix::None
                } else {
                    X86SsePrefix::OpSize
                },
                opcode,
            },
        ));
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


    /// Lift MMX/SSE2/SSE4.1 packed integer equality and signed greater-than
    /// comparisons.
    pub(crate) fn lift_sse_integer_compare(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mmx_opcode = matches!(opcode, 0x64 | 0x65 | 0x66 | 0x74 | 0x75 | 0x76);
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

        let (elem, cond) = match opcode {
            0x64 => (VecElementType::I8, VecCmpCond::Gt),
            0x65 => (VecElementType::I16, VecCmpCond::Gt),
            0x66 => (VecElementType::I32, VecCmpCond::Gt),
            0x74 => (VecElementType::I8, VecCmpCond::Eq),
            0x75 => (VecElementType::I16, VecCmpCond::Eq),
            0x76 => (VecElementType::I32, VecCmpCond::Eq),
            0x29 => (VecElementType::I64, VecCmpCond::Eq),
            0x37 => (VecElementType::I64, VecCmpCond::Gt),
            _ => unreachable!(),
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
        let result = if modrm.is_memory && !mmx {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: result,
                src1: dst,
                src2,
                cond,
                elem,
                lanes: width.lanes(elem) as u8,
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
            self.append_legacy_packed_result(dst, result, elem, pc, ctx, &mut ops);
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


    pub(crate) fn lift_sse_half_move(
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
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let store = matches!(opcode, 0x13 | 0x17);
        if (store && !modrm.is_memory)
            || (!store && prefix.operand_size_override && !modrm.is_memory)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let lane = if matches!(opcode, 0x16 | 0x17) { 1 } else { 0 };
        let mut ops = Vec::new();
        if store {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: self.xmm(modrm.reg),
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
            let dst = self.xmm(modrm.reg);
            let (source, source_lane) = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
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
                (scalar, None)
            } else {
                (self.xmm(modrm.rm), Some(if opcode == 0x12 { 1 } else { 0 }))
            };
            let scalar = if let Some(source_lane) = source_lane {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: source_lane,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                source
            };
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
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_movnt(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mmx = opcode == 0xE7 && !prefix.operand_size_override;
        let valid_prefix = match opcode {
            0x2B => prefix.rep_prefix.is_none(),
            0xE7 => prefix.rep_prefix.is_none(),
            _ => false,
        };
        if prefix.lock || prefix.rex2.is_some() || !valid_prefix {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
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
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VStore {
                src: if mmx {
                    self.mm(modrm.reg)
                } else {
                    self.xmm(modrm.reg)
                },
                addr,
                width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
            },
            X86OpHint::VecAlign(if mmx {
                X86VecAlign::Unaligned
            } else {
                X86VecAlign::Aligned
            }),
        ));
        if mmx {
            // A faulting store must not enter MMX state.
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        }
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


    pub(crate) fn lift_sse_duplicate_move(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let (elem, high) = match (opcode, prefix.operand_size_override, prefix.rep_prefix) {
            (0x12, false, Some(0xF3)) => (VecElementType::F32, false),
            (0x16, false, Some(0xF3)) => (VecElementType::F32, true),
            (0x12, false, Some(0xF2)) => (VecElementType::F64, false),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
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
        let src = if modrm.is_memory && elem == VecElementType::F64 {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
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
                    elem,
                    lanes: 2,
                },
            ));
            vector
        } else if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_duplicate_shuffle(raw, src, VecWidth::V128, elem, high, pc, ctx, &mut ops);
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_two_source_shuffle_imm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match (prefix.operand_size_override, prefix.rep_prefix) {
            (false, None) => VecElementType::F32,
            (true, None) => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.lock || prefix.rex2.is_some() {
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
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
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
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        let dst = self.xmm(modrm.reg);
        let raw = ctx.alloc_vreg();
        self.append_two_source_shuffle_imm(
            raw,
            dst,
            src2,
            VecWidth::V128,
            elem,
            bytes[imm_offset],
            pc,
            ctx,
            &mut ops,
        );
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
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


    pub(crate) fn lift_sse_packed_shuffle_imm(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let prefix_kind = match (prefix.operand_size_override, prefix.rep_prefix) {
            (true, None) => X86SsePrefix::OpSize,
            (false, Some(0xF3)) => X86SsePrefix::Rep,
            (false, Some(0xF2)) => X86SsePrefix::Repne,
            (false, None) => X86SsePrefix::None,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let mmx = prefix_kind == X86SsePrefix::None;
        if prefix.lock || prefix.rex2.is_some() {
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
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: if mmx { VecWidth::V64 } else { VecWidth::V128 },
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else if mmx {
            self.mm(modrm.rm)
        } else {
            self.xmm(modrm.rm)
        };
        let (elem, high_words) = match prefix_kind {
            X86SsePrefix::None => (VecElementType::I16, None),
            X86SsePrefix::OpSize => (VecElementType::I32, None),
            X86SsePrefix::Rep => (VecElementType::I16, Some(true)),
            X86SsePrefix::Repne => (VecElementType::I16, Some(false)),
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
                OpKind::X86PackedShuffleImm {
                    dst,
                    src,
                    width: VecWidth::V64,
                    elem,
                    imm: bytes[imm_offset],
                    high_words,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x70,
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
            let raw = ctx.alloc_vreg();
            self.append_packed_shuffle_imm(
                raw,
                src,
                VecWidth::V128,
                elem,
                bytes[imm_offset],
                high_words,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed + 1,
        ))
    }


    /// Lift legacy SSSE3 PSHUFB with MMX or XMM operands.
    pub(crate) fn lift_sse_pshufb(
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
        let raw = if !mmx && modrm.is_memory {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let shuffle = OpKind::VByteShuffle {
            dst: raw,
            src: dst,
            control,
            lanes: if mmx { 8 } else { 16 },
            block_lanes: if mmx { 8 } else { 16 },
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                shuffle,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x00,
                },
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                shuffle,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x00,
                },
            ));
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
        } else if modrm.is_memory {
            self.append_legacy_packed_result(dst, raw, VecElementType::I8, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_horizontal_integer(
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
        let elem = if matches!(opcode, 0x02 | 0x06) {
            VecElementType::I32
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
        let raw = if !mmx && modrm.is_memory {
            ctx.alloc_vreg()
        } else {
            dst
        };
        let lanes = if mmx {
            VecWidth::V64.lanes(elem) as u8
        } else {
            VecWidth::V128.lanes(elem) as u8
        };
        let horizontal = OpKind::VHorizontalBin {
            dst: raw,
            src1: dst,
            src2,
            elem,
            lanes,
            block_lanes: lanes,
            subtract: matches!(opcode, 0x05..=0x07),
            saturating: matches!(opcode, 0x03 | 0x07),
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                horizontal,
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
                horizontal,
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


    pub(crate) fn lift_sse_pmaddubsw(
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
                OpKind::VDotProduct {
                    dst,
                    acc: VReg::Imm(0),
                    src1: dst,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V64,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x04,
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
            // Keep the computation detached from the architectural destination:
            // the aligned source read must fault before PMADDUBSW changes XMM1,
            // and the generic vector write would otherwise clear its legacy
            // YMM/ZMM backing state above bit 127.
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VDotProduct {
                    dst: raw,
                    acc: VReg::Imm(0),
                    src1: dst,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V128,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        } else {
            // A zero immediate is also the canonical all-zero vector for the
            // interpreter. Keeping the register form atomic lets strict native
            // admission reproduce the original SSSE3 instruction exactly.
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VDotProduct {
                    dst,
                    acc: VReg::Imm(0),
                    src1: dst,
                    src2,
                    mask: None,
                    src_elem: VecElementType::I8,
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V128,
                    src1_unsigned: true,
                    saturate: true,
                    zeroing: false,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x04,
                },
            ));
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


    pub(crate) fn lift_sse_pmulhrsw(
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
                OpKind::VMulShiftSat {
                    dst,
                    src1: dst,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes: 4,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x0B,
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMulShiftSat {
                    dst: raw,
                    src1: dst,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes: 8,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMulShiftSat {
                    dst,
                    src1: dst,
                    src2,
                    src_elem: VecElementType::I16,
                    lanes: 8,
                    signed1: true,
                    signed2: true,
                    shift_left: 0,
                    round: true,
                    sat_bits: 0,
                    out_shift: 15,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0B,
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


    pub(crate) fn lift_sse_variable_blend(
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
        let elem = match opcode {
            0x10 => VecElementType::I8,
            0x14 => VecElementType::I32,
            0x15 => VecElementType::I64,
            _ => unreachable!(),
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
        self.append_variable_blend(
            raw,
            dst,
            src2,
            self.xmm(0),
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


    pub(crate) fn lift_sse_pmuldq(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let mmx = !signed && !prefix.operand_size_override;
        if (!mmx && !prefix.operand_size_override)
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
            self.append_pmuldq(dst, dst, src2, VecWidth::V64, false, pc, ctx, &mut ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        } else {
            let raw = ctx.alloc_vreg();
            self.append_pmuldq(raw, dst, src2, VecWidth::V128, signed, pc, ctx, &mut ops);
            self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_movntdqa(
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
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
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
        self.append_legacy_packed_result(
            self.xmm(modrm.reg),
            loaded,
            VecElementType::I64,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_ptest(
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
        let second = if modrm.is_memory {
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
        self.append_ptest_flags(
            self.xmm(modrm.reg),
            second,
            VecWidth::V128,
            None,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    /// Lift SSE4.1 PMULLD (66 0F 38 40)
    pub(crate) fn lift_sse_pmulld(
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
        let mut ops = Vec::new();
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;

        let dst = self.xmm(modrm.reg);
        let hint = X86OpHint::SseOp {
            prefix: X86SsePrefix::OpSize,
            opcode: 0x40,
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
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMul {
                    dst,
                    src1: dst,
                    src2: tmp,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                hint,
            ));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(0),
                pc,
                OpKind::VMul {
                    dst,
                    src1: dst,
                    src2: self.xmm(modrm.rm),
                    elem: VecElementType::I32,
                    lanes: 4,
                },
                hint,
            ));
        }

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_pmullw(
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
        let kind = OpKind::VMul {
            dst,
            src1: dst,
            src2,
            elem: VecElementType::I16,
            lanes: if mmx { 4 } else { 8 },
        };
        if mmx {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xD5,
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
                kind,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xD5,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_pmul_high_word(
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
                Self::pmul_high_word_kind(dst, dst, src2, VecWidth::V64, opcode == 0xE5),
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
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(raw, dst, src2, VecWidth::V128, opcode == 0xE5),
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(dst, dst, src2, VecWidth::V128, opcode == 0xE5),
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


    pub(crate) fn lift_sse_pmaddwd(
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
                Self::pmaddwd_kind(dst, dst, src2, VecWidth::V64),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xF5,
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
                Self::pmaddwd_kind(raw, dst, src2, VecWidth::V128),
            ));
            self.append_legacy_packed_result(dst, raw, VecElementType::I32, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(dst, dst, src2, VecWidth::V128),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xF5,
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


    pub(crate) fn lift_sse_gfni(
        &self,
        opcode: u8,
        bytes: &[u8],
        prefix: &X86Prefix,
        affine: bool,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.lock
            || prefix.rex2.is_some()
            || (affine && !matches!(opcode, 0xCE | 0xCF))
            || (!affine && opcode != 0xCF)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if affine && bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + bytes.len(),
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let bytes_consumed = prefix.cursor + imm_offset + usize::from(affine);
        let next_pc = pc + bytes_consumed as u64;
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
        let raw = if affine {
            self.append_gf2p8_affine_vector(
                dst,
                src2,
                VecWidth::V128,
                bytes[imm_offset],
                opcode == 0xCF,
                pc,
                ctx,
                &mut ops,
            )
        } else {
            self.append_gf2p8_mul_vector(dst, src2, VecWidth::V128, pc, ctx, &mut ops)
        };
        self.append_legacy_packed_result(dst, raw, VecElementType::I8, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }


    pub(crate) fn lift_sse_maskmovdqu(
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
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }
        let mmx = !prefix.operand_size_override;
        let mut ops = Vec::new();
        self.append_maskmov(
            if mmx {
                self.mm(modrm.reg)
            } else {
                self.xmm(modrm.reg)
            },
            if mmx {
                self.mm(modrm.rm)
            } else {
                self.xmm(modrm.rm)
            },
            if mmx { 8 } else { 16 },
            prefix.address_size_override,
            prefix.segment_override,
            pc,
            ctx,
            &mut ops,
        );
        if mmx {
            // Place the architectural state transition after every predicated
            // store: earlier active bytes may be visible when a later byte
            // faults, while the fault still suppresses the register-state
            // commit of the instruction as a whole.
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
            ));
        }
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_palignr(
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
        if bytes.len() <= modrm.bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + modrm.bytes_consumed,
                need: prefix.cursor + modrm.bytes_consumed + 1,
            });
        }
        let imm = bytes[modrm.bytes_consumed];
        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64 + 1;
        let mut ops = Vec::new();
        let low = if modrm.is_memory {
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
                OpKind::X86PackedAlignRight {
                    dst,
                    high: dst,
                    low,
                    width: VecWidth::V64,
                    amount: imm,
                },
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x0F,
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
            let raw = ctx.alloc_vreg();
            self.append_align_right(raw, dst, low, VecWidth::V128, imm, pc, ctx, &mut ops);
            self.append_legacy_packed_result(dst, raw, VecElementType::I8, pc, ctx, &mut ops);
        }
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


    pub(crate) fn lift_sse_immediate_blend(
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
        let (elem, repeat_128) = match opcode {
            0x0C => (VecElementType::I32, false),
            0x0D => (VecElementType::I64, false),
            0x0E => (VecElementType::I16, true),
            _ => unreachable!(),
        };
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
        self.append_immediate_blend(
            raw,
            dst,
            src2,
            elem,
            VecWidth::V128,
            imm,
            repeat_128,
            pc,
            ctx,
            &mut ops,
        );
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }


    pub(crate) fn lift_sse_pclmulqdq(
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
        let imm = bytes[imm_offset];
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
        let raw = ctx.alloc_vreg();
        self.append_pclmulqdq(raw, dst, src2, VecWidth::V128, imm, pc, ctx, &mut ops);
        self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
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


    pub(crate) fn lift_sse_dot_product(
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
        let elem = if opcode == 0x40 {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
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
        let raw = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86DotProduct {
                dst: raw,
                src1: dst,
                src2,
                elem,
                width: VecWidth::V128,
                imm: bytes[imm_offset],
            },
        ));
        self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
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


    pub(crate) fn lift_sse_extract_0f3a(
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
        let (elem, lane_mask, mem_width, op_width) = match opcode {
            0x14 => (VecElementType::I8, 0x0F, MemWidth::B1, OpWidth::W32),
            0x15 => (VecElementType::I16, 0x07, MemWidth::B2, OpWidth::W32),
            0x16 if prefix.rex_w() => (VecElementType::I64, 0x01, MemWidth::B8, OpWidth::W64),
            0x16 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            0x17 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            _ => unreachable!(),
        };
        let modrm = decode_modrm(bytes, prefix, pc)?;
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + imm_offset,
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let lane = bytes[imm_offset] & lane_mask;
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let addr = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            Some(addr)
        } else {
            None
        };
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: scalar,
                vec: self.xmm(modrm.reg),
                lane,
                elem,
                sign: SignExtend::Zero,
            },
        ));
        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: scalar,
                    addr,
                    width: mem_width,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.rm),
                    src: SrcOperand::Reg(scalar),
                    width: op_width,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }


    pub(crate) fn lift_sse_pinsrw_pextrw(
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
        let modrm = decode_modrm(bytes, prefix, pc)?;
        if opcode == 0xC5 && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor + imm_offset,
                need: prefix.cursor + imm_offset + 1,
            });
        }
        let mmx = !prefix.operand_size_override;
        let lane = bytes[imm_offset] & if mmx { 0x03 } else { 0x07 };
        let next_pc = pc + prefix.cursor as u64 + imm_offset as u64 + 1;
        let mut ops = Vec::new();

        if mmx {
            let hint = X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode,
            };
            if opcode == 0xC5 {
                // REX.R extends the GPR destination, while REX.B is ignored
                // for the three-bit MM source register.
                ops.push(SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                ));
                ops.push(SmirOp::with_hint(
                    OpId(1),
                    pc,
                    OpKind::VExtractLane {
                        dst: self.gpr(modrm.reg),
                        vec: self.mm(modrm.rm & 0x07),
                        lane,
                        elem: VecElementType::I16,
                        sign: SignExtend::Zero,
                    },
                    hint,
                ));
            } else {
                let scalar = if modrm.is_memory {
                    let (addr, pre_ops) =
                        self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
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
                    // REX.B extends the GPR source, while REX.R is ignored
                    // for the three-bit MM destination register.
                    self.gpr(modrm.rm)
                };
                let dst = self.mm(modrm.reg & 0x07);
                // A faulting memory source must not enter MMX state.
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                ));
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst,
                        vec: dst,
                        scalar,
                        lane,
                        elem: VecElementType::I16,
                    },
                    hint,
                ));
            }
            return Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1));
        }

        if opcode == 0xC5 {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: self.xmm(modrm.rm),
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
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
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
            let dst = self.xmm(modrm.reg);
            let raw = ctx.alloc_vreg();
            self.append_insert_scalar_lane(
                raw,
                dst,
                scalar,
                VecElementType::I16,
                lane,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, VecElementType::I16, pc, ctx, &mut ops);
        }

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }


    pub(crate) fn lift_sse_insert_0f3a(
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
        let dst = self.xmm(modrm.reg);

        if opcode == 0x21 {
            let inserted = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
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
                scalar
            } else {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: self.xmm(modrm.rm),
                        lane: (imm >> 6) & 0x03,
                        elem: VecElementType::I32,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            };
            let raw = ctx.alloc_vreg();
            self.append_insertps(
                raw,
                dst,
                inserted,
                (imm >> 4) & 0x03,
                imm & 0x0F,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, VecElementType::I32, pc, ctx, &mut ops);
        } else {
            let (elem, lane_mask, mem_width) = match opcode {
                0x20 => (VecElementType::I8, 0x0F, MemWidth::B1),
                0x22 if prefix.rex_w() => (VecElementType::I64, 0x01, MemWidth::B8),
                0x22 => (VecElementType::I32, 0x03, MemWidth::B4),
                _ => unreachable!(),
            };
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) =
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
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
                scalar
            } else {
                self.gpr(modrm.rm)
            };
            let raw = ctx.alloc_vreg();
            self.append_insert_scalar_lane(
                raw,
                dst,
                scalar,
                elem,
                imm & lane_mask,
                pc,
                ctx,
                &mut ops,
            );
            self.append_legacy_packed_result(dst, raw, elem, pc, ctx, &mut ops);
        }

        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
    }


    pub(crate) fn lift_sse_aes_round(
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
        let (op, src2) = match opcode {
            0xDB => (X86AesOp::InvMixColumns, None),
            0xDC => (X86AesOp::Enc, Some(src)),
            0xDD => (X86AesOp::EncLast, Some(src)),
            0xDE => (X86AesOp::Dec, Some(src)),
            0xDF => (X86AesOp::DecLast, Some(src)),
            _ => unreachable!(),
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Aes {
                dst: raw,
                src1: if opcode == 0xDB { src } else { dst },
                src2,
                width: VecWidth::V128,
                op,
                imm: 0,
            },
        ));
        self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }


    pub(crate) fn lift_sse_aes_keygen(
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
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Aes {
                dst: raw,
                src1: src,
                src2: None,
                width: VecWidth::V128,
                op: X86AesOp::KeygenAssist,
                imm: bytes[imm_offset],
            },
        ));
        self.append_legacy_packed_result(dst, raw, VecElementType::I64, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, prefix.cursor + imm_offset + 1))
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
        if mmx && !modrm.is_memory {
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
