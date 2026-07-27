//! Fault-precise dynamic APX feature guard lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::lower::{LowerError, X86_GUEST_APX_ENABLED_OFFSET};

use super::{X86_64Lowerer, X86Cond};

/// Validate the operand-free APX guard emitted by the strict x86-64 lifter.
pub(crate) fn x86_require_apx_shape_valid(op: &SmirOp) -> bool {
    matches!(op.kind, OpKind::X86RequireApx) && op.x86_hint.is_none()
}

impl X86_64Lowerer {
    /// Emit the dynamic APX state check shared by the standalone lifter guard
    /// and terminal helper-backed operations whose shape carries APX
    /// provenance internally.
    pub(crate) fn emit_x86_require_apx_guard(&mut self, guest_pc: u64) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86RequireApx requires JIT fault-deoptimization guards".to_string(),
            });
        }

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+apx_enabled],0
        self.code.emit_u32(X86_GUEST_APX_ENABLED_OFFSET as u32);
        self.code.emit_u8(0);
        let enabled = self.emit_jcc_placeholder(X86Cond::Ne);

        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(guest_pc);

        self.patch_rel32_to_current(enabled)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }

    /// Continue only while APX remains enabled in the marshalled guest state.
    /// A disabled feature restores RAX and the complete native RFLAGS image,
    /// then exits at the original guest PC so direct execution delivers #UD.
    pub(crate) fn emit_x86_require_apx(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_require_apx_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86RequireApx".to_string(),
                operand: "requires the exact unhinted operand-free guard".to_string(),
            });
        }
        self.emit_x86_require_apx_guard(op.guest_pc)
    }
}
