//! x86 debug-register read lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86DebugReg};
use crate::smir::ir::types::OpId;
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Lift `MOV r64, DR0-DR7` (`0F 21 /r`).
    ///
    /// Intel defines ModR/M.mod as ignored, so only the raw ModR/M byte is
    /// consumed: apparent memory encodings have no SIB or displacement. DR4
    /// and DR5 remain represented because CR4.DE selects a dynamic #UD versus
    /// the legacy DR6/DR7 aliases.
    pub(crate) fn lift_read_debug_0f21(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if bytes.is_empty() {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: prefix.cursor,
                need: prefix.cursor + 1,
            });
        }
        if prefix.lock || prefix.rex2.is_some() || prefix.rex_r() != 0 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![bytes[0]],
            });
        }

        let modrm = bytes[0];
        let debug = match (modrm >> 3) & 7 {
            0 => X86DebugReg::Dr0,
            1 => X86DebugReg::Dr1,
            2 => X86DebugReg::Dr2,
            3 => X86DebugReg::Dr3,
            4 => X86DebugReg::Dr4,
            5 => X86DebugReg::Dr5,
            6 => X86DebugReg::Dr6,
            7 => X86DebugReg::Dr7,
            _ => unreachable!("three-bit debug-register selector changed"),
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86ReadDebug {
                    dst: self.gpr((modrm & 7) | prefix.rex_b()),
                    debug,
                },
            )],
            prefix.cursor + 1,
        ))
    }
}
