//! Helper-backed VEX scalar and 128-bit chunk extraction to memory.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::runtime::X86JitVexExtractMemorySequence;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX, X86_STATE_PTR_AT_RBP};

impl X86_64Lowerer {
    /// Fuse one exact VEX scalar or 128-bit chunk extraction whose destination
    /// is guest memory.
    ///
    /// Native extraction writes only preserved host RAX or a borrowed XMM
    /// register. The selected bytes are copied to the nonarchitectural
    /// transfer slot, all borrowed architectural carriers are restored, and
    /// the precise helper performs the sole 1-/2-/4-/8-/16-byte guest write.
    /// A helper failure therefore exits at the instruction PC without
    /// committing guest registers, flags, MXCSR, or memory.
    pub(crate) fn try_lower_jit_vex_extract_memory_destination(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_extract_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        let (address, size) = match sequence {
            X86JitVexExtractMemorySequence::Scalar(sequence) => {
                let address = match &block.ops[index + 1].kind {
                    OpKind::Store { addr, .. } => addr,
                    _ => unreachable!("validated VEX scalar extraction ends with Store"),
                };
                let encoding = sequence.encoding;
                self.code.emit_u8(0x50); // push guest RAX
                self.code
                    .emit_bytes(encoding.register_instruction.as_slice());
                self.code.emit_u8(0x51); // push guest RCX
                self.code.emit_bytes(&[0x48, 0x8B, 0x4D]);
                self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rcx,[rbp+state]
                self.emit_jit_vector_scratch_gpr_store(encoding.memory_width);
                self.code.emit_u8(0x59); // pop guest RCX
                self.code.emit_u8(0x58); // pop guest RAX
                (address, encoding.memory_width.bytes())
            }
            X86JitVexExtractMemorySequence::Chunk(sequence) => {
                let address = match &block.ops[index + 6].kind {
                    OpKind::VStore { addr, .. } => addr,
                    _ => unreachable!("validated VEX chunk extraction ends with VStore"),
                };
                let encoding = sequence.encoding;
                self.code.emit_u8(0x50); // push guest RAX
                self.code
                    .emit_bytes(encoding.register_instruction.as_slice());
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_store(PhysReg::Xmm(encoding.scratch), VecWidth::V128);
                self.emit_jit_vector_scratch_restore(encoding.scratch);
                self.code.emit_u8(0x58); // pop guest RAX
                (address, 16)
            }
        };

        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            false,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            size,
            false,
            true,
        )?;
        Ok(Some(sequence.consumed()))
    }
}
