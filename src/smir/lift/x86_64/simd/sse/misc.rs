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
                suppress_exceptions: false,
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
}
