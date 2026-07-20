//! Fault-precise, state-backed lowering for x86 debug-register reads.

use crate::smir::ir::ops::{OpKind, SmirOp, X86DebugReg};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
    X86_GUEST_DR0_OFFSET, X86_GUEST_DR1_OFFSET, X86_GUEST_DR2_OFFSET, X86_GUEST_DR3_OFFSET,
    X86_GUEST_DR6_OFFSET, X86_GUEST_DR7_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the exact 64-bit GPR/debug-register shape emitted by the strict
/// x86-64 lifter. APX EGPRs are excluded because MOV-from-DR has no REX2 form.
pub(crate) fn x86_read_debug_shape_valid(kind: &OpKind) -> bool {
    let OpKind::X86ReadDebug { dst, debug } = kind else {
        return false;
    };
    matches!(
        dst,
        VReg::Arch(ArchReg::X86(reg)) if reg.gpr_index().is_some_and(|index| index < 16)
    ) && matches!(
        debug,
        X86DebugReg::Dr0
            | X86DebugReg::Dr1
            | X86DebugReg::Dr2
            | X86DebugReg::Dr3
            | X86DebugReg::Dr4
            | X86DebugReg::Dr5
            | X86DebugReg::Dr6
            | X86DebugReg::Dr7
    )
}

impl X86_64Lowerer {
    /// Read guest debug state through `GuestRegs`; never execute the host's
    /// privileged `MOV r64, DRn` instruction.
    ///
    /// Dynamic CPL, CR4.DE, and DR7.GD failures restore every GPR and RFLAGS
    /// bit before handing off at the original guest PC. The direct interpreter
    /// then delivers the exact #GP(0), #UD, or #DB and its DR6/DR7 side effects.
    pub(crate) fn emit_x86_read_debug(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86ReadDebug requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_read_debug_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86ReadDebug".to_string(),
                operand: "requires one legacy x86 GPR destination and DR0-DR7".to_string(),
            });
        }
        let OpKind::X86ReadDebug { dst, debug } = &op.kind else {
            unreachable!("validated X86ReadDebug shape changed")
        };
        let destination = match dst {
            VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().unwrap(),
            _ => unreachable!("validated X86ReadDebug destination changed"),
        };
        let debug_offset = match debug {
            X86DebugReg::Dr0 => X86_GUEST_DR0_OFFSET,
            X86DebugReg::Dr1 => X86_GUEST_DR1_OFFSET,
            X86DebugReg::Dr2 => X86_GUEST_DR2_OFFSET,
            X86DebugReg::Dr3 => X86_GUEST_DR3_OFFSET,
            X86DebugReg::Dr4 | X86DebugReg::Dr6 => X86_GUEST_DR6_OFFSET,
            X86DebugReg::Dr5 | X86DebugReg::Dr7 => X86_GUEST_DR7_OFFSET,
        };

        // Publish identity-mapped GPRs before using RAX/RDX/RCX as scratch.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let mut faults = Vec::with_capacity(3);

        // Real-address mode (CR0.PE=0) permits the read. Otherwise effective
        // CPL must be zero; GuestRegs.cpl maps virtual-8086 execution to CPL3.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],1
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        let real_mode = self.emit_jcc_placeholder(X86Cond::E);

        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.patch_rel32_to_current(real_mode)?;

        // DR4/DR5 are aliases only while CR4.DE is clear.
        if matches!(debug, X86DebugReg::Dr4 | X86DebugReg::Dr5) {
            self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr4],DE
            self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
            self.code.emit_u32(1 << 3);
            faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        }

        // General detect faults before any debug-register value is published.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+dr7],GD
        self.code.emit_u32(X86_GUEST_DR7_OFFSET as u32);
        self.code.emit_u32(1 << 13);
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, debug_offset, OpWidth::W64);
        }
        self.emit_store_gpr_slot_from_reg(destination, PhysReg::Rdx, OpWidth::W64)?;

        // Preserve the state pointer before an RBP destination updates the
        // prologue's saved guest-RBP word, then resume with exact flags.
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        if destination == 5 {
            self.emit_sync_saved_rbp_from_state(PhysReg::Rcx);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // Fault path is completely non-committing. Direct replay owns DR6.BD
        // and DR7.GD exception-delivery changes.
        for fault in faults {
            self.patch_rel32_to_current(fault)?;
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
