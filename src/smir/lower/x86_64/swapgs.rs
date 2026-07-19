//! Fault-precise, state-backed SWAPGS lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CPL_OFFSET, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_KERNEL_GS_BASE_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the exact architectural state operands emitted by the x86 lifter.
pub(crate) fn x86_swapgs_shape_valid(kind: &OpKind) -> bool {
    matches!(
        kind,
        OpKind::X86SwapGs {
            gs_base: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
            kernel_gs_base: VReg::Arch(ArchReg::X86(X86Reg::KernelGsBase)),
        }
    )
}

impl X86_64Lowerer {
    /// Exchange the two guest base values through `GuestRegs`. CPL faults
    /// deoptimize at the original guest PC before either value is written; the
    /// mode-aware CPU admission gate separately rejects SWAPGS outside CS.L=1.
    pub(crate) fn emit_x86_swapgs(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86SwapGs requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_swapgs_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86SwapGs".to_string(),
                operand: "requires exact GS.base and IA32_KERNEL_GS_BASE operands".to_string(),
            });
        }

        // Publish every identity-mapped GPR before borrowing RAX/RCX/RDX as
        // scratch. The saved flags are restored on both success and fault.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        // CPL != 0 => #GP(0). No guest state has been modified at this point.
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+disp32],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        let fault_branch = self.emit_jcc_placeholder(X86Cond::Ne);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rdx,
                PhysReg::Rax,
                X86_GUEST_GS_BASE_OFFSET,
                OpWidth::W64,
            );
            emitter.emit_mov_rm(
                PhysReg::Rcx,
                PhysReg::Rax,
                X86_GUEST_KERNEL_GS_BASE_OFFSET,
                OpWidth::W64,
            );
            emitter.emit_mov_mr(
                PhysReg::Rax,
                X86_GUEST_GS_BASE_OFFSET,
                PhysReg::Rcx,
                OpWidth::W64,
            );
            emitter.emit_mov_mr(
                PhysReg::Rax,
                X86_GUEST_KERNEL_GS_BASE_OFFSET,
                PhysReg::Rdx,
                OpWidth::W64,
            );
        }

        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        let fault = self.code.position();
        let rel = fault as i64 - fault_branch as i64 - 4;
        if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
            return Err(LowerError::RelocationOutOfRange {
                offset: fault_branch,
                target: fault,
            });
        }
        self.code.patch_i32(fault_branch, rel as i32);
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
