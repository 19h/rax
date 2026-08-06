//! Helper-backed Type-E9NF EVEX scalar-insert memory-source lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86ScalarInsertMemoryKind;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one complete canonical EVEX scalar-insert memory decomposition.
    ///
    /// The precise helper performs the unconditional 1-/2-/4-/8-byte E9NF
    /// load before any architectural state can change. `VPINSR*` transfers the
    /// loaded scalar through preserved host RAX; `VINSERTPS` consumes lane zero
    /// of a private low XMM scratch. The byte classifier has already removed
    /// helper-owned APX address controls and rewritten the register form.
    pub(crate) fn try_lower_jit_evex_scalar_insert_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_scalar_insert_memory_sequence(
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
                op: "EVEX scalar-insert memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX scalar-insert state".to_string(),
            });
        }
        let address = match &block.ops[index].kind {
            OpKind::Load { addr, .. } => addr,
            _ => unreachable!("validated EVEX scalar-insert sequence starts with Load"),
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

        let encoding = sequence.encoding;
        let scratch = PhysReg::Xmm(encoding.scratch);
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, VecWidth::V128);
        if encoding.kind != X86ScalarInsertMemoryKind::Vinsertps {
            let transfer_width = if encoding.kind == X86ScalarInsertMemoryKind::Vpinsrq {
                OpWidth::W64
            } else {
                OpWidth::W32
            };
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_sse_movd_q_rr(0x7E, scratch, PhysReg::Rax, transfer_width);
        }
        self.code
            .emit_bytes(encoding.register_instruction.as_slice());

        // Re-establish the GuestRegs base after VPINSR* reused RAX, restore the
        // borrowed complete architectural ZMM, and recover guest RAX.
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_restore(encoding.scratch);
        self.code.emit_u8(0x58); // pop guest RAX
        Ok(Some(sequence.consumed))
    }
}
