//! Fault-precise, helper-backed SGDT/SIDT lowering.

use crate::smir::ir::ops::{OpKind, SmirOp, X86DescriptorTable, X86DescriptorTableStoreOp};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CR4_OFFSET,
    X86_GUEST_DESCRIPTOR_STORE_FN_OFFSET, X86_STATE_PTR_AT_RBP,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

/// Validate the exact state-backed memory shape emitted by the strict x86-64
/// lifter. EGPR address components require REX2/APX; REX2 may also encode a
/// legacy-only address and still requires the dynamic APX guard.
pub(crate) fn x86_descriptor_table_store_shape_valid(op: &SmirOp) -> bool {
    let OpKind::X86DescriptorTableStore(X86DescriptorTableStoreOp {
        addr, requires_apx, ..
    }) = &op.kind
    else {
        return false;
    };
    let uses_egpr = addr
        .regs()
        .iter()
        .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
    op.x86_hint.is_none() && addr.is_x86_state_backed_shape() && (!uses_egpr || *requires_apx)
}

impl X86_64Lowerer {
    /// Store guest GDTR/IDTR through the canonical MMU helper. Guard failures
    /// and helper failures restore every GPR and RFLAGS bit and deoptimize at
    /// the faulting guest PC before any architectural commit.
    pub(crate) fn emit_x86_descriptor_table_store(
        &mut self,
        op: &SmirOp,
    ) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86DescriptorTableStore requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !self.mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "X86DescriptorTableStore requires JIT MMU helpers".to_string(),
            });
        }
        if !x86_descriptor_table_store_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86DescriptorTableStore".to_string(),
                operand: "requires an unhinted state-backed x86 address, with APX for every EGPR"
                    .to_string(),
            });
        }
        let OpKind::X86DescriptorTableStore(store) = &op.kind else {
            unreachable!("validated descriptor-table store shape changed")
        };

        // Publish all identity-mapped GPRs before borrowing RAX/RSI/RDI/RDX.
        // The two pushes preserve flags and maintain SysV call alignment.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let mut guard_faults = Vec::with_capacity(2);
        if store.requires_apx {
            self.code.emit_bytes(&[0x83, 0xB8]); // cmp dword [rax+apx],0
            self.code.emit_u32(X86_GUEST_APX_ENABLED_OFFSET as u32);
            self.code.emit_u8(0);
            guard_faults.push(self.emit_jcc_placeholder(X86Cond::E));
        }

        // SourceArch::X86_64 is protected/long mode. UMIP blocks SGDT/SIDT
        // only when CR4.UMIP=1 and effective CPL is nonzero.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr4],UMIP
        self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
        self.code.emit_u32(1 << 11);
        let umip_clear = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        guard_faults.push(self.emit_jcc_placeholder(X86Cond::Ne));
        self.patch_rel32_to_current(umip_clear)?;

        self.emit_helper_call_state(PhysReg::Rax, true, self.preserve_vector_mem_helpers);
        self.emit_jit_mem_effective_address(&store.addr, false)?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rdi, PhysReg::Rax, OpWidth::W64);
            emitter.emit_mov_ri(
                PhysReg::Rdx,
                match store.table {
                    X86DescriptorTable::Gdt => 0,
                    X86DescriptorTable::Idt => 1,
                },
                OpWidth::W32,
            );
        }
        self.code.emit_u8(0xFC); // cld: platform ABI requires DF=0
        self.code.emit_u8(0xFF);
        self.code.emit_u8(0x90); // call qword [rax+descriptor_store_fn]
        self.code
            .emit_u32(X86_GUEST_DESCRIPTOR_STORE_FN_OFFSET as u32);

        self.code.emit_bytes(&[0x48, 0x8B, 0x4D]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rcx,[rbp+state_ptr]
        self.code.emit_bytes(&[0x48, 0x85, 0xC0]); // test rax,rax
        let helper_fault = self.emit_jcc_placeholder(X86Cond::E);

        self.emit_helper_call_state(PhysReg::Rcx, false, self.preserve_vector_mem_helpers);
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // Guard failures retain the state pointer in RAX; helper failure has
        // already reloaded it into RCX. Both paths restore the same snapshot.
        for branch in guard_faults {
            self.patch_rel32_to_current(branch)?;
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.patch_rel32_to_current(helper_fault)?;
        self.emit_helper_call_state(PhysReg::Rcx, false, self.preserve_vector_mem_helpers);
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D);
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
