//! Helper-backed EVEX scalar floating-point unary memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{SignExtend, VReg};
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Fuse one exact scalar special or approximate floating-point
    /// memory-source decomposition.
    ///
    /// The scalar helper stages the complete 2/4/8-byte source before a
    /// byte-validated `[rsp]` replay. A live K bit-0 guard suppresses the
    /// helper for inactive writemasks. The native instruction retains its
    /// imm8, dynamic MXCSR behavior, merge/zero masking, source-1 upper lanes,
    /// and destination upper-zeroing without borrowing a guest vector.
    pub(crate) fn try_lower_jit_evex_scalar_fp_unary_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_scalar_fp_unary_memory_sequence(
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
                op: "EVEX scalar floating-point unary memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX scalar unary operations"
                    .to_string(),
            });
        }
        let load_index = index + sequence.load_offset;
        let address = match &block.ops[load_index].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } if *width == sequence.encoding.memory_width => addr,
            OpKind::PredLoad {
                addr,
                width,
                signed: SignExtend::Zero,
                ..
            } if *width == sequence.encoding.memory_width => addr,
            _ => unreachable!("validated EVEX scalar unary sequence owns its scalar memory op"),
        };
        self.emit_evex_scalar_memory_stack_replay(
            block.ops[index].guest_pc,
            address,
            sequence.encoding.memory_width,
            sequence.encoding.writemask,
            sequence.encoding.stack_instruction,
        )?;
        Ok(Some(sequence.consumed))
    }
}
