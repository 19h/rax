//! Fault-precise dynamic AMD SSE4A execution-state guard.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{DispSize, OpWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CPUID_SSE4A_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

const CR0_EM: i64 = 1 << 2;
const CR0_TS: i64 = 1 << 3;
const CR4_OSFXSR: i64 = 1 << 9;

pub(crate) fn x86_require_sse4a_shape_valid(op: &SmirOp) -> bool {
    matches!(op.kind, OpKind::X86RequireSse4a) && op.x86_hint.is_none()
}

impl X86_64Lowerer {
    /// Continue only while SSE4A, !CR0.EM, !CR0.TS, and CR4.OSFXSR remain
    /// true. Failure restores the exact incoming native state and hands the
    /// original guest PC to direct execution for precise #UD/#NM delivery.
    pub(crate) fn emit_x86_require_sse4a(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86RequireSse4a requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_require_sse4a_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86RequireSse4a".to_string(),
                operand: "requires the exact unhinted operand-free guard".to_string(),
            });
        }

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CPUID_SSE4A_OFFSET,
                DispSize::Auto,
                1,
                OpWidth::W64,
            );
        }
        let feature_absent = self.emit_jcc_placeholder(X86Cond::E);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CR0_OFFSET,
                DispSize::Auto,
                CR0_EM | CR0_TS,
                OpWidth::W64,
            );
        }
        let sse_disabled = self.emit_jcc_placeholder(X86Cond::Ne);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CR4_OFFSET,
                DispSize::Auto,
                CR4_OSFXSR,
                OpWidth::W64,
            );
        }
        let enabled = self.emit_jcc_placeholder(X86Cond::Ne);

        self.patch_rel32_to_current(feature_absent)?;
        self.patch_rel32_to_current(sse_disabled)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(enabled)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }
}
