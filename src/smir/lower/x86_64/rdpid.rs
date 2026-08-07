//! State-backed native lowering for x86 RDPID.

use super::*;

/// Whether `dst` is an architectural x86 GPR that the RDPID lowering can
/// commit without ever mapping guest RSP/RBP onto native RSP/RBP.
pub(crate) fn x86_rdpid_gpr_valid(dst: &VReg) -> bool {
    matches!(dst, VReg::Arch(ArchReg::X86(reg)) if reg.gpr_index().is_some_and(|index| index <= 31))
}

/// Validate the complete native RDPID operation shape.
pub(crate) fn x86_rdpid_shape_valid(kind: &OpKind) -> bool {
    matches!(kind, OpKind::X86ReadPid { dst } if x86_rdpid_gpr_valid(dst))
}

impl X86_64Lowerer {
    /// Read the emulated IA32_TSC_AUX value and commit its zero-extended value
    /// to an architectural GPR. RSP, RBP, and APX EGPRs use canonical
    /// `GuestRegs` slots; legacy identity-mapped destinations remain direct.
    pub(crate) fn lower_x86_read_pid(&mut self, dst: VReg) -> Result<(), LowerError> {
        if !x86_rdpid_gpr_valid(&dst) {
            return Err(LowerError::InvalidOperand {
                op: "X86ReadPid".to_string(),
                operand: "destination must be an architectural x86 GPR".to_string(),
            });
        }
        let index = Self::x86_gpr_index(dst).expect("validated architectural RDPID GPR");

        if index <= 15 && !matches!(index, 4 | 5) {
            let dst = self.get_dst_reg(dst)?;
            Self::ensure_flag_stack_operands_safe("X86ReadPid", &[dst])?;
            let mut emitter = X86Emitter::new(&mut self.code);
            // The destination is architecturally overwritten, so it is also a
            // flag-neutral state-pointer scratch.
            emitter.emit_mov_rm(dst, PhysReg::Rbp, X86_STATE_PTR_AT_RBP, OpWidth::W64);
            emitter.emit_mov_rm(dst, dst, X86_GUEST_TSC_AUX_OFFSET, OpWidth::W32);
            return Ok(());
        }

        // State-backed destinations have no usable physical host counterpart.
        // Preserve two identity-mapped scratches, zero-extend through ECX, and
        // commit through GuestRegs. The saved guest-RBP word must track slot 5
        // because the native epilogue restores it after discarding host RBP.
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_push(PhysReg::Rax);
        emitter.emit_push(PhysReg::Rcx);
        emitter.emit_mov_rm(
            PhysReg::Rax,
            PhysReg::Rbp,
            X86_STATE_PTR_AT_RBP,
            OpWidth::W64,
        );
        emitter.emit_mov_rm(
            PhysReg::Rcx,
            PhysReg::Rax,
            X86_GUEST_TSC_AUX_OFFSET,
            OpWidth::W32,
        );
        emitter.emit_mov_mr(
            PhysReg::Rax,
            i32::from(index) * 8,
            PhysReg::Rcx,
            OpWidth::W64,
        );
        if index == 5 {
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rcx, OpWidth::W64);
        }
        emitter.emit_pop(PhysReg::Rcx);
        emitter.emit_pop(PhysReg::Rax);
        Ok(())
    }
}
