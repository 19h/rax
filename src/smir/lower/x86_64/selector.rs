//! Fault-precise, helper-backed SLDT/STR lowering.

use crate::smir::ir::ops::{
    OpKind, SmirOp, X86SystemSelector, X86SystemSelectorLoadOp, X86SystemSelectorSource,
    X86SystemSelectorStoreOp, X86SystemSelectorTarget,
};
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET,
    X86_GUEST_CR4_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
    X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET, X86_STATE_PTR_AT_RBP,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the exact direct destination emitted by the strict x86 lifter.
/// EGPR destinations and address components necessarily require REX2/APX;
/// REX2 may also encode a legacy register or address.
pub(crate) fn x86_system_selector_store_shape_valid(op: &SmirOp) -> bool {
    let OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        target,
        requires_apx,
        ..
    }) = &op.kind
    else {
        return false;
    };
    if op.x86_hint.is_some() {
        return false;
    }

    match target {
        X86SystemSelectorTarget::Register { dst, width } => {
            let Some(index) = (match dst {
                VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index(),
                _ => None,
            }) else {
                return false;
            };
            matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64)
                && (index < 16 || *requires_apx)
        }
        X86SystemSelectorTarget::Memory { addr } => {
            let uses_egpr = addr
                .regs()
                .iter()
                .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
            addr.is_x86_state_backed_shape() && (!uses_egpr || *requires_apx)
        }
    }
}

/// Validate the strict long-mode LLDT form admitted by native lowering. LTR
/// shares the IR shape but remains fail-closed until its descriptor busy-bit
/// transaction is implemented.
pub(crate) fn x86_system_selector_load_shape_valid(op: &SmirOp) -> bool {
    let OpKind::X86SystemSelectorLoad(X86SystemSelectorLoadOp {
        selector,
        source,
        requires_apx,
        next_pc,
    }) = &op.kind
    else {
        return false;
    };
    if *selector != X86SystemSelector::Ldtr
        || op.x86_hint.is_some()
        || !matches!(next_pc.checked_sub(op.guest_pc), Some(3..=15))
    {
        return false;
    }

    match source {
        X86SystemSelectorSource::Register { src } => {
            let Some(index) = (match src {
                VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index(),
                _ => None,
            }) else {
                return false;
            };
            index < 16 || *requires_apx
        }
        X86SystemSelectorSource::Memory { addr } => {
            let uses_egpr = addr
                .regs()
                .iter()
                .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
            addr.is_x86_state_backed_shape() && (!uses_egpr || *requires_apx)
        }
    }
}

impl X86_64Lowerer {
    /// Emit APX, protected-mode, VM86, and UMIP checks while RAX is the live
    /// `GuestRegs` base. Every failure replays the direct instruction at its
    /// original PC; a disabled UMIP control transfers directly to commit.
    fn emit_x86_system_selector_store_guards(
        &mut self,
        requires_apx: bool,
    ) -> (Vec<usize>, Vec<usize>) {
        let mut fault_branches = Vec::with_capacity(4);
        let mut commit_branches = Vec::with_capacity(1);

        if requires_apx {
            self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+apx],0
            self.code.emit_u32(X86_GUEST_APX_ENABLED_OFFSET as u32);
            self.code.emit_u8(0);
            fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));
        }

        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],PE
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));

        self.code.emit_bytes(&[0x48, 0xF7, 0x80]); // test qword [rax+rflags],VM
        self.code.emit_u32(X86_GUEST_RFLAGS_OFFSET as u32);
        self.code
            .emit_u32(crate::isa::x86_64::flags::bits::VM as u32);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));

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

    /// Read the guest LDTR/TR selector through the owning-vCPU helper and
    /// commit the encoded GPR width or one exact 2-byte MMU-backed store. Host
    /// SLDT/STR are never emitted because they would observe host descriptor
    /// state and host UMIP controls.
    pub(crate) fn emit_x86_system_selector_store(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86SystemSelectorStore requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_system_selector_store_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86SystemSelectorStore".to_string(),
                operand: "requires an unhinted W16/W32/W64 x86 GPR or state-backed memory target, with APX for every EGPR"
                    .to_string(),
            });
        }
        let OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
            selector,
            target,
            requires_apx,
        }) = &op.kind
        else {
            unreachable!("validated X86SystemSelectorStore shape changed")
        };
        if matches!(target, X86SystemSelectorTarget::Memory { .. }) && !self.mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "SLDT/STR memory destination requires JIT MMU helpers".to_string(),
            });
        }

        // A memory form owns one aligned 16-byte value slot across its helper
        // calls. The slot does not alter SysV call alignment.
        if matches!(target, X86SystemSelectorTarget::Memory { .. }) {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }

        // Publish every identity-mapped GPR before borrowing ABI registers.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let (fault_branches, commit_branches) =
            self.emit_x86_system_selector_store_guards(*requires_apx);
        for branch in commit_branches {
            self.patch_rel32_to_current(branch)?;
        }

        self.emit_helper_call_state(PhysReg::Rax, true, self.preserve_vector_system_helpers);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rax, OpWidth::W64);
            emitter.emit_mov_ri(
                PhysReg::Rsi,
                match selector {
                    X86SystemSelector::Ldtr => 0,
                    X86SystemSelector::Tr => 1,
                },
                OpWidth::W32,
            );
        }
        self.code.emit_u8(0xFC); // cld: platform ABI requires DF=0
        self.code.emit_u8(0xFF);
        self.code.emit_u8(0x90); // call qword [rax+system_selector_fn]
        self.code
            .emit_u32(X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET as u32);

        self.code.emit_bytes(&[0x48, 0x8B, 0x4D]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rcx,[rbp+state_ptr]
        self.emit_helper_call_state(PhysReg::Rcx, false, self.preserve_vector_system_helpers);

        match target {
            X86SystemSelectorTarget::Register { dst, width } => {
                let destination = match dst {
                    VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().unwrap(),
                    _ => unreachable!("validated selector-store destination changed"),
                };
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    // The helper returns the selector in RAX, while the shared
                    // state-slot writer uses RAX as its fixed GuestRegs base.
                    // Preserve the value in RDX and restore that base from RCX
                    // before committing the encoded partial/full GPR width.
                    emitter.emit_mov_rr(PhysReg::Rdx, PhysReg::Rax, OpWidth::W64);
                    emitter.emit_mov_rr(PhysReg::Rax, PhysReg::Rcx, OpWidth::W64);
                }
                self.emit_store_gpr_slot_from_reg(destination, PhysReg::Rdx, *width)?;
                if destination == 5 {
                    self.emit_sync_saved_rbp_from_state(PhysReg::Rcx);
                }
                self.emit_reload_all(PhysReg::Rcx);
                self.code.emit_u8(0x9D); // popfq
                self.emit_flag_preserving_stack_pop8();
            }
            X86SystemSelectorTarget::Memory { addr } => {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    // Two snapshots precede the outer selector slot.
                    emitter.emit_mov_mr(PhysReg::Rsp, 16, PhysReg::Rax, OpWidth::W64);
                }
                self.emit_reload_all(PhysReg::Rcx);
                self.code.emit_u8(0x9D);
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

        // Guard failures precede both selector observation and destination
        // commit. Restore the exact pre-instruction image and replay direct.
        for branch in fault_branches {
            self.patch_rel32_to_current(branch)?;
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D);
        self.emit_flag_preserving_stack_pop8();
        if matches!(target, X86SystemSelectorTarget::Memory { .. }) {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }

    /// Load LDTR through the owning vCPU's MMU and descriptor validator. The
    /// operation exits natively after a successful serializing commit; every
    /// dynamic guard, source-memory, or descriptor fault restores the complete
    /// pre-instruction register/flag image and replays direct at `guest_pc`.
    pub(crate) fn emit_x86_system_selector_load(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86SystemSelectorLoad requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !self.mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "X86SystemSelectorLoad requires JIT MMU helpers".to_string(),
            });
        }
        if !x86_system_selector_load_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86SystemSelectorLoad".to_string(),
                operand:
                    "requires an unhinted LLDT source, APX for every EGPR, and an exact next PC"
                        .to_string(),
            });
        }
        let OpKind::X86SystemSelectorLoad(load) = &op.kind else {
            unreachable!("validated X86SystemSelectorLoad shape changed")
        };

        // Publish every identity-mapped GPR before borrowing ABI registers.
        // The two pushes preserve guest flags and maintain SysV call alignment.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let mut guard_faults = Vec::with_capacity(4);
        if load.requires_apx {
            self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+apx],0
            self.code.emit_u32(X86_GUEST_APX_ENABLED_OFFSET as u32);
            self.code.emit_u8(0);
            guard_faults.push(self.emit_jcc_placeholder(X86Cond::E));
        }
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],PE
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        guard_faults.push(self.emit_jcc_placeholder(X86Cond::E));
        self.code.emit_bytes(&[0x48, 0xF7, 0x80]); // test qword [rax+rflags],VM
        self.code.emit_u32(X86_GUEST_RFLAGS_OFFSET as u32);
        self.code
            .emit_u32(crate::isa::x86_64::flags::bits::VM as u32);
        guard_faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        guard_faults.push(self.emit_jcc_placeholder(X86Cond::Ne));

        self.emit_helper_call_state(PhysReg::Rax, true, self.preserve_vector_mem_helpers);
        let memory_source = matches!(&load.source, X86SystemSelectorSource::Memory { .. });
        match &load.source {
            X86SystemSelectorSource::Register { src } => {
                let VReg::Arch(ArchReg::X86(reg)) = src else {
                    unreachable!("validated LLDT register source changed")
                };
                self.emit_struct_mov(
                    PhysReg::Rax,
                    6,
                    i32::from(reg.gpr_index().unwrap()) * 8,
                    false,
                );
            }
            X86SystemSelectorSource::Memory { addr } => {
                self.emit_jit_mem_effective_address(addr, false)?;
            }
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rax, OpWidth::W64);
            let encoding = i64::from(memory_source) | (i64::from(load.requires_apx) << 1);
            emitter.emit_mov_ri(PhysReg::Rdx, encoding, OpWidth::W32);
        }
        self.code.emit_u8(0xFC); // cld: platform ABI requires DF=0
        self.code.emit_u8(0xFF);
        self.code.emit_u8(0x90); // call qword [rax+system_selector_load_fn]
        self.code
            .emit_u32(X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET as u32);

        self.code.emit_bytes(&[0x48, 0x8B, 0x4D]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rcx,[rbp+state_ptr]
        self.code.emit_bytes(&[0x48, 0x85, 0xC0]); // test rax,rax
        let helper_fault = self.emit_jcc_placeholder(X86Cond::E);

        self.emit_x86_serialize();
        self.emit_helper_call_state(PhysReg::Rcx, false, self.preserve_vector_mem_helpers);
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(load.next_pc);

        for branch in guard_faults {
            self.patch_rel32_to_current(branch)?;
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.patch_rel32_to_current(helper_fault)?;
        self.emit_helper_call_state(PhysReg::Rcx, false, self.preserve_vector_mem_helpers);
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D);
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);
        Ok(())
    }
}
