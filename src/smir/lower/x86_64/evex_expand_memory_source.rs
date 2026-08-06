//! Helper-backed EVEX packed expand memory lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::{
    EVEX_E4_MASKED_VECTOR_FRAME_SIZE, EVEX_E4_MASKED_VECTOR_STAGING_OFFSET,
};
use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexExpandMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, SignExtend, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

/// The masked E4 frame reserves bytes 0..64 for the dense vector and 64..72
/// for one scalar helper result. Its final qword retains popcount(k & KL).
const EXPAND_ACTIVE_COUNT_OFFSET: i32 = 72;

impl X86_64Lowerer {
    #[allow(clippy::too_many_arguments)]
    fn emit_evex_expand_dense_slot_helper(
        &mut self,
        guest_pc: u64,
        address: &crate::smir::ir::types::Address,
        slot: usize,
        memory_width: crate::smir::ir::types::MemWidth,
        copy_width: OpWidth,
        lane_bytes: i32,
    ) -> Result<(), LowerError> {
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push guest RAX
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rax,
                PhysReg::Rsp,
                16 + EXPAND_ACTIVE_COUNT_OFFSET,
                OpWidth::W64,
            );
            emitter.emit_cmp_ri(
                PhysReg::Rax,
                i64::try_from(slot).expect("at most 64 packed expand lanes"),
                OpWidth::W64,
            );
        }
        // Slot n is architecturally read iff popcount(KL) > n.
        let inactive = self.emit_jcc_placeholder(X86Cond::Be);
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags

        let slot_offset = i32::try_from(slot).expect("at most 64 packed expand lanes") * lane_bytes;
        self.emit_jit_mem_op_linear_offset(
            guest_pc,
            true,
            None,
            Some(16 + EVEX_E4_MASKED_VECTOR_STAGING_OFFSET),
            None,
            None,
            None,
            address,
            memory_width,
            SignExtend::Zero,
            EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
            slot_offset,
        )?;

        // Copy exactly one helper result into its dense stack-vector slot.
        self.code.emit_u8(0x50); // push guest RAX
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rax,
                PhysReg::Rsp,
                8 + EVEX_E4_MASKED_VECTOR_STAGING_OFFSET,
                copy_width,
            );
            emitter.emit_mov_mr(PhysReg::Rsp, 8 + slot_offset, PhysReg::Rax, copy_width);
        }
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(inactive)?;
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags
        self.patch_rel32_to_current(done)?;
        Ok(())
    }

    fn emit_evex_masked_expand_memory_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexExpandMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated packed expand sequence starts with LEA"),
        };
        let mask = sequence
            .encoding
            .writemask
            .expect("validated masked packed expand replay");
        let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as usize;
        let (memory_width, copy_width, lane_bytes) =
            Self::evex_e4_memory_element_widths(sequence.encoding.elem)?;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(
                PhysReg::Rsp,
                PhysReg::Rsp,
                -EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
            );
            for offset in (0..64).step_by(8) {
                emitter.emit_mov_mi_disp(
                    PhysReg::Rsp,
                    offset,
                    crate::smir::ir::types::DispSize::Auto,
                    0,
                    OpWidth::W64,
                );
            }
        }

        // Compute the dense source length once. The repository's x86-64-v3
        // baseline includes POPCNT; RAX and architectural flags are restored
        // before any helper or replay instruction executes.
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_opmask_mask_to_rax64(mask);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            if lanes < 64 {
                emitter.emit_and_ri(PhysReg::Rax, ((1u64 << lanes) - 1) as i64, OpWidth::W64);
            }
            emitter.emit_popcnt(PhysReg::Rax, PhysReg::Rax, OpWidth::W64);
            emitter.emit_mov_mr(
                PhysReg::Rsp,
                16 + EXPAND_ACTIVE_COUNT_OFFSET,
                PhysReg::Rax,
                OpWidth::W64,
            );
        }
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags

        for slot in 0..lanes {
            self.emit_evex_expand_dense_slot_helper(
                block.ops[index].guest_pc,
                address,
                slot,
                memory_width,
                copy_width,
                lane_bytes,
            )?;
        }
        self.code.emit_bytes(stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, EVEX_E4_MASKED_VECTOR_FRAME_SIZE);
        }
        Ok(())
    }

    /// Fuse one exact VEXPANDPS/PD or VPEXPANDB/W/D/Q memory decomposition.
    ///
    /// Unmasked forms issue one full-vector helper read before a register
    /// replay. Masked forms read exactly the dense prefix selected by
    /// popcount(KL), in ascending element order, into nonarchitectural stack
    /// storage. The native expand executes only after every required guest
    /// read succeeds, so a helper fault exits at the source PC without
    /// committing destination, flags, MXCSR, or other guest state.
    pub(crate) fn try_lower_jit_evex_expand_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_expand_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        if self.avx_ymm16_vector_state {
            return Err(LowerError::InvalidOperand {
                op: "EVEX packed expand memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 expand state".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexExpandMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Lea { addr, .. } => addr,
                    _ => unreachable!("validated packed expand sequence starts with LEA"),
                };
                self.emit_jit_vector_mem_helper(
                    block.ops[index].guest_pc,
                    true,
                    X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                    address,
                    sequence.memory_size,
                    true,
                    true,
                )?;
                let scratch_reg = Self::evex_e4_memory_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexExpandMemoryReplay::MaskedVector { stack_instruction } => {
                self.emit_evex_masked_expand_memory_replay(
                    block,
                    index,
                    sequence,
                    stack_instruction,
                )?;
            }
        }
        Ok(Some(sequence.consumed))
    }
}
