//! Fault-precise, state-backed lowering for x86 debug-register writes.

use crate::smir::ir::ops::{OpKind, SmirOp, X86DebugReg};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
    X86_GUEST_DR0_OFFSET, X86_GUEST_DR1_OFFSET, X86_GUEST_DR2_OFFSET, X86_GUEST_DR3_OFFSET,
    X86_GUEST_DR6_OFFSET, X86_GUEST_DR7_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the exact 64-bit GPR/debug-register shape emitted by the strict
/// x86-64 lifter. REX2 permits every GPR through R31; the preceding
/// `X86RequireApx` operation retains the source encoding's dynamic admission.
pub(crate) fn x86_write_debug_shape_valid(kind: &OpKind) -> bool {
    let OpKind::X86WriteDebug { src, debug } = kind else {
        return false;
    };
    matches!(
        src,
        VReg::Arch(ArchReg::X86(reg)) if reg.gpr_index().is_some_and(|index| index < 32)
    ) && matches!(
        debug,
        X86DebugReg::Dr0
            | X86DebugReg::Dr1
            | X86DebugReg::Dr2
            | X86DebugReg::Dr3
            | X86DebugReg::Dr4
            | X86DebugReg::Dr5
            | X86DebugReg::Dr6
            | X86DebugReg::Dr7
    )
}

impl X86_64Lowerer {
    /// Write guest debug state through `GuestRegs`; never execute the host's
    /// privileged `MOV DRn, r64` instruction.
    ///
    /// Dynamic CPL, CR4.DE, DR7.GD, and DR6/DR7 high-half failures restore
    /// every GPR and RFLAGS bit before handing off at the original guest PC.
    /// Successful writes execute a register-preserving CPUID barrier because
    /// Intel classifies MOV-to-DR as a privileged serializing instruction.
    pub(crate) fn emit_x86_write_debug(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86WriteDebug requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_write_debug_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86WriteDebug".to_string(),
                operand: "requires one x86 GPR source and DR0-DR7".to_string(),
            });
        }
        let OpKind::X86WriteDebug { src, debug } = &op.kind else {
            unreachable!("validated X86WriteDebug shape changed")
        };
        let source = match src {
            VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().unwrap(),
            _ => unreachable!("validated X86WriteDebug source changed"),
        };
        let debug_offset = match debug {
            X86DebugReg::Dr0 => X86_GUEST_DR0_OFFSET,
            X86DebugReg::Dr1 => X86_GUEST_DR1_OFFSET,
            X86DebugReg::Dr2 => X86_GUEST_DR2_OFFSET,
            X86DebugReg::Dr3 => X86_GUEST_DR3_OFFSET,
            X86DebugReg::Dr4 | X86DebugReg::Dr6 => X86_GUEST_DR6_OFFSET,
            X86DebugReg::Dr5 | X86DebugReg::Dr7 => X86_GUEST_DR7_OFFSET,
        };

        // Publish the complete source register image before using RAX/RDX/RCX
        // as scratch. RSP/RBP remain canonical in their state-backed slots.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rdx,
                PhysReg::Rax,
                i32::from(source) * 8,
                OpWidth::W64,
            );
        }

        let mut faults = Vec::with_capacity(4);

        // Real-address mode (CR0.PE=0) permits the write. Otherwise effective
        // CPL must be zero; GuestRegs.cpl maps virtual-8086 execution to CPL3.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],1
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        let real_mode = self.emit_jcc_placeholder(X86Cond::E);

        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.patch_rel32_to_current(real_mode)?;

        // DR4/DR5 are aliases only while CR4.DE is clear.
        if matches!(debug, X86DebugReg::Dr4 | X86DebugReg::Dr5) {
            self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr4],DE
            self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
            self.code.emit_u32(1 << 3);
            faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        }

        // General detect faults before the selected debug register changes.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+dr7],GD
        self.code.emit_u32(X86_GUEST_DR7_OFFSET as u32);
        self.code.emit_u32(1 << 13);
        faults.push(self.emit_jcc_placeholder(X86Cond::Ne));

        // Effective DR6/DR7 reject any nonzero source bit 63:32. DR4/DR5
        // inherit this rule through their aliases when CR4.DE is clear.
        if matches!(
            debug,
            X86DebugReg::Dr4 | X86DebugReg::Dr5 | X86DebugReg::Dr6 | X86DebugReg::Dr7
        ) {
            self.code.emit_bytes(&[0x48, 0x89, 0xD1]); // mov rcx,rdx
            self.code.emit_bytes(&[0x48, 0xC1, 0xE9, 0x20]); // shr rcx,32
            faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rax, debug_offset, PhysReg::Rdx, OpWidth::W64);
        }
        self.emit_x86_serialize();

        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // Fault path is completely non-committing. Direct replay owns exact
        // exception priority and the DR6.BD/DR7.GD delivery side effects.
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
