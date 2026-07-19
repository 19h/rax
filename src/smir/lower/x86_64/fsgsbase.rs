//! Fault-precise, state-backed FSGSBASE lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_FS_BASE_OFFSET,
    X86_GUEST_GS_BASE_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the exact architectural operand shape emitted by the x86 lifter.
/// EGPR operands necessarily require REX2/APX, while REX2 may also encode any
/// legacy GPR.
pub(crate) fn x86_fsgsbase_shape_valid(kind: &OpKind) -> bool {
    let OpKind::X86FsGsBase {
        operand,
        base,
        width,
        requires_apx,
        ..
    } = kind
    else {
        return false;
    };
    let Some(index) = (match operand {
        VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index(),
        _ => None,
    }) else {
        return false;
    };
    matches!(
        base,
        VReg::Arch(ArchReg::X86(X86Reg::FsBase | X86Reg::GsBase))
    ) && matches!(width, OpWidth::W32 | OpWidth::W64)
        && (index < 16 || *requires_apx)
}

impl X86_64Lowerer {
    /// Lower FSGSBASE through `GuestRegs`. Dynamic #UD/#GP conditions branch to
    /// a precise native handoff at the original guest PC; the interpreter then
    /// delivers the architectural exception without a partial destination or
    /// segment-base commit.
    pub(crate) fn emit_x86_fsgsbase(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86FsGsBase requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_fsgsbase_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86FsGsBase".to_string(),
                operand: "requires a GPR operand, FS.base/GS.base, W32/W64, and APX for EGPRs"
                    .to_string(),
            });
        }
        let OpKind::X86FsGsBase {
            operand,
            base,
            write,
            width,
            requires_apx,
        } = &op.kind
        else {
            unreachable!("validated X86FsGsBase shape changed")
        };
        let operand_index = match operand {
            VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().unwrap(),
            _ => unreachable!("validated X86FsGsBase operand changed"),
        };
        let base_offset = match base {
            VReg::Arch(ArchReg::X86(X86Reg::FsBase)) => X86_GUEST_FS_BASE_OFFSET,
            VReg::Arch(ArchReg::X86(X86Reg::GsBase)) => X86_GUEST_GS_BASE_OFFSET,
            _ => unreachable!("validated X86FsGsBase base changed"),
        };

        // Publish every identity-mapped GPR before using RAX/RDX/RCX as
        // scratch. The canonical RSP/RBP/EGPR slots are already state-backed.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let mut fault_branches = Vec::with_capacity(3);

        // CR4.FSGSBASE=0 => #UD.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+disp32],imm32
        self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
        self.code.emit_u32(1 << 16);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));

        // A REX2 encoding additionally requires the configured APX feature.
        if *requires_apx {
            self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+disp32],0
            self.code.emit_u32(X86_GUEST_APX_ENABLED_OFFSET as u32);
            self.code.emit_u8(0);
            fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));
        }

        if *write {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(
                    PhysReg::Rdx,
                    PhysReg::Rax,
                    i32::from(operand_index) * 8,
                    *width,
                );
            }
            if *width == OpWidth::W64 {
                // RCX = sign_extend_48(RDX); mismatch => non-canonical #GP(0).
                self.code.emit_bytes(&[
                    0x48, 0x89, 0xD1, // mov rcx,rdx
                    0x48, 0xC1, 0xE1, 0x10, // shl rcx,16
                    0x48, 0xC1, 0xF9, 0x10, // sar rcx,16
                    0x48, 0x39, 0xD1, // cmp rcx,rdx
                ]);
                fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
            }
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rax, base_offset, PhysReg::Rdx, OpWidth::W64);
        } else {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, base_offset, *width);
            }
            self.emit_store_gpr_slot_from_reg(operand_index, PhysReg::Rdx, *width)?;
        }

        // RAX still holds the state pointer. Preserve it in RCX before an RBP
        // destination synchronizes the prologue's saved guest-RBP word.
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        if !*write && operand_index == 5 {
            self.emit_sync_saved_rbp_from_state(PhysReg::Rcx);
        }
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // All dynamic faults are non-committing and restart this instruction.
        let fault = self.code.position();
        for branch in fault_branches {
            let rel = fault as i64 - branch as i64 - 4;
            if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
                return Err(LowerError::RelocationOutOfRange {
                    offset: branch,
                    target: fault,
                });
            }
            self.code.patch_i32(branch, rel as i32);
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D);
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
