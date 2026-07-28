//! Fault-precise dynamic AMD XOP feature and state guard.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{DispSize, OpWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CPUID_XOP_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
    X86_GUEST_CS_L_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_XCR0_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

pub(crate) fn x86_require_xop_shape_valid(op: &SmirOp) -> bool {
    matches!(op.kind, OpKind::X86RequireXop) && op.x86_hint.is_none()
}

impl X86_64Lowerer {
    /// Continue only while the live guest satisfies every architectural XOP
    /// enablement condition. Compatibility mode is architecturally valid, but
    /// the strict x86-64 lifter deliberately deoptimizes it. Any failed #UD or
    /// #NM condition exits at the source instruction PC; direct replay then
    /// supplies the exact exception class and priority without committed work.
    pub(crate) fn emit_x86_require_xop(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86RequireXop requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_require_xop_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86RequireXop".to_string(),
                operand: "requires the exact unhinted operand-free guard".to_string(),
            });
        }

        const CR0_PE: i64 = 1;
        const CR0_TS: i64 = 1 << 3;
        const CR4_OSXSAVE: i64 = 1 << 18;
        const XCR0_XMM: i64 = 1 << 1;
        const XCR0_YMM: i64 = 1 << 2;

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();

        let mut faults = Vec::with_capacity(7);
        self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+cpuid_xop],0
        self.code.emit_u32(X86_GUEST_CPUID_XOP_OFFSET as u32);
        self.code.emit_u8(0);
        faults.push(self.emit_jcc_placeholder(X86Cond::E));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CR0_OFFSET,
                DispSize::Auto,
                CR0_PE,
                OpWidth::W64,
            );
        }
        faults.push(self.emit_jcc_placeholder(X86Cond::E));
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cs_l],0
        self.code.emit_u32(X86_GUEST_CS_L_OFFSET as u32);
        self.code.emit_u8(0);
        faults.push(self.emit_jcc_placeholder(X86Cond::E));
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
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CR4_OFFSET,
                DispSize::Auto,
                CR4_OSXSAVE,
                OpWidth::W64,
            );
        }
        faults.push(self.emit_jcc_placeholder(X86Cond::E));
        for component in [XCR0_XMM, XCR0_YMM] {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_XCR0_OFFSET,
                DispSize::Auto,
                component,
                OpWidth::W64,
            );
            faults.push(self.emit_jcc_placeholder(X86Cond::E));
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CR0_OFFSET,
                DispSize::Auto,
                CR0_TS,
                OpWidth::W64,
            );
        }
        let enabled = self.emit_jcc_placeholder(X86Cond::E);

        for fault in faults {
            self.patch_rel32_to_current(fault)?;
        }
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(enabled)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }
}
