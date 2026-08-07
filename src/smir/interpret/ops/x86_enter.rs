//! Fault-precise interpretation of x86 `ENTER imm16, imm8`.

use crate::isa::x86_64::execute::system::is_canonical_48;
use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::OpWidth;

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

fn write_little_endian(
    memory: &mut dyn SmirMemory,
    address: u64,
    value: u64,
    size: usize,
) -> Result<(), MemoryError> {
    memory.write(address, &value.to_le_bytes()[..size])
}

impl SmirInterpreter {
    pub(crate) fn execute_op_x86_enter(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let OpKind::X86Enter(enter) = &op.kind else {
            return self.execute_op_x86_alignment(ctx, memory, op);
        };

        let valid_shape = op.x86_hint.is_none()
            && enter.nesting_level < 32
            && matches!(enter.width, OpWidth::W16 | OpWidth::W64)
            && matches!(enter.next_pc.checked_sub(op.guest_pc), Some(4..=15));
        let (old_rsp, old_rbp, valid_mode) = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => (
                x86.gpr[4],
                x86.gpr[5],
                x86.efer & (1 << 10) != 0 && x86.cs_l && (!enter.requires_apx || x86.apx_enabled),
            ),
            _ => (0, 0, false),
        };
        if !valid_shape || !valid_mode {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        let size = enter.width.bits() as usize / 8;
        let delta = size as u64;
        let mut stack_pointer = old_rsp.wrapping_sub(delta);
        if !canonical_range(stack_pointer, size) {
            ctx.request_exit(ExitReason::StackSegment {
                addr: op.guest_pc,
                error_code: 0,
            });
            return Ok(());
        }
        write_little_endian(memory, stack_pointer, old_rbp, size)?;
        let frame_pointer = stack_pointer;

        for index in 1..enter.nesting_level {
            let parent_address = old_rbp.wrapping_sub(u64::from(index) * delta);
            if !canonical_range(parent_address, size) {
                ctx.request_exit(ExitReason::StackSegment {
                    addr: op.guest_pc,
                    error_code: 0,
                });
                return Ok(());
            }
            let parent = read_little_endian(memory, parent_address, size)?;
            stack_pointer = stack_pointer.wrapping_sub(delta);
            if !canonical_range(stack_pointer, size) {
                ctx.request_exit(ExitReason::StackSegment {
                    addr: op.guest_pc,
                    error_code: 0,
                });
                return Ok(());
            }
            write_little_endian(memory, stack_pointer, parent, size)?;
        }

        if enter.nesting_level != 0 {
            stack_pointer = stack_pointer.wrapping_sub(delta);
            if !canonical_range(stack_pointer, size) {
                ctx.request_exit(ExitReason::StackSegment {
                    addr: op.guest_pc,
                    error_code: 0,
                });
                return Ok(());
            }
            write_little_endian(memory, stack_pointer, frame_pointer, size)?;
        }

        let final_rsp = stack_pointer.wrapping_sub(u64::from(enter.allocation_size));
        if !canonical_range(final_rsp, 1) {
            ctx.request_exit(ExitReason::StackSegment {
                addr: op.guest_pc,
                error_code: 0,
            });
            return Ok(());
        }
        // Intel SDM Vol. 3C §31.4.4 specifies a write check for the byte at the
        // final stack pointer without an actual data write.
        memory.probe(final_rsp, 1, true)?;

        let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
            unreachable!("validated x86 ENTER context changed")
        };
        x86.gpr[4] = final_rsp;
        x86.gpr[5] = match enter.width {
            OpWidth::W16 => (old_rbp & !0xFFFF) | (frame_pointer & 0xFFFF),
            OpWidth::W64 => frame_pointer,
            _ => unreachable!("validated ENTER width changed"),
        };
        Ok(())
    }
}
