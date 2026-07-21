//! x86 debug-register transfer lifting.

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::{OpKind, SmirOp, X86DebugReg};
use crate::smir::ir::types::OpId;
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

fn decode_debug_register(selector: u8) -> Option<X86DebugReg> {
    match selector {
        0 => Some(X86DebugReg::Dr0),
        1 => Some(X86DebugReg::Dr1),
        2 => Some(X86DebugReg::Dr2),
        3 => Some(X86DebugReg::Dr3),
        4 => Some(X86DebugReg::Dr4),
        5 => Some(X86DebugReg::Dr5),
        6 => Some(X86DebugReg::Dr6),
        7 => Some(X86DebugReg::Dr7),
        _ => None,
    }
}

impl X86_64Lifter {
    /// Lift `MOV r64, DR0-DR7` (`0F 21 /r`).
    ///
    /// Intel defines ModR/M.mod as ignored, so only the raw ModR/M byte is
    /// consumed: apparent memory encodings have no SIB or displacement. DR4
    /// and DR5 remain represented because CR4.DE selects a dynamic #UD versus
    /// the legacy DR6/DR7 aliases. REX2.B4/B3 extend the destination through
    /// R31; any REX/REX2 R extension selects a nonexistent DR and #UDs.
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
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![bytes[0]],
            });
        }

        let modrm = bytes[0];
        let bytes_consumed = prefix.cursor + 1;
        let Some(debug) = decode_debug_register(((modrm >> 3) & 7) | prefix.rex_r()) else {
            return Ok(LiftResult {
                ops: vec![],
                bytes_consumed,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: vec![],
            });
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
            bytes_consumed,
        ))
    }

    /// Lift `MOV DR0-DR7, r64` (`0F 23 /r`).
    ///
    /// ModR/M.mod is ignored and DR4/DR5 remain encoded so the interpreter or
    /// native guard can apply CR4.DE and legacy aliasing dynamically. The
    /// strict x86-64 lifter always models the long-mode 64-bit source form;
    /// the CPU rejects this operation from compatibility-mode JIT regions.
    /// REX2.B4/B3 extend the source through R31; any REX/REX2 R extension
    /// selects a nonexistent DR and #UDs.
    pub(crate) fn lift_write_debug_0f23(
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
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: vec![bytes[0]],
            });
        }

        let modrm = bytes[0];
        let bytes_consumed = prefix.cursor + 1;
        let Some(debug) = decode_debug_register(((modrm >> 3) & 7) | prefix.rex_r()) else {
            return Ok(LiftResult {
                ops: vec![],
                bytes_consumed,
                control_flow: ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode,
                },
                branch_targets: vec![],
            });
        };
        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86WriteDebug {
                    src: self.gpr((modrm & 7) | prefix.rex_b()),
                    debug,
                },
            )],
            bytes_consumed,
        ))
    }
}
