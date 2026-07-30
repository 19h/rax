//! Helper-backed VEX packed-conversion memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact `VLoad` plus classic VEX packed-conversion sequence.
    ///
    /// The MMU helper commits only the nonarchitectural transfer slot. A
    /// borrowed low XMM/YMM register carries its 8-/16-/32-byte result into a
    /// byte-preserving register rewrite of the original instruction, then is
    /// restored completely before continuation. Guest MXCSR remains active
    /// for native rounding and accrued IE/DE/PE status. A failed helper exits
    /// before any architectural destination or MXCSR state is modified.
    pub(crate) fn try_lower_jit_vex_packed_convert_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_vex_packed_convert_memory_sequence(
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
        let address = match &block.ops[index].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VEX packed conversion starts with a vector load"),
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

        let transfer_width = sequence.encoding.transfer_width();
        let scratch = match transfer_width {
            VecWidth::V128 => PhysReg::Xmm(sequence.encoding.scratch),
            VecWidth::V256 => PhysReg::Ymm(sequence.encoding.scratch),
            _ => unreachable!("validated VEX packed conversion transfer width"),
        };
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, transfer_width);
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
