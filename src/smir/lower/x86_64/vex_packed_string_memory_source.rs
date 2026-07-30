//! Helper-backed VEX packed-string memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact VEX packed-string memory-source operation.
    ///
    /// The MMU helper commits only the nonarchitectural vector transfer slot.
    /// The exact register-source rewrite consumes that value from a borrowed
    /// low XMM register. RAX is restored before replay because explicit-length
    /// forms read EAX/RAX; the borrowed vector is restored afterwards without
    /// modifying the comparison result flags or ECX index result.
    pub(crate) fn try_lower_jit_vex_packed_string_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_packed_string_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VEX packed-string sequence starts with a vector load"),
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

        let encoding = sequence.encoding;
        let scratch = PhysReg::Xmm(encoding.scratch);
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, VecWidth::V128);
        self.code.emit_u8(0x58); // restore explicit length source RAX

        self.code
            .emit_bytes(encoding.register_instruction.as_slice());

        // MOV/PUSH/POP and VMOVDQU do not alter the packed-string result flags.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_restore(encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX

        if encoding.kind.returns_mask() && self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(0);
        }
        Ok(Some(sequence.consumed))
    }
}
