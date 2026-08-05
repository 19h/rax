//! Helper-backed EVEX vector-chunk insert memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_chunk_insert_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            _ => unreachable!("validated EVEX chunk-insert source width"),
        }
    }

    /// Fuse one exact EVEX VINSERTF*/VINSERTI* memory decomposition.
    ///
    /// Type E6NF requires one complete 16/32-byte helper access before any
    /// destination update, including when every writemask lane is inactive.
    /// The tuple is replayed from an otherwise unused low vector register.
    pub(crate) fn try_lower_jit_evex_chunk_insert_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_chunk_insert_memory_sequence(
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
                op: "EVEX vector-chunk insert memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 state".to_string(),
            });
        }

        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated EVEX chunk insert starts with VLoad"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            sequence.encoding.memory_size,
            true,
            true,
        )?;
        let scratch_reg = Self::evex_chunk_insert_phys_reg(
            sequence.encoding.scratch,
            sequence.encoding.chunk_width,
        );
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.chunk_width);
        self.code
            .emit_bytes(sequence.encoding.register_instruction.as_slice());
        self.emit_jit_vector_scratch_restore(sequence.encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX

        Ok(Some(sequence.consumed))
    }
}
