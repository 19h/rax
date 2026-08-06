//! Helper-backed EVEX duplicate-move memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_duplicate_move_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX duplicate-move width"),
        }
    }

    /// Fuse one exact EVEX VMOVSLDUP/VMOVSHDUP/VMOVDDUP memory source.
    ///
    /// The E4NF/E5NF tuple is read once, unconditionally, into the helper
    /// transfer slot. A byte-validated register-source replay then applies
    /// duplication, destination writemasking, zeroing, and upper-lane clearing
    /// atomically. EVEX.128 VMOVDDUP reads exactly 8 bytes and zero-fills the
    /// unused transfer-slot tail before the replay.
    pub(crate) fn try_lower_jit_evex_duplicate_move_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_duplicate_move_memory_sequence(
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
                op: "EVEX duplicate move memory".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 duplicate moves".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated EVEX duplicate move owns its memory source"),
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

        let scratch =
            Self::evex_duplicate_move_phys_reg(sequence.encoding.scratch, sequence.encoding.width);
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
