//! Fault-precise, state-backed CLTS lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::lower::{
    LowerError, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET, X86_STATE_PTR_AT_RBP,
};

use super::{X86_64Lowerer, X86Cond};

/// CLTS has no explicit operands. Keep a target-specific predicate so native
/// admission remains fail-closed if the IR representation later changes.
pub(crate) fn x86_clts_shape_valid(kind: &OpKind) -> bool {
    matches!(kind, OpKind::X86Clts)
}

impl X86_64Lowerer {
    /// Clear the state-backed guest CR0.TS bit without executing host CLTS.
    ///
    /// Protected-mode CPL and VM86 checks are dynamic. A disallowed execution
    /// restores RAX/RFLAGS and returns at the original guest PC before CR0 is
    /// modified, so the interpreter delivers #GP(0) precisely. Real-address
    /// mode bypasses the CPL check. Successful execution preserves every GPR
    /// and RFLAGS bit and may continue in the native region because CR0.TS does
    /// not change instruction decoding or address translation.
    pub(crate) fn emit_x86_clts(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86Clts requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_clts_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86Clts".to_string(),
                operand: "requires the exact operand-free CLTS form".to_string(),
            });
        }

        // Preserve the complete native guest flag image and guest RAX while
        // RAX addresses GuestRegs. The trampoline keeps host-unsafe AC in its
        // dedicated shadow, which this operation deliberately leaves intact.
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.code.emit_bytes(&[0x48, 0x8B, 0x45]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rax,[rbp+state]

        // Real-address mode (CR0.PE=0) permits CLTS. Otherwise effective CPL
        // must be zero; GuestRegs.cpl already maps VM86 execution to CPL3.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],1
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        let commit_real_mode = self.emit_jcc_placeholder(X86Cond::E);

        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        let fault = self.emit_jcc_placeholder(X86Cond::Ne);

        self.patch_rel32_to_current(commit_real_mode)?;
        self.code.emit_bytes(&[0x48, 0x83, 0xA0]); // and qword [rax+cr0],-9
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u8(0xF7);
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // Fault path: no architectural state has committed. Restore the exact
        // inputs and hand the instruction to the direct interpreter.
        self.patch_rel32_to_current(fault)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
