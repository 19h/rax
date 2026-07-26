//! State-backed x86 BMI2 `MULX` lowering.

use super::*;

/// A `MULX` needs the state bridge when any explicit operand cannot use the
/// native identity GPR map. The implicit multiplicand is always guest RDX.
pub(crate) fn x86_state_backed_gpr_mulx_candidate(op: &SmirOp) -> bool {
    matches!(
        &op.kind,
        OpKind::MulU {
            dst_lo,
            dst_hi,
            src2: SrcOperand::Reg(src2),
            ..
        } if x86_state_backed_arch_gpr(dst_lo)
            || dst_hi.as_ref().is_some_and(x86_state_backed_arch_gpr)
            || x86_state_backed_arch_gpr(src2)
    )
}

/// Validate the exact state-backed form emitted by the VEX and APX `MULX`
/// lifters. Keeping this validator operand-exact makes native admission
/// fail-closed for malformed hinted SMIR.
pub(crate) fn x86_state_backed_gpr_mulx_valid(op: &SmirOp) -> bool {
    let arch_gpr =
        |reg: &VReg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some());

    x86_state_backed_gpr_mulx_candidate(op)
        && matches!(op.x86_hint, Some(X86OpHint::Mulx))
        && matches!(
            &op.kind,
            OpKind::MulU {
                dst_lo,
                dst_hi: Some(dst_hi),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                src2: SrcOperand::Reg(src2),
                width: OpWidth::W32 | OpWidth::W64,
                flags: FlagUpdate::None,
            } if arch_gpr(dst_lo) && arch_gpr(dst_hi) && arch_gpr(src2)
        )
}

impl X86_64Lowerer {
    /// Execute `MULX` through scratch host registers after snapshotting the
    /// complete guest GPR file. This covers guest RSP/RBP and APX EGPRs without
    /// mapping either guest stack register onto the native stack/frame pointer.
    pub(crate) fn lower_state_backed_gpr_mulx(
        &mut self,
        dst_lo: VReg,
        dst_hi: VReg,
        src2: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let dst_lo_idx = Self::x86_gpr_index(dst_lo).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed MULX".to_string(),
            operand: "low destination is not an architectural x86 GPR".to_string(),
        })?;
        let dst_hi_idx = Self::x86_gpr_index(dst_hi).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed MULX".to_string(),
            operand: "high destination is not an architectural x86 GPR".to_string(),
        })?;
        let src2_idx = Self::x86_gpr_index(src2).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed MULX".to_string(),
            operand: "explicit source is not an architectural x86 GPR".to_string(),
        })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: "state-backed MULX".to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }

        // MOV, MULX, PUSH, and LEA preserve RFLAGS. The snapshot therefore
        // retains every incoming flag without a PUSHFQ/POPFQ pair.
        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, 2 * 8, width);
            emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rax, i32::from(src2_idx) * 8, width);
            // MULX high, low, source; implicit multiplicand is EDX/RDX.
            emitter.emit_vex_bmi_rr_pp(
                0xF6,
                X86SsePrefix::Repne,
                PhysReg::Rdi,
                PhysReg::R8,
                PhysReg::Rcx,
                width,
            );
        }

        // Architectural assignment order is low followed by high. Preserve it
        // so an aliased destination retains the high half.
        self.emit_store_gpr_slot_from_reg(dst_lo_idx, PhysReg::Rcx, width)?;
        if dst_lo_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rcx, OpWidth::W64);
        }
        self.emit_store_gpr_slot_from_reg(dst_hi_idx, PhysReg::Rdi, width)?;
        if dst_hi_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdi, OpWidth::W64);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }
}
