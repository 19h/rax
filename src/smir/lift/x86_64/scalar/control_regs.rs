//! x86 control-register read/write lifting.

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::{OpKind, SmirOp, X86ControlReg};
use crate::smir::ir::types::OpId;
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Lift `MOV r64, CR0/CR2/CR3/CR4/CR8` (`0F 20 /r`).
    ///
    /// Intel defines the ModR/M.mod field as ignored for this instruction, so
    /// only the raw ModR/M byte is consumed: apparent memory forms do not carry
    /// a SIB or displacement. REX2.R4/R3 extend the control selector and
    /// REX2.B4/B3 extend the destination through R31. Reserved control-register
    /// numbers are guaranteed #UDs and therefore become explicit invalid-opcode
    /// traps rather than unsupported frontiers.
    pub(crate) fn lift_read_control_0f20(
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
        let control = match ((modrm >> 3) & 7) | prefix.rex_r() {
            0 => Some(X86ControlReg::Cr0),
            2 => Some(X86ControlReg::Cr2),
            3 => Some(X86ControlReg::Cr3),
            4 => Some(X86ControlReg::Cr4),
            8 => Some(X86ControlReg::Cr8),
            _ => None,
        };
        let bytes_consumed = prefix.cursor + 1;
        let Some(control) = control else {
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
                OpKind::X86ReadControl {
                    dst: self.gpr((modrm & 7) | prefix.rex_b()),
                    control,
                },
            )],
            bytes_consumed,
        ))
    }

    /// Lift `MOV CR0/CR2/CR3/CR4/CR8, r64` (`0F 22 /r`).
    ///
    /// The strict x86-64 source model always has the 64-bit operand form. The
    /// direct decoder retains the architecturally distinct r32 behavior for
    /// compatibility/legacy mode. ModR/M.mod is ignored and therefore cannot
    /// introduce a SIB, displacement, or memory access. REX2.R4/R3 extend the
    /// control selector and REX2.B4/B3 extend the source through R31.
    pub(crate) fn lift_write_control_0f22(
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
        let control = match ((modrm >> 3) & 7) | prefix.rex_r() {
            0 => Some(X86ControlReg::Cr0),
            2 => Some(X86ControlReg::Cr2),
            3 => Some(X86ControlReg::Cr3),
            4 => Some(X86ControlReg::Cr4),
            8 => Some(X86ControlReg::Cr8),
            _ => None,
        };
        let bytes_consumed = prefix.cursor + 1;
        let Some(control) = control else {
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
                OpKind::X86WriteControl {
                    src: self.gpr((modrm & 7) | prefix.rex_b()),
                    control,
                    next_pc: pc.wrapping_add(bytes_consumed as u64),
                },
            )],
            bytes_consumed,
        ))
    }
}
