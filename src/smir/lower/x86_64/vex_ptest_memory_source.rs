//! Helper-backed VEX packed bit-test memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn vex_ptest_memory_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            _ => unreachable!("validated VEX packed bit-test width"),
        }
    }

    /// Fuse one exact packed bit-test memory-source decomposition.
    ///
    /// The precise MMU helper commits only the nonarchitectural vector
    /// transfer slot. A byte-validated register-source rewrite consumes the
    /// helper value from a borrowed low vector register, which is restored
    /// without modifying the resulting architectural flags.
    pub(crate) fn try_lower_jit_vex_ptest_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_ptest_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let load = &block.ops[index];
        let address = match &load.kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VEX packed bit-test sequence starts with VLoad"),
        };
        self.emit_jit_vector_mem_helper(
            load.guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            sequence.encoding.memory_size,
            true,
            true,
        )?;

        let encoding = sequence.encoding;
        let scratch = Self::vex_ptest_memory_phys_reg(encoding.scratch, encoding.width);
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, encoding.width);
        self.code
            .emit_bytes(encoding.register_instruction.as_slice());
        self.emit_ptest_defined_flag_canonicalization();
        self.emit_jit_vector_scratch_restore(encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX without modifying test flags

        Ok(Some(sequence.consumed))
    }
}
