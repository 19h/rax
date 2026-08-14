//! Fault-precise interpretation of x86 PUSHF/PUSHFQ and POPF/POPFQ.

use crate::isa::x86_64::execute::system::{
    X86StackFlagsFault, X86StackFlagsState, evaluate_x86_pop_flags, evaluate_x86_push_flags,
    is_canonical_48, validate_x86_stack_flags_access,
};
use crate::isa::x86_64::flags;
use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp, X86StackFlagsKind};
use crate::smir::ir::types::OpWidth;

const MATERIALIZED_RFLAGS_MASK: u64 = 0x4_0CD7;
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

fn write_little_endian(
    memory: &mut dyn SmirMemory,
    address: u64,
    value: u64,
    size: usize,
) -> Result<(), MemoryError> {
    memory.write(address, &value.to_le_bytes()[..size])
}

impl SmirInterpreter {
    pub(crate) fn execute_op_x86_stack_flags(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let OpKind::X86StackFlags(stack) = &op.kind else {
            return self.execute_op_x86_leave(ctx, memory, op);
        };

        let minimum_len = if stack.requires_apx { 3 } else { 1 };
        let valid_shape = op.x86_hint.is_none()
            && matches!(stack.width, OpWidth::W16 | OpWidth::W64)
            && stack
                .next_pc
                .checked_sub(op.guest_pc)
                .is_some_and(|len| (minimum_len..=15).contains(&len));
        if !valid_shape {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        ctx.flags.materialize_all();
        let materialized = ctx.flags.materialized.to_rflags();
        let (old_rsp, state, valid_mode) = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => {
                let rflags = (x86.rflags & !MATERIALIZED_RFLAGS_MASK)
                    | (materialized & MATERIALIZED_RFLAGS_MASK);
                (
                    x86.gpr[4],
                    X86StackFlagsState {
                        cr0: x86.cr0,
                        cr4: x86.cr4,
                        rflags,
                        cpl: x86.cpl,
                    },
                    x86.efer & (1 << 10) != 0
                        && x86.cs_l
                        && (!stack.requires_apx || x86.apx_enabled),
                )
            }
            _ => (
                0,
                X86StackFlagsState {
                    cr0: 0,
                    cr4: 0,
                    rflags: 0,
                    cpl: 0,
                },
                false,
            ),
        };
        if !valid_mode {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        let size = stack.width.bits() as usize / 8;
        match validate_x86_stack_flags_access(state, size as u8) {
            Ok(()) => {}
            Err(X86StackFlagsFault::GeneralProtection) => {
                ctx.request_exit(ExitReason::GeneralProtection {
                    addr: op.guest_pc,
                    error_code: 0,
                });
                return Ok(());
            }
            Err(X86StackFlagsFault::InvalidWidth) => unreachable!("validated stack-flags width"),
        }

        let address = match stack.kind {
            X86StackFlagsKind::Push => old_rsp.wrapping_sub(size as u64),
            X86StackFlagsKind::Pop => old_rsp,
        };
        if !canonical_range(address, size) {
            ctx.request_exit(ExitReason::StackSegment {
                addr: op.guest_pc,
                error_code: 0,
            });
            return Ok(());
        }
        if address & (size as u64 - 1) != 0
            && state.cr0 & CR0_AM != 0
            && state.rflags & flags::bits::AC != 0
            && state.cpl == 3
        {
            ctx.request_exit(ExitReason::AlignmentCheck { addr: op.guest_pc });
            return Ok(());
        }

        match stack.kind {
            X86StackFlagsKind::Push => {
                let image = evaluate_x86_push_flags(state, size as u8)
                    .expect("validated PUSHF state and width");
                write_little_endian(memory, address, image, size)?;
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    unreachable!("validated x86 PUSHF context changed")
                };
                x86.gpr[4] = address;
            }
            X86StackFlagsKind::Pop => {
                let popped = read_little_endian(memory, address, size)?;
                let new_rflags = match evaluate_x86_pop_flags(state, size as u8, popped) {
                    Ok(rflags) => rflags,
                    Err(X86StackFlagsFault::GeneralProtection) => {
                        ctx.request_exit(ExitReason::GeneralProtection {
                            addr: op.guest_pc,
                            error_code: 0,
                        });
                        return Ok(());
                    }
                    Err(X86StackFlagsFault::InvalidWidth) => {
                        unreachable!("validated POPF width")
                    }
                };
                let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
                    unreachable!("validated x86 POPF context changed")
                };
                x86.gpr[4] = old_rsp.wrapping_add(size as u64);
                x86.rflags = new_rflags;
                ctx.flags.materialized = MaterializedFlags::from_rflags(new_rflags);
                ctx.flags.lazy = None;
            }
        }
        Ok(())
    }
}
