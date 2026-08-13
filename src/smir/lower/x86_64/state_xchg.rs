//! State-backed x86 register `XCHG` lowering.
//!
//! Guest RSP/RBP and APX EGPRs do not have usable identity mappings on an
//! x86-64 host. Materialize a coherent `GuestRegs` snapshot, exchange the
//! requested low-width values, and commit both architectural slots without
//! exposing the host stack or frame pointers as guest state.

use crate::smir::ir::types::{OpWidth, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::x86_64::{X86_64Lowerer, X86Emitter};

impl X86_64Lowerer {
    pub(crate) fn lower_state_backed_gpr_xchg(
        &mut self,
        reg1: VReg,
        reg2: VReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if !matches!(
            width,
            OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
        ) {
            return Err(LowerError::InvalidOperand {
                op: "state-backed Xchg".to_string(),
                operand: format!("unsupported width {width:?}"),
            });
        }

        let reg1_idx = Self::x86_gpr_index(reg1).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Xchg".to_string(),
            operand: "first operand is not an architectural x86 GPR".to_string(),
        })?;
        let reg2_idx = Self::x86_gpr_index(reg2).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed Xchg".to_string(),
            operand: "second operand is not an architectural x86 GPR".to_string(),
        })?;

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(reg2_idx) * 8, width);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(reg1_idx) * 8, width);
        }

        self.emit_store_gpr_slot_from_reg(reg1_idx, PhysReg::Rdx, width)?;
        self.emit_store_gpr_slot_from_reg(reg2_idx, PhysReg::Rdi, width)?;

        // The native prologue stores guest RBP at [host RBP]. Keep that saved
        // image coherent with the exact x86 partial-write contract so the
        // epilogue cannot overwrite the state-file result.
        let saved_rbp_commit_width = match width {
            OpWidth::W8 | OpWidth::W16 => width,
            OpWidth::W32 | OpWidth::W64 => OpWidth::W64,
            OpWidth::W128 => unreachable!("width validated before native emission"),
        };
        if reg1_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, saved_rbp_commit_width);
        }
        if reg2_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdi, saved_rbp_commit_width);
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
