//! Helper-backed EVEX `VMOVNTDQA` memory-source lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VReg;
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Fuse one exact EVEX.128/256/512 `VMOVNTDQA` memory source.
    ///
    /// The explicit width-matched alignment guard exits at the current guest
    /// PC before any memory helper call. On the aligned path the precise
    /// vector helper commits the complete architectural destination only
    /// after the full 16-, 32-, or 64-byte read succeeds. The non-temporal
    /// placement hint has no architectural state effect in the SMIR memory
    /// model.
    pub(crate) fn try_lower_jit_evex_movntdqa_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_movntdqa_memory_sequence(
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
                op: "EVEX VMOVNTDQA memory".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 VMOVNTDQA".to_string(),
            });
        }
        let (guard_address, alignment) = match &block.ops[index].kind {
            OpKind::X86CheckAlignment { addr, alignment } => (addr, *alignment),
            _ => unreachable!("validated EVEX VMOVNTDQA starts with an alignment guard"),
        };
        let load_address = match &block.ops[index + 1].kind {
            OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated EVEX VMOVNTDQA owns a vector load"),
        };

        self.emit_x86_check_alignment(block.ops[index].guest_pc, guard_address, alignment)?;
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            sequence.encoding.destination,
            load_address,
            sequence.encoding.width.bytes(),
            true,
            true,
        )?;
        Ok(Some(sequence.consumed))
    }
}
