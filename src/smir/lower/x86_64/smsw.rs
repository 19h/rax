//! Fault-precise, state-backed SMSW lowering.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SmswOp, X86SmswTarget};
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET,
    X86_GUEST_CR4_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the direct architectural destination emitted by the x86 lifter.
/// EGPR destinations and address components necessarily require REX2/APX;
/// REX2 may also encode a legacy register or address.
pub(crate) fn x86_smsw_shape_valid(kind: &OpKind) -> bool {
    let OpKind::X86Smsw(X86SmswOp {
        target,
        requires_apx,
    }) = kind
    else {
        return false;
    };

    match target {
        X86SmswTarget::Register { dst, width } => {
            let Some(index) = (match dst {
                VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index(),
                _ => None,
            }) else {
                return false;
            };
            matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                && (index < 16 || *requires_apx)
        }
        X86SmswTarget::Memory { addr } => {
            let uses_egpr = addr
                .regs()
                .iter()
                .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
            addr.is_x86_state_backed_shape() && (!uses_egpr || *requires_apx)
        }
    }
}

impl X86_64Lowerer {
    /// Emit the dynamic checks shared by both SMSW destinations while RAX is
    /// the `GuestRegs` base. Returned branches select either the common fault
    /// path or the commit point immediately following the guards.
    fn emit_x86_smsw_guards(&mut self, requires_apx: bool) -> (Vec<usize>, Vec<usize>) {
        let mut fault_branches = Vec::with_capacity(2);
        let mut commit_branches = Vec::with_capacity(2);

        // Prefix availability has priority over the instruction's UMIP check.
        if requires_apx {
            self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+apx],0
            self.code.emit_u32(X86_GUEST_APX_ENABLED_OFFSET as u32);
            self.code.emit_u8(0);
            fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));
        }

        // Real-address mode has effective CPL 0. Otherwise SMSW is blocked
        // only when CR4.UMIP=1 and effective CPL is nonzero.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],1
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        commit_branches.push(self.emit_jcc_placeholder(X86Cond::E));

        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr4],UMIP
        self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
        self.code.emit_u32(1 << 11);
        commit_branches.push(self.emit_jcc_placeholder(X86Cond::E));

        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));

        (fault_branches, commit_branches)
    }

    /// Read guest CR0 and commit the architecturally selected GPR width or an
    /// exact 2-byte MMU-backed store. Host SMSW is never emitted because it
    /// would observe host CR0 and host privilege state.
    pub(crate) fn emit_x86_smsw(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86Smsw requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_smsw_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86Smsw".to_string(),
                operand: "requires a W16/W32/W64 x86 GPR or state-backed memory target, with APX for every EGPR"
                    .to_string(),
            });
        }
        let OpKind::X86Smsw(X86SmswOp {
            target,
            requires_apx,
        }) = &op.kind
        else {
            unreachable!("validated X86Smsw shape changed")
        };
        if matches!(target, X86SmswTarget::Memory { .. }) && !self.mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "SMSW memory destination requires JIT MMU helpers".to_string(),
            });
        }

        // A memory form owns one aligned 16-byte slot across its helper call.
        if matches!(target, X86SmswTarget::Memory { .. }) {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }

        // Publish every identity-mapped GPR before borrowing RAX/RDX/RCX.
        // RSP/RBP/EGPR values already reside in their canonical state slots.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let (fault_branches, commit_branches) = self.emit_x86_smsw_guards(*requires_apx);
        for branch in commit_branches {
            self.patch_rel32_to_current(branch)?;
        }

        match target {
            X86SmswTarget::Register { dst, width } => {
                let destination = match dst {
                    VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().unwrap(),
                    _ => unreachable!("validated X86Smsw destination changed"),
                };
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, X86_GUEST_CR0_OFFSET, *width);
                }
                self.emit_store_gpr_slot_from_reg(destination, PhysReg::Rdx, *width)?;

                self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
                if destination == 5 {
                    self.emit_sync_saved_rbp_from_state(PhysReg::Rcx);
                }
                self.emit_reload_all(PhysReg::Rcx);
                self.code.emit_u8(0x9D); // popfq
                self.emit_flag_preserving_stack_pop8();
            }
            X86SmswTarget::Memory { addr } => {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rm(
                        PhysReg::Rdx,
                        PhysReg::Rax,
                        X86_GUEST_CR0_OFFSET,
                        OpWidth::W64,
                    );
                    // At this point two snapshots precede the outer slot.
                    emitter.emit_mov_mr(PhysReg::Rsp, 16, PhysReg::Rdx, OpWidth::W64);
                }
                self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
                self.emit_reload_all(PhysReg::Rcx);
                self.code.emit_u8(0x9D); // popfq
                self.emit_flag_preserving_stack_pop8();

                self.emit_jit_mem_op(
                    op.guest_pc,
                    false,
                    None,
                    None,
                    None,
                    None,
                    Some(16),
                    addr,
                    MemWidth::B2,
                    SignExtend::Zero,
                    16,
                )?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
            }
        }

        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // Guard failures precede both the GPR write and MMU helper call. Restore
        // the pre-instruction image and restart at SMSW for #UD or #GP(0).
        for branch in fault_branches {
            self.patch_rel32_to_current(branch)?;
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D);
        self.emit_flag_preserving_stack_pop8();
        if matches!(target, X86SmswTarget::Memory { .. }) {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
