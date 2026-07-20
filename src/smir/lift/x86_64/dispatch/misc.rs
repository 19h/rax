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
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl X86_64Lifter {
    pub(crate) fn lift_vex_evex(
        &self,
        pc: u64,
        bytes: &[u8],
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let prefix = match bytes.first().copied() {
            Some(0x62) => decode_evex_prefix(bytes, pc)?,
            _ => decode_vex_prefix(bytes, pc)?,
        };

        if prefix.map == X86VecMap::Map0F
            && bytes.get(prefix.bytes) == Some(&0x01)
            && bytes.get(prefix.bytes + 1) == Some(&0xC5)
        {
            // PCONFIG has only the NP legacy encoding. VEX and EVEX forms of
            // the same opcode/ModR/M sequence are invalid and raise #UD before
            // any vector or PCONFIG architectural state can be observed.
            return Ok(LiftResult {
                ops: Vec::new(),
                bytes_consumed: prefix.bytes + 2,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: Vec::new(),
            });
        }

        self.lift_vec_opcode(prefix, bytes, pc, ctx)
    }
}
