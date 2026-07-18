//! mem.rs

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
}
