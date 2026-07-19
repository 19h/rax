//! Fault-precise, state-backed RDPKRU/WRPKRU lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_CR4_OFFSET, X86_GUEST_PKRU_OFFSET};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the fixed implicit-register shape emitted by the x86 lifter.
pub(crate) fn x86_pkru_shape_valid(kind: &OpKind) -> bool {
    matches!(
        kind,
        OpKind::X86Pkru {
            eax: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            ecx: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            edx: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            pkru: VReg::Arch(ArchReg::X86(X86Reg::Pkru)),
            ..
        }
    )
}

impl X86_64Lowerer {
    /// Lower RDPKRU/WRPKRU through `GuestRegs`. The host PKRU is never read or
    /// written. Dynamic #UD/#GP conditions branch to a precise native handoff
    /// at the original guest PC before any architectural destination commits.
    pub(crate) fn emit_x86_pkru(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86Pkru requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_pkru_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86Pkru".to_string(),
                operand: "requires implicit EAX, ECX, EDX, and PKRU operands".to_string(),
            });
        }
        let OpKind::X86Pkru { write, .. } = &op.kind else {
            unreachable!("validated X86Pkru shape changed")
        };

        // Snapshot all identity-mapped GPRs before borrowing RAX/RDX/RCX.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let mut fault_branches = Vec::with_capacity(3);

        // CR4.PKE=0 => #UD.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+disp32],imm32
        self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
        self.code.emit_u32(1 << 22);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::E));

        // Both instructions require ECX[31:0]=0; WRPKRU additionally requires
        // EDX[31:0]=0. The high halves are architecturally ignored.
        self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+disp32],0
        self.code.emit_u32(8); // GuestRegs.gpr[RCX]
        self.code.emit_u8(0);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
        if *write {
            self.code.emit_bytes(&[0x83, 0xB8]);
            self.code.emit_u32(16); // GuestRegs.gpr[RDX]
            self.code.emit_u8(0);
            fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));
        }

        if *write {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, 0, OpWidth::W32);
            emitter.emit_mov_mr(
                PhysReg::Rax,
                X86_GUEST_PKRU_OFFSET,
                PhysReg::Rdx,
                OpWidth::W32,
            );
        } else {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(
                    PhysReg::Rdx,
                    PhysReg::Rax,
                    X86_GUEST_PKRU_OFFSET,
                    OpWidth::W32,
                );
            }
            self.emit_store_gpr_slot_from_reg(0, PhysReg::Rdx, OpWidth::W32)?;
            self.code.emit_bytes(&[0x31, 0xD2]); // xor edx,edx
            self.emit_store_gpr_slot_from_reg(2, PhysReg::Rdx, OpWidth::W32)?;
        }

        // Restore the complete identity-mapped GPR set and byte-exact RFLAGS.
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // Faults are non-committing and restart the PKRU instruction.
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
