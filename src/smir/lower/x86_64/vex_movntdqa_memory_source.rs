//! Helper-backed VEX `VMOVNTDQA` memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VReg;
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Fuse one exact VEX.128/256 `VMOVNTDQA` memory source.
    ///
    /// The explicit alignment guard exits at the current guest PC on failure,
    /// before the MMU helper is called. On the aligned path the precise vector
    /// helper commits the complete architectural destination only after the
    /// full 16-byte or 32-byte read succeeds. The non-temporal placement hint
    /// has no architectural state effect in the SMIR memory model.
    pub(crate) fn try_lower_jit_vex_movntdqa_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_movntdqa_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let (guard_address, alignment) = match &block.ops[index].kind {
            OpKind::X86CheckAlignment { addr, alignment } => (addr, *alignment),
            _ => unreachable!("validated VEX VMOVNTDQA sequence starts with an alignment guard"),
        };
        let load_address = match &block.ops[index + 1].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VEX VMOVNTDQA sequence contains a vector load"),
        };

        self.emit_x86_check_alignment(block.ops[index].guest_pc, guard_address, alignment)?;
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            sequence.encoding.destination,
            load_address,
            sequence.encoding.width.bytes() as u32,
            true,
            true,
        )?;
        Ok(Some(sequence.consumed))
    }
}
