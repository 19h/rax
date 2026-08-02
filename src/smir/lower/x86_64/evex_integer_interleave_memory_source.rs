//! Helper-backed EVEX VPUNPCK* full-vector memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_integer_interleave_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX integer-interleave vector width"),
        }
    }

    /// Fuse one exact EVEX VPUNPCKLBW/LWD/LDQ/LQDQ/HBW/HWD/HDQ/HQDQ Full Mem
    /// decomposition.
    ///
    /// Type E4NF/E4NF.nb requires one complete 16/32/64-byte helper access
    /// before any architectural destination update, including when every
    /// writemask lane is inactive. A byte-validated register form consumes the
    /// staged tuple from an otherwise unused low vector register; the
    /// nonarchitectural register is restored after replay.
    pub(crate) fn try_lower_jit_evex_integer_interleave_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_integer_interleave_memory_sequence(
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
                op: "EVEX integer-interleave Full Mem source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 state".to_string(),
            });
        }

        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated EVEX integer-interleave sequence starts with VLoad"),
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

        let scratch = sequence.encoding.scratch;
        let scratch_reg = Self::evex_integer_interleave_phys_reg(scratch, sequence.encoding.width);
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
        self.code
            .emit_bytes(sequence.encoding.register_instruction.as_slice());
        self.emit_jit_vector_scratch_restore(scratch);
        self.code.emit_u8(0x58); // pop guest RAX

        Ok(Some(sequence.consumed))
    }
}
