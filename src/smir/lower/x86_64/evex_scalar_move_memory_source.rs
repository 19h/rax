//! Helper-backed EVEX scalar move memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexScalarMoveMemoryKind;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VReg;
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Fuse one exact EVEX `VMOVSH`, `VMOVSS`, or `VMOVSD` memory
    /// decomposition.
    ///
    /// Loads stage the precise scalar helper result and execute the original
    /// instruction against `[rsp]`, preserving merge/zero masking and clearing
    /// all destination bits above the scalar. Stores first transfer the live
    /// low vector lane to `[rsp]`, then commit the exact scalar through the
    /// helper. A live K[0] guard suppresses both helper access and store staging
    /// when masked off. Neither direction changes RFLAGS or MXCSR.
    pub(crate) fn try_lower_jit_evex_scalar_move_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_scalar_move_memory_sequence(
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
                op: "EVEX scalar move memory".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX scalar moves".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load { addr, .. }
            | OpKind::PredLoad { addr, .. }
            | OpKind::Store { addr, .. }
            | OpKind::PredStore { addr, .. } => addr,
            _ => unreachable!("validated EVEX scalar move sequence owns its memory operation"),
        };
        match sequence.encoding.kind {
            X86EvexScalarMoveMemoryKind::Load => self.emit_evex_scalar_memory_stack_replay(
                block.ops[index].guest_pc,
                address,
                sequence.encoding.memory_width,
                sequence.encoding.writemask,
                sequence.encoding.stack_instruction,
            )?,
            X86EvexScalarMoveMemoryKind::Store => self.emit_evex_scalar_memory_stack_store(
                block.ops[index].guest_pc,
                address,
                sequence.encoding.memory_width,
                sequence.encoding.writemask,
                sequence.encoding.stack_instruction,
            )?,
        }
        Ok(Some(sequence.consumed))
    }
}
