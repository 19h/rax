//! Fault-precise AMD TBM feature guard for the x86-on-AArch64 bridge.

use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::OpWidth;
use crate::smir::lower::LowerError;

use super::{
    A64_GUEST_PC_OFFSET, A64_GUEST_X86_TBM_ENABLED_OFFSET, A64_GUEST_X86_TBM_MODE_VALID_OFFSET,
    A64_STATE_REG, Aarch64Lowerer,
};

impl Aarch64Lowerer {
    /// Continue only while TBM remains enabled and the bridged x86 guest is in
    /// protected, non-virtual-8086 64-bit mode. Compatibility mode is replayed
    /// by the direct decoder because the strict lifter models long-mode XOP.W
    /// and address defaults. A rejected path records the source PC and returns
    /// before guarded address-generation or memory operations can execute.
    pub(crate) fn emit_x86_require_tbm(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.x86_guest_state_guards {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 X86RequireTbm requires x86 guest-state guards".into(),
            });
        }
        if !crate::smir::lower::x86_64::x86_require_tbm_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86RequireTbm".into(),
                operand: "requires the exact unhinted operand-free guard".into(),
            });
        }

        const SCRATCH: u8 = 9;
        self.emit_push_scratch(SCRATCH);
        self.emit_ldst_unsigned(
            SCRATCH,
            A64_STATE_REG,
            3,
            0b01,
            A64_GUEST_X86_TBM_ENABLED_OFFSET / 8,
        );
        let feature_absent = self.code.position();
        self.emit(0xb400_0000 | u32::from(SCRATCH)); // cbz x9, rejected
        self.emit_ldst_unsigned(
            SCRATCH,
            A64_STATE_REG,
            3,
            0b01,
            A64_GUEST_X86_TBM_MODE_VALID_OFFSET / 8,
        );
        let enabled = self.code.position();
        self.emit(0xb500_0000 | u32::from(SCRATCH)); // cbnz x9, enabled

        self.patch_compare_branch_to_current(feature_absent, SCRATCH, false)?;
        self.emit_mov_imm(SCRATCH, op.guest_pc as i64, OpWidth::W64)?;
        self.emit_ldst_unsigned(SCRATCH, A64_STATE_REG, 3, 0b00, A64_GUEST_PC_OFFSET / 8);
        self.emit_pop_scratch(SCRATCH);
        self.emit(0xd65f_03c0); // ret to identity trampoline

        self.patch_compare_branch_to_current(enabled, SCRATCH, true)?;
        self.emit_pop_scratch(SCRATCH);
        Ok(())
    }
}
