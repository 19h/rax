//! State-backed x86 GPR lowering for AMD TBM operations.

use crate::smir::ir::ops::X86TbmKind;
use crate::smir::ir::types::{OpWidth, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

use super::{X86_64Lowerer, X86Emitter};

impl X86_64Lowerer {
    pub(crate) fn lower_state_backed_gpr_tbm(
        &mut self,
        dst: VReg,
        src: VReg,
        width: OpWidth,
        kind: X86TbmKind,
        defined_rflags_mask: Option<i64>,
    ) -> Result<(), LowerError> {
        let dst_idx = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed X86Tbm::{kind:?}"),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        let src_idx = Self::x86_gpr_index(src).ok_or_else(|| LowerError::InvalidOperand {
            op: format!("state-backed X86Tbm::{kind:?}"),
            operand: "source is not an architectural x86 GPR".to_string(),
        })?;
        if !matches!(width, OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: format!("state-backed X86Tbm::{kind:?}"),
                operand: format!("unsupported width {width:?}"),
            });
        }

        self.code.emit_u8(0x50); // preserve guest RAX while snapshotting state
        self.emit_load_state_ptr_rax();
        self.emit_spill_legacy_gprs_to_state_from_rax(0);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(src_idx) * 8, width);
        }
        self.emit_x86_tbm_regs(PhysReg::Rdx, PhysReg::Rdi, width, kind, defined_rflags_mask);

        self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
        if dst_idx == 5 {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
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
