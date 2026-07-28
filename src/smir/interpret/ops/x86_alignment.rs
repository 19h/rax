//! x86 memory-alignment validation.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{OpKind, SmirOp};

impl SmirInterpreter {
    pub(crate) fn execute_op_x86_alignment(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        match &op.kind {
            OpKind::X86CheckAlignment { addr, alignment } => {
                debug_assert!(alignment.is_power_of_two());
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & (u64::from(*alignment) - 1) != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                }
            }

            OpKind::X86CheckAlignmentAc {
                addr,
                access_size,
                alignment,
                stack_segment,
            } => {
                if !matches!(*access_size, 16 | 32) || *alignment != 16 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let effective_addr = self.compute_address(ctx, addr);
                let Some(x86) = (match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => Some(x86),
                    _ => None,
                }) else {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                };

                if x86.cs_l {
                    let canonical = effective_addr
                        .checked_add(u64::from(*access_size) - 1)
                        .is_some_and(|last| {
                            crate::isa::x86_64::execute::system::is_canonical_48(effective_addr)
                                && crate::isa::x86_64::execute::system::is_canonical_48(last)
                        });
                    if !canonical {
                        ctx.request_exit(if *stack_segment {
                            ExitReason::StackSegment {
                                addr: op.guest_pc,
                                error_code: 0,
                            }
                        } else {
                            ExitReason::GeneralProtection {
                                addr: op.guest_pc,
                                error_code: 0,
                            }
                        });
                        return Ok(());
                    }
                }

                const CR0_AM: u64 = 1 << 18;
                if effective_addr & (u64::from(*alignment) - 1) != 0
                    && x86.cr0 & CR0_AM != 0
                    && x86.cpl == 3
                    && ctx.flags.materialized.ac
                {
                    ctx.request_exit(ExitReason::AlignmentCheck { addr: op.guest_pc });
                }
            }

            _ => return self.execute_op_fp(ctx, memory, op),
        }

        Ok(())
    }
}
