//! Canonical, fault-ordered x86 INVPCID interpretation.

use crate::smir::interpret::SmirInterpreter;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{SmirOp, X86InvpcidOp};
use crate::smir::ir::types::{ArchReg, VReg};

impl SmirInterpreter {
    pub(super) fn execute_x86_invpcid(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
        invpcid: &X86InvpcidOp,
    ) -> Result<(), MemoryError> {
        let instruction_len = invpcid.next_pc.checked_sub(op.guest_pc);
        let type_index = match invpcid.invpcid_type {
            VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index(),
            _ => None,
        };
        let uses_egpr = type_index.is_some_and(|index| index >= 16)
            || invpcid
                .addr
                .regs()
                .iter()
                .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
        let minimum_len = if invpcid.requires_apx { 6 } else { 5 };
        if !instruction_len.is_some_and(|len| (minimum_len..=15).contains(&len))
            || type_index.is_none()
            || op.x86_hint.is_some()
            || !invpcid.addr.is_x86_state_backed_shape()
            || (uses_egpr && !invpcid.requires_apx)
        {
            ctx.request_exit(ExitReason::Undefined {
                addr: op.guest_pc,
                opcode: 0,
            });
            return Ok(());
        }

        let cr4 = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => {
                if invpcid.requires_apx && !x86.apx_enabled {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                if x86.cpl != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                    return Ok(());
                }
                x86.cr4
            }
            _ => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: op.guest_pc,
                    opcode: 0,
                });
                return Ok(());
            }
        };

        let effective_addr = self.compute_address(ctx, &invpcid.addr);
        let canonical = effective_addr.checked_add(15).is_some_and(|last| {
            crate::isa::x86_64::execute::system::is_canonical_48(effective_addr)
                && crate::isa::x86_64::execute::system::is_canonical_48(last)
        });
        if !canonical {
            ctx.request_exit(if invpcid.stack_segment {
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

        let mut payload = [0_u8; 16];
        memory.read(effective_addr, &mut payload)?;
        let descriptor_low = u64::from_le_bytes(payload[..8].try_into().unwrap());
        let descriptor_linear = u64::from_le_bytes(payload[8..].try_into().unwrap());
        let invpcid_type = ctx.read_vreg(invpcid.invpcid_type);
        let descriptor = match crate::isa::x86_64::execute::system::validate_x86_invpcid(
            invpcid_type,
            descriptor_low,
            descriptor_linear,
            cr4,
        ) {
            Ok(descriptor) => descriptor,
            Err(()) => {
                ctx.request_exit(ExitReason::GeneralProtection {
                    addr: op.guest_pc,
                    error_code: 0,
                });
                return Ok(());
            }
        };
        memory.invalidate_process_context(invpcid_type as u8, descriptor.pcid, descriptor.linear);
        Ok(())
    }
}
