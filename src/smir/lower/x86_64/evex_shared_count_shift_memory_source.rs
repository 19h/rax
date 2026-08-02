//! Helper-backed EVEX packed shared-count shift memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact packed AVX-512 shift whose shared 128-bit count operand
    /// is memory.
    ///
    /// Type E4NF.nb requires one unconditional 16-byte memory access even
    /// when every writemask bit is clear. The helper stages that complete
    /// operand, a byte-validated register rewrite consumes its low 64-bit
    /// count, and the architectural destination is not touched if the helper
    /// faults.
    pub(crate) fn try_lower_jit_evex_shared_count_shift_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_shared_count_shift_memory_sequence(
                block,
                index,
                true,
                &self.x86_instruction_bytes,
                virtual_definitions,
                virtual_uses,
            )
        else {
            return Ok(None);
        };
        if self.avx_ymm16_vector_state {
            return Err(LowerError::InvalidOperand {
                op: "EVEX shared-count shift memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 shifts".to_string(),
            });
        }

        let address = match &block.ops[index].kind {
            OpKind::VLoad {
                addr,
                width: VecWidth::V128,
                ..
            } => addr,
            _ => unreachable!("validated shared-count shift starts with a 128-bit VLoad"),
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
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(PhysReg::Xmm(sequence.encoding.scratch), VecWidth::V128);
        self.code
            .emit_bytes(sequence.encoding.register_instruction.as_slice());
        self.emit_jit_vector_scratch_restore(sequence.encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX
        Ok(Some(sequence.consumed))
    }
}
