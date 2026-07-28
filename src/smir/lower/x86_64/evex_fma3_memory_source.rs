//! Helper-backed EVEX packed FMA3 memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_fma3_memory_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX packed FMA3 vector width"),
        }
    }

    /// Fuse the exact `VLoad`/`X86Fma`/`VMov` decomposition for one unmasked,
    /// non-broadcast EVEX packed FMA3 memory source. The MMU helper commits
    /// only the nonarchitectural vector transfer slot. A byte-validated
    /// register-source rewrite consumes that value from a borrowed low vector
    /// register, which is restored completely before native execution
    /// continues.
    pub(crate) fn try_lower_jit_evex_packed_fma3_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_packed_fma3_memory_sequence(
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
                op: "EVEX packed FMA3 memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX FMA3".to_string(),
            });
        }
        let address = match &block.ops[index].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated EVEX FMA3 sequence starts with VLoad"),
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

        let scratch =
            Self::evex_fma3_memory_phys_reg(sequence.encoding.scratch, sequence.encoding.width);
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, sequence.encoding.width);
        self.code
            .emit_bytes(sequence.encoding.register_instruction.as_slice());
        self.emit_jit_vector_scratch_restore(sequence.encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX

        Ok(Some(sequence.consumed))
    }
}
