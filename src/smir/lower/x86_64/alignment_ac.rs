//! Dynamic, fault-precise x86 alignment-check lowering.

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{Address, DispSize, OpWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_AC_FLAG_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET,
    X86_GUEST_CS_L_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

pub(crate) fn x86_check_alignment_ac_shape_valid(op: &SmirOp) -> bool {
    matches!(
        &op.kind,
        OpKind::X86CheckAlignmentAc {
            addr,
            alignment: 16,
            ..
        } if op.x86_hint.is_none() && addr.is_x86_state_backed_shape()
    )
}

impl X86_64Lowerer {
    /// Validate the complete 16-byte guest linear range before translation,
    /// then apply the live CR0.AM/RFLAGS.AC/CPL alignment predicate. A failing
    /// condition deoptimizes at the source PC so direct replay can select
    /// #GP(0), #SS(0), or #AC(0) with architectural priority.
    pub(crate) fn emit_x86_check_alignment_ac(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86CheckAlignmentAc requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_check_alignment_ac_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86CheckAlignmentAc".to_string(),
                operand: "requires an unhinted state-backed 16-byte address".to_string(),
            });
        }
        let OpKind::X86CheckAlignmentAc {
            addr, alignment, ..
        } = &op.kind
        else {
            unreachable!("validated X86CheckAlignmentAc operation changed kind");
        };
        debug_assert_eq!(*alignment, 16);

        const CR0_AM: i64 = 1 << 18;

        // Snapshot live identity-mapped GPRs. RSP/RBP/EGPR values are already
        // canonical in GuestRegs and address evaluation reads that snapshot.
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);
        self.emit_x86_state_address_rsi(addr)?;

        let mut faults = Vec::with_capacity(4);
        let mut success = Vec::with_capacity(4);

        // Compatibility mode does not apply 64-bit canonical-address checks.
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cs_l],0
        self.code.emit_u32(X86_GUEST_CS_L_OFFSET as u32);
        self.code.emit_u8(0);
        let skip_canonical = self.emit_jcc_placeholder(X86Cond::E);

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            // RDX = sign_extend_48(start); mismatch is noncanonical.
            emitter.emit_mov_rr(PhysReg::Rdx, PhysReg::Rsi, OpWidth::W64);
            emitter.emit_shl_ri(PhysReg::Rdx, 16, OpWidth::W64);
            emitter.emit_sar_ri(PhysReg::Rdx, 16, OpWidth::W64);
            emitter.emit_cmp_rr(PhysReg::Rdx, PhysReg::Rsi, OpWidth::W64);
        }
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rdx, PhysReg::Rsi, OpWidth::W64);
            emitter.emit_add_ri(PhysReg::Rdx, i64::from(*alignment - 1), OpWidth::W64);
        }
        // Wrapping the final byte below the first byte invalidates the range.
        faults.push(self.emit_jcc_placeholder(X86Cond::B));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rdx, OpWidth::W64);
            emitter.emit_shl_ri(PhysReg::Rdi, 16, OpWidth::W64);
            emitter.emit_sar_ri(PhysReg::Rdi, 16, OpWidth::W64);
            emitter.emit_cmp_rr(PhysReg::Rdi, PhysReg::Rdx, OpWidth::W64);
        }
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.patch_rel32_to_current(skip_canonical)?;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_CR0_OFFSET,
                DispSize::Auto,
                CR0_AM,
                OpWidth::W64,
            );
        }
        success.push(self.emit_jcc_placeholder(X86Cond::E));
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],3
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(3);
        success.push(self.emit_jcc_placeholder(X86Cond::Ne));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_mi_disp(
                PhysReg::Rax,
                X86_GUEST_AC_FLAG_OFFSET,
                DispSize::Auto,
                1,
                OpWidth::W64,
            );
        }
        success.push(self.emit_jcc_placeholder(X86Cond::E));
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_ri(PhysReg::Rsi, i64::from(*alignment - 1), OpWidth::W64);
        }
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));

        // Successful path, including dynamically disabled alignment checking.
        for branch in success {
            self.patch_rel32_to_current(branch)?;
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        for fault in faults {
            self.patch_rel32_to_current(fault)?;
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);
        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
