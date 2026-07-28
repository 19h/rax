//! Helper-backed VEX/EVEX VPCLMULQDQ memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn vpclmulqdq_memory_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated VPCLMULQDQ vector width"),
        }
    }

    /// Fuse the exact semantic decomposition for one VEX/EVEX VPCLMULQDQ
    /// memory source. The MMU helper commits only the nonarchitectural vector
    /// transfer slot. A byte-validated register-source rewrite consumes that
    /// value from a borrowed low vector register, which is restored completely
    /// before native execution continues.
    pub(crate) fn try_lower_jit_vpclmulqdq_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vpclmulqdq_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        if self.avx_ymm16_vector_state && !sequence.encoding.supports_avx_ymm16 {
            return Err(LowerError::InvalidOperand {
                op: "VPCLMULQDQ memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX or 512-bit VPCLMULQDQ"
                    .to_string(),
            });
        }
        let address = match &block.ops[index].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VPCLMULQDQ sequence starts with VLoad"),
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
            Self::vpclmulqdq_memory_phys_reg(sequence.encoding.scratch, sequence.encoding.width);
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, sequence.encoding.width);
        self.code
            .emit_bytes(sequence.encoding.register_instruction.as_slice());
        self.emit_jit_vector_scratch_restore(sequence.encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX

        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(sequence.encoding.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
