//! Fault-precise dynamic AMD TBM feature guard.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{DispSize, OpWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CPUID_TBM_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CS_L_OFFSET,
    X86_GUEST_RFLAGS_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

pub(crate) fn x86_require_tbm_shape_valid(op: &SmirOp) -> bool {
    matches!(op.kind, OpKind::X86RequireTbm) && op.x86_hint.is_none()
}

impl X86_64Lowerer {
    /// Continue only while CPUID.TBM is enabled and the live guest is in
    /// protected, non-virtual-8086 64-bit mode. Compatibility mode is
    /// architecturally valid for TBM, but its WIG/address-size behavior is
    /// outside the strict x86-64 lifter and therefore deoptimizes to direct
    /// execution. Failure restores native RAX/RFLAGS and exits at the source
    /// instruction PC without committing guarded work.
    pub(crate) fn emit_x86_require_tbm(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86RequireTbm requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_require_tbm_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86RequireTbm".to_string(),
                operand: "requires the exact unhinted operand-free guard".to_string(),
            });
        }

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+cpuid_tbm],0
        self.code.emit_u32(X86_GUEST_CPUID_TBM_OFFSET as u32);
        self.code.emit_u8(0);
        let feature_absent = self.emit_jcc_placeholder(X86Cond::E);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CR0_OFFSET,
                DispSize::Auto,
                1,
                OpWidth::W64,
            );
        }
        let real_mode = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cs_l],0
        self.code.emit_u32(X86_GUEST_CS_L_OFFSET as u32);
        self.code.emit_u8(0);
        let compatibility_mode = self.emit_jcc_placeholder(X86Cond::E);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_RFLAGS_OFFSET,
                DispSize::Auto,
                crate::isa::x86_64::flags::bits::VM as i64,
                OpWidth::W64,
            );
        }
        let enabled = self.emit_jcc_placeholder(X86Cond::E);

        self.patch_rel32_to_current(feature_absent)?;
        self.patch_rel32_to_current(real_mode)?;
        self.patch_rel32_to_current(compatibility_mode)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(enabled)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }
}
