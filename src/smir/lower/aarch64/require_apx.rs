//! Fault-precise Intel APX feature guard for the x86-on-AArch64 bridge.

use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::OpWidth;
use crate::smir::lower::LowerError;

use super::{A64_GUEST_PC_OFFSET, A64_GUEST_X86_APX_ENABLED_OFFSET, A64_STATE_REG, Aarch64Lowerer};

impl Aarch64Lowerer {
    /// Enable x86-specific feature guards backed by the appended bridge state
    /// in `Aarch64GuestRegs`.
    ///
    /// Callers must first validate the function with the x86-on-AArch64 native
    /// gate. The default remains disabled so an ordinary AArch64/AArch32 guest
    /// cannot reinterpret its state object as x86 architectural state.
    pub fn set_x86_guest_state_guards(&mut self, enable: bool) {
        self.x86_guest_state_guards = enable;
    }

    /// Continue only while APX remains enabled in the live x86 bridge state.
    /// The disabled path restores the mapped scratch GPR, preserves NZCV, writes
    /// the exact source instruction PC, and returns before any guarded operation
    /// can commit. Direct x86 execution then replays the instruction and raises
    /// the architecturally required #UD.
    pub(crate) fn emit_x86_require_apx(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.x86_guest_state_guards {
            return Err(LowerError::UnsupportedOp {
                op: "AArch64 X86RequireApx requires x86 guest-state guards".into(),
            });
        }
        if !crate::smir::lower::x86_64::x86_require_apx_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86RequireApx".into(),
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
            A64_GUEST_X86_APX_ENABLED_OFFSET / 8,
        );
        let enabled = self.code.position();
        self.emit(0xb500_0000 | u32::from(SCRATCH)); // cbnz x9, enabled

        self.emit_mov_imm(SCRATCH, op.guest_pc as i64, OpWidth::W64)?;
        self.emit_ldst_unsigned(SCRATCH, A64_STATE_REG, 3, 0b00, A64_GUEST_PC_OFFSET / 8);
        self.emit_pop_scratch(SCRATCH);
        self.emit(0xd65f_03c0); // ret to the identity trampoline

        self.patch_compare_branch_to_current(enabled, SCRATCH, true)?;
        self.emit_pop_scratch(SCRATCH);
        Ok(())
    }
}
