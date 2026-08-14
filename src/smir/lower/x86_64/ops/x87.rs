//! State-backed x87 environment/control lowering.

use crate::smir::ir::ops::{OpKind, SmirOp, X86X87ControlKind};
use crate::smir::ir::types::{DispSize, OpWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CR0_OFFSET, X86_GUEST_X87_CONTROL_WORD_OFFSET,
    X86_GUEST_X87_DATA_PTR_OFFSET, X86_GUEST_X87_INSTR_PTR_OFFSET,
    X86_GUEST_X87_LAST_OPCODE_OFFSET, X86_GUEST_X87_STATUS_WORD_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_STATE_PTR_AT_RBP,
};

use crate::smir::lower::x86_64::{X86_64Lowerer, X86Cond, X86Emitter};

const CR0_EM: i64 = 1 << 2;
const CR0_TS: i64 = 1 << 3;

/// Validate the operand-free x87 controls with complete state-backed native
/// semantics. Prefix provenance is represented by a preceding APX guard, not
/// an operation hint.
pub(crate) fn x86_x87_control_shape_valid(op: &SmirOp) -> bool {
    op.x86_hint.is_none()
        && matches!(
            op.kind,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::Init
                    | X86X87ControlKind::ClearExceptions
                    | X86X87ControlKind::EnterMmx
                    | X86X87ControlKind::EmptyMmx
                    | X86X87ControlKind::StoreStatusAx,
                addr: None,
            }
        )
}

impl X86_64Lowerer {
    /// Deoptimize before the x87 instruction while CR0.EM or CR0.TS is set.
    /// Direct replay delivers #NM after the already-emitted encoding/APX guard
    /// and before any environment or GPR state is committed.
    fn emit_x86_x87_available_guard(&mut self, guest_pc: u64) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "x87 control requires JIT fault-deoptimization guards".to_string(),
            });
        }

        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
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
        let available = self.emit_jcc_placeholder(X86Cond::E);

        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        self.emit_native_exit(guest_pc);

        self.patch_rel32_to_current(available)?;
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }

    fn emit_x86_x87_tag_word(&mut self, tag_word: i64) {
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        X86Emitter::new(&mut self.code).emit_mov_mi_disp(
            PhysReg::Rax,
            X86_GUEST_X87_TAG_WORD_OFFSET,
            DispSize::Auto,
            tag_word,
            OpWidth::W64,
        );
        self.code.emit_u8(0x58); // pop rax
    }

    fn emit_x86_x87_init(&mut self, guest_pc: u64) -> Result<(), LowerError> {
        self.emit_x86_x87_available_guard(guest_pc)?;
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        let mut emitter = X86Emitter::new(&mut self.code);
        for (offset, value) in [
            (X86_GUEST_X87_CONTROL_WORD_OFFSET, 0x037F),
            (X86_GUEST_X87_STATUS_WORD_OFFSET, 0),
            (X86_GUEST_X87_TAG_WORD_OFFSET, 0xFFFF),
            (X86_GUEST_X87_DATA_PTR_OFFSET, 0),
            (X86_GUEST_X87_INSTR_PTR_OFFSET, 0),
            (X86_GUEST_X87_LAST_OPCODE_OFFSET, 0),
        ] {
            emitter.emit_mov_mi_disp(PhysReg::Rax, offset, DispSize::Auto, value, OpWidth::W64);
        }
        self.code.emit_u8(0x58); // pop rax
        Ok(())
    }

    fn emit_x86_x87_clear_exceptions(&mut self, guest_pc: u64) -> Result<(), LowerError> {
        self.emit_x86_x87_available_guard(guest_pc)?;
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        X86Emitter::new(&mut self.code).emit_alu_mi_disp(
            4,
            PhysReg::Rax,
            X86_GUEST_X87_STATUS_WORD_OFFSET,
            DispSize::Auto,
            !0x80FF,
            OpWidth::W64,
        );
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
        Ok(())
    }

    fn emit_x86_x87_store_status_ax(&mut self, guest_pc: u64) -> Result<(), LowerError> {
        self.emit_x86_x87_available_guard(guest_pc)?;
        self.code.emit_u8(0x51); // push rcx
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_mov_rm(
            PhysReg::Rcx,
            PhysReg::Rbp,
            X86_STATE_PTR_AT_RBP,
            OpWidth::W64,
        );
        emitter.emit_mov_rm(
            PhysReg::Rax,
            PhysReg::Rcx,
            X86_GUEST_X87_STATUS_WORD_OFFSET,
            OpWidth::W16,
        );
        self.code.emit_u8(0x59); // pop rcx
        Ok(())
    }

    pub(crate) fn lower_op_x87(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        let OpKind::X86X87Control { kind, .. } = &op.kind else {
            return self.lower_op_misc(op);
        };
        if !matches!(
            kind,
            X86X87ControlKind::Init
                | X86X87ControlKind::ClearExceptions
                | X86X87ControlKind::EnterMmx
                | X86X87ControlKind::EmptyMmx
                | X86X87ControlKind::StoreStatusAx
        ) {
            return self.lower_op_misc(op);
        }
        if !x86_x87_control_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: format!("X86X87Control {kind:?}"),
                operand: "requires an unhinted operand-free control".to_string(),
            });
        }

        match kind {
            X86X87ControlKind::Init => self.emit_x86_x87_init(op.guest_pc),
            X86X87ControlKind::ClearExceptions => self.emit_x86_x87_clear_exceptions(op.guest_pc),
            X86X87ControlKind::EnterMmx => {
                self.emit_x86_x87_tag_word(0);
                Ok(())
            }
            X86X87ControlKind::EmptyMmx => {
                self.emit_x86_x87_tag_word(0xFFFF);
                Ok(())
            }
            X86X87ControlKind::StoreStatusAx => self.emit_x86_x87_store_status_ax(op.guest_pc),
            _ => unreachable!("filtered above"),
        }
    }
}
