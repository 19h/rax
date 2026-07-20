//! Fault-precise, host-safe CLAC/STAC lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{DispSize, OpWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_AC_FLAG_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

const X86_AC_BIT: i64 = 1 << 18;
const HOST_SAFE_RFLAGS_MASK: i32 = -0x44101; // ~(TF | NT | AC)

/// CLAC/STAC have no explicit operands; this predicate keeps target admission
/// fail-closed if the operation taxonomy changes.
pub(crate) fn x86_set_ac_shape_valid(kind: &OpKind) -> bool {
    matches!(kind, OpKind::SetAC { .. })
}

impl X86_64Lowerer {
    /// Materialize native status flags while sourcing guest AC from its shadow.
    /// The temporary flag mutations used to merge AC are undone before the
    /// next operation observes RFLAGS.
    pub(crate) fn emit_x86_read_flags_with_ac(&mut self, dst: PhysReg) -> Result<(), LowerError> {
        Self::ensure_flag_stack_operands_safe("ReadFlags", &[dst])?;

        self.code.emit_u8(0x9C); // preserve the original host-safe flag image
        self.code.emit_u8(0x50); // save guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+ac],0
        self.code.emit_u32(X86_GUEST_AC_FLAG_OFFSET as u32);
        self.code.emit_u8(0);
        self.code.emit_u8(0x58); // restore guest RAX without changing cmp flags
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_pop(dst);
        }
        let clear_ac = self.emit_jcc_placeholder(X86Cond::E);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_or_ri(dst, X86_AC_BIT, OpWidth::W64);
        }
        self.code.emit_u8(0xE9);
        let restore_flags = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(clear_ac)?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_and_ri(dst, !X86_AC_BIT, OpWidth::W64);
        }
        self.patch_rel32_to_current(restore_flags)?;

        // Restore the exact pre-ReadFlags native flags, except for host-unsafe
        // control bits. The architectural result remains in `dst` with guest
        // AC merged into it.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(dst);
        }
        self.code.emit_bytes(&[0x48, 0x81, 0x24, 0x24]);
        self.code.emit_i32(HOST_SAFE_RFLAGS_MASK);
        self.code.emit_u8(0x9D);
        Ok(())
    }

    /// Split guest AC out of a materialized flag image before loading the
    /// remaining host-safe flags with POPFQ.
    pub(crate) fn emit_x86_write_flags_with_ac(&mut self, src: PhysReg) -> Result<(), LowerError> {
        Self::ensure_flag_stack_operands_safe("WriteFlags", &[src])?;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(src);
        }
        self.code.emit_u8(0x50); // save guest RAX above the incoming image
        self.emit_load_state_ptr_rax();
        self.code.emit_bytes(&[0xF7, 0x44, 0x24, 0x08]); // test dword [rsp+8],AC
        self.code.emit_i32(X86_AC_BIT as i32);
        let clear_ac = self.emit_jcc_placeholder(X86Cond::E);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mi_disp(
                PhysReg::Rax,
                X86_GUEST_AC_FLAG_OFFSET,
                DispSize::Auto,
                1,
                OpWidth::W64,
            );
        }
        self.code.emit_u8(0xE9);
        let restore_rax = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(clear_ac)?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mi_disp(
                PhysReg::Rax,
                X86_GUEST_AC_FLAG_OFFSET,
                DispSize::Auto,
                0,
                OpWidth::W64,
            );
        }
        self.patch_rel32_to_current(restore_rax)?;

        self.code.emit_u8(0x58); // restore guest RAX
        self.code.emit_bytes(&[0x48, 0x81, 0x24, 0x24]);
        self.code.emit_i32(HOST_SAFE_RFLAGS_MASK);
        self.code.emit_u8(0x9D);
        Ok(())
    }

    /// Update the guest AC shadow without executing host CLAC/STAC or loading
    /// AC into host RFLAGS. Protected-mode CPL failures deoptimize at the
    /// original instruction before the shadow is modified; real mode bypasses
    /// the CPL check.
    pub(crate) fn emit_x86_set_ac(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "SetAC requires JIT fault-deoptimization guards".to_string(),
            });
        }
        let OpKind::SetAC { value } = &op.kind else {
            return Err(LowerError::InvalidOperand {
                op: "SetAC".to_string(),
                operand: "requires the exact operand-free SetAC form".to_string(),
            });
        };

        self.code.emit_u8(0x50); // save guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // preserve every native status flag

        // #UD iff CR0.PE=1 and effective CPL != 0. Virtual-8086 mode is
        // marshalled as CPL3. CR0.PE=0 permits a stale nonzero selector RPL.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],1
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        let real_mode = self.emit_jcc_placeholder(X86Cond::E);

        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        let fault = self.emit_jcc_placeholder(X86Cond::Ne);
        self.patch_rel32_to_current(real_mode)?;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mi_disp(
                PhysReg::Rax,
                X86_GUEST_AC_FLAG_OFFSET,
                DispSize::Auto,
                i64::from(*value),
                OpWidth::W64,
            );
        }
        self.code.emit_u8(0x9D); // restore status flags
        self.code.emit_u8(0x58); // restore guest RAX
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        let fault_target = self.code.position();
        let rel = fault_target as i64 - fault as i64 - 4;
        if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
            return Err(LowerError::RelocationOutOfRange {
                offset: fault,
                target: fault_target,
            });
        }
        self.code.patch_i32(fault, rel as i32);
        self.code.emit_u8(0x9D);
        self.code.emit_u8(0x58);
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
