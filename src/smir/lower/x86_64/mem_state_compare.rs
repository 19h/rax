//! Fused memory-source compare against a state-backed GPR.

use super::*;

impl X86_64Lowerer {
    /// Lower `Load v,[mem]; CMP|TEST` whose non-memory operand is guest RSP/RBP
    /// or an APX EGPR. The MMU helper stages its result on a caller frame, the
    /// architectural operand is reloaded from its `GuestRegs` slot into scratch
    /// RAX, and only the compare itself writes flags — the surrounding MOV/LEA
    /// bookkeeping is flag-neutral, so the published flags are exactly the
    /// architectural ones.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_mem_state_compare(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_mem_state_compare_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        // Caller-frame layout after the flag-neutral reservation:
        //   [rsp+0]  zero-extended memory operand written by the load helper
        //   [rsp+24] complete architectural RAX
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
            emitter.emit_mov_mr(PhysReg::Rsp, 24, PhysReg::Rax, OpWidth::W64);
        }
        // The load helper's own PUSH RAX/PUSHFQ make caller [rsp+0] the active
        // [rsp+16]. A faulting load removes the whole caller frame and exits at
        // the guest PC without publishing flags.
        self.emit_jit_mem_op(
            sequence.guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            sequence.addr,
            sequence.mem_width,
            SignExtend::Zero,
            32,
        )?;

        self.emit_load_state_ptr_rax();
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rax,
                PhysReg::Rax,
                i32::from(sequence.state_index) * 8,
                OpWidth::W64,
            );
            if sequence.is_test {
                // TEST is commutative, so the memory-destination form covers
                // both architectural operand orders.
                emitter.emit_test_mr_disp(
                    PhysReg::Rsp,
                    0,
                    DispSize::Auto,
                    PhysReg::Rax,
                    sequence.width,
                );
            } else if sequence.memory_is_first {
                emitter.emit_alu_mem_disp(
                    0x38,
                    PhysReg::Rax,
                    PhysReg::Rsp,
                    0,
                    DispSize::Auto,
                    sequence.width,
                    X86AluEncoding::RmReg,
                );
            } else {
                emitter.emit_alu_mem_disp(
                    0x38,
                    PhysReg::Rax,
                    PhysReg::Rsp,
                    0,
                    DispSize::Auto,
                    sequence.width,
                    X86AluEncoding::RegRm,
                );
            }
            // MOV and LEA never write flags, so the compare's result survives.
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 24, OpWidth::W64);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        Ok(Some(2))
    }
}
