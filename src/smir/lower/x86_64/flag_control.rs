//! Native lowering for operand-free x86 carry and direction-flag controls.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{DispSize, OpWidth};
use crate::smir::lower::{X86_GUEST_RFLAGS_OFFSET, regalloc::PhysReg};

use super::{X86_64Lowerer, X86Emitter};

const X86_DF_BIT: i64 = 1 << 10;

/// Admit only the exact, unhinted operand-free flag-control taxonomy emitted
/// by the x86 lifter. `ReadFlags`/`WriteFlags` remain outside this target gate.
pub(crate) fn x86_flag_control_shape_valid(op: &SmirOp) -> bool {
    op.x86_hint.is_none()
        && matches!(
            op.kind,
            OpKind::SetCF { .. } | OpKind::SetDF { .. } | OpKind::CmcCF
        )
}

impl X86_64Lowerer {
    /// Lower CLD/STD and, in trampoline-backed JIT mode, commit DF to the
    /// GuestRegs shadow that survives native handoff. The trampoline exports
    /// only arithmetic status flags, so changing host DF alone is insufficient.
    pub(crate) fn emit_x86_set_df(&mut self, value: bool) {
        if self.jit_fault_deopt_guards {
            self.code.emit_u8(0x50); // preserve guest RAX
            self.code.emit_u8(0x9C); // preserve status flags and incoming DF
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                if value {
                    emitter.emit_alu_mi_disp(
                        1, // OR
                        PhysReg::Rax,
                        X86_GUEST_RFLAGS_OFFSET,
                        DispSize::Auto,
                        X86_DF_BIT,
                        OpWidth::W64,
                    );
                } else {
                    emitter.emit_alu_mi_disp(
                        4, // AND
                        PhysReg::Rax,
                        X86_GUEST_RFLAGS_OFFSET,
                        DispSize::Auto,
                        !X86_DF_BIT,
                        OpWidth::W64,
                    );
                }
            }
            self.code.emit_u8(0x9D); // restore pre-instruction status flags
            self.code.emit_u8(0x58); // restore guest RAX
        }

        let mut emitter = X86Emitter::new(&mut self.code);
        if value {
            emitter.emit_std();
        } else {
            emitter.emit_cld();
        }
    }
}
