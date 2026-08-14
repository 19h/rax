//! Native x86 `RDRAND`/`RDSEED` admission and state-backed lowering.
//!
//! Identity-mapped legacy GPRs can receive the host instruction directly.
//! Guest RSP/RBP and APX EGPRs instead receive the random value through a
//! scratch register and the canonical `GuestRegs` file. Intel SDM Order No.
//! 325383-092US (June 2026), Vol. 2B specifies a zero destination at the
//! selected width when CF=0, CF as the readiness result, and
//! OF/SF/ZF/AF/PF=0; the host instruction therefore supplies both the value
//! and exact status image without a semantic helper.

use super::*;

/// Whether `op` is an unhinted architectural `RDRAND`/`RDSEED` shape that the
/// x86-64 lowerer can encode either directly or through the state bridge.
pub(crate) fn x86_random_shape_valid(op: &SmirOp) -> bool {
    matches!(
        &op.kind,
        OpKind::X86Random {
            dst: VReg::Arch(ArchReg::X86(dst)),
            width: OpWidth::W16 | OpWidth::W32 | OpWidth::W64,
            ..
        } if op.x86_hint.is_none() && dst.gpr_index().is_some()
    )
}

/// Identify random-source operations whose destination has no usable native
/// identity mapping (guest RSP/RBP or an APX EGPR).
pub(crate) fn x86_state_random_candidate(op: &SmirOp) -> bool {
    matches!(
        &op.kind,
        OpKind::X86Random { dst, .. } if x86_state_backed_arch_gpr(dst)
    )
}

pub(crate) fn x86_state_random_valid(op: &SmirOp) -> bool {
    x86_state_random_candidate(op) && x86_random_shape_valid(op)
}

impl X86_64Lowerer {
    fn lower_state_backed_x86_random(
        &mut self,
        dst: VReg,
        width: OpWidth,
        seed: bool,
    ) -> Result<(), LowerError> {
        let dst_index = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed X86Random".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // preserve guest RAX while snapshotting
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_x86_random(PhysReg::Rdx, width, seed);
        }

        // Every instruction after the random source is MOV or LEA and is
        // therefore flag-neutral. The host instruction's CF and cleared
        // OF/SF/ZF/AF/PF reach the native trampoline without reconstruction.
        self.emit_store_gpr_slot_from_reg(dst_index, PhysReg::Rdx, width)?;
        if dst_index == 5 {
            let commit_width = if width == OpWidth::W16 {
                OpWidth::W16
            } else {
                OpWidth::W64
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }

    pub(crate) fn lower_x86_random(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_random_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86Random".to_string(),
                operand: format!("invalid random-source shape: {op:?}"),
            });
        }
        let OpKind::X86Random { dst, width, seed } = &op.kind else {
            unreachable!("shape validator accepted only X86Random")
        };

        if x86_state_random_candidate(op) {
            return self.lower_state_backed_x86_random(*dst, *width, *seed);
        }

        let dst = self.get_dst_reg(*dst)?;
        Self::ensure_flag_stack_operands_safe("X86Random", &[dst])?;
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_x86_random(dst, *width, *seed);
        Ok(())
    }
}
