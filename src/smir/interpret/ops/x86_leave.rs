//! Fault-precise interpretation of long-mode x86 `LEAVE`.

use crate::isa::x86_64::execute::system::is_canonical_48;
use crate::isa::x86_64::flags;
use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp, X86LeaveWidth};

const CR0_AM: u64 = 1 << 18;

fn canonical_range(address: u64, size: usize) -> bool {
    address
        .checked_add(size as u64 - 1)
        .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last))
}

fn read_little_endian(
    memory: &mut dyn SmirMemory,
    address: u64,
    size: usize,
) -> Result<u64, MemoryError> {
    let mut bytes = [0_u8; 8];
    memory.read(address, &mut bytes[..size])?;
    Ok(u64::from_le_bytes(bytes))
}

impl SmirInterpreter {
    pub(crate) fn execute_op_x86_leave(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let OpKind::X86Leave(leave) = &op.kind else {
            return self.execute_op_x86_enter(ctx, memory, op);
        };

        let minimum_len = if leave.requires_apx { 3 } else { 1 };
        let valid_shape = op.x86_hint.is_none()
            && leave
                .next_pc
                .checked_sub(op.guest_pc)
                .is_some_and(|len| (minimum_len..=15).contains(&len));
        let (frame, valid_mode) = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => (
                x86.gpr[5],
                x86.efer & (1 << 10) != 0 && x86.cs_l && (!leave.requires_apx || x86.apx_enabled),
            ),
            _ => (0, false),
        };
        if !valid_shape || !valid_mode {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        let size = usize::from(leave.width.bytes());
        if !canonical_range(frame, size) {
            ctx.request_exit(ExitReason::StackSegment {
                addr: op.guest_pc,
                error_code: 0,
            });
            return Ok(());
        }
        let ArchRegState::X86_64(x86) = &ctx.arch_regs else {
            unreachable!("validated x86 LEAVE context changed")
        };
        if frame & (size as u64 - 1) != 0
            && x86.cr0 & CR0_AM != 0
            && x86.rflags & flags::bits::AC != 0
            && x86.cpl == 3
        {
            ctx.request_exit(ExitReason::AlignmentCheck { addr: op.guest_pc });
            return Ok(());
        }

        let value = read_little_endian(memory, frame, size)?;
        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!("validated x86 LEAVE context changed")
        };
        x86.gpr[4] = frame.wrapping_add(size as u64);
        x86.gpr[5] = match leave.width {
            X86LeaveWidth::W16 => (frame & !0xFFFF) | (value & 0xFFFF),
            X86LeaveWidth::W64 => value,
        };
        Ok(())
    }
}
