//! String / REP-prefixed instruction lifting

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
    /// Lift MOVS/STOS/LODS/SCAS/CMPS, with or without REP prefixes.
    pub(crate) fn lift_string(
        &self,
        opcode: u8,
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![opcode],
            });
        }
        let kind = match opcode {
            0xA4 | 0xA5 => X86StringKind::Movs,
            0xAA | 0xAB => X86StringKind::Stos,
            0xAC | 0xAD => X86StringKind::Lods,
            0xAE | 0xAF => X86StringKind::Scas,
            0xA6 | 0xA7 => X86StringKind::Cmps,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: vec![opcode],
                });
            }
        };
        let width = if opcode & 1 == 0 {
            MemWidth::B1
        } else {
            self.size_to_memwidth(prefix.op_size())
        };
        let rep = match (kind, prefix.rep_prefix) {
            (_, None) => X86RepMode::None,
            (X86StringKind::Scas | X86StringKind::Cmps, Some(0xF3)) => X86RepMode::Repe,
            (X86StringKind::Scas | X86StringKind::Cmps, Some(0xF2)) => X86RepMode::Repne,
            _ => X86RepMode::Rep,
        };
        let src_segment = if matches!(
            kind,
            X86StringKind::Movs | X86StringKind::Lods | X86StringKind::Cmps
        ) {
            match prefix.segment_override {
                Some(0x64) => Some(VReg::Arch(ArchReg::X86(X86Reg::FsBase))),
                Some(0x65) => Some(VReg::Arch(ArchReg::X86(X86Reg::GsBase))),
                _ => None,
            }
        } else {
            None
        };
        let op = OpKind::X86String {
            kind,
            rep,
            accumulator: self.gpr(0),
            src_index: self.gpr(6),
            dst_index: self.gpr(7),
            count: self.gpr(1),
            src_segment,
            width,
            address_width: if prefix.address_size_override {
                OpWidth::W32
            } else {
                OpWidth::W64
            },
        };
        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, op)],
            prefix.cursor,
        ))
    }
}
