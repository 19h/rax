//! Helper-backed EVEX VPUNPCK*DQ/QDQ scalar-broadcast memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{SignExtend, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

impl X86_64Lowerer {
    /// Fuse one exact EVEX VPUNPCKLDQ/LQDQ/HDQ/HQDQ scalar-broadcast memory
    /// decomposition. The scalar MMU helper stages 4/8 bytes in a 16-byte
    /// nonarchitectural stack slot, and a byte-validated rewrite consumes
    /// `[rsp]{1toN}`. E4NF writemasking controls only the destination; the
    /// helper access is unconditional and completes before any commit.
    pub(crate) fn try_lower_jit_evex_broadcast_interleave_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_broadcast_interleave_memory_sequence(
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
                op: "EVEX broadcast VPUNPCK*DQ/QDQ memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX state".to_string(),
            });
        }

        let memory_index = index + sequence.memory_offset;
        let address = match &block.ops[memory_index].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } if *width == sequence.encoding.memory_width => addr,
            _ => unreachable!("validated EVEX broadcast interleave sequence owns its scalar load"),
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        self.emit_jit_mem_op(
            block.ops[index].guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            address,
            sequence.encoding.memory_width,
            SignExtend::Zero,
            16,
        )?;
        self.code
            .emit_bytes(sequence.encoding.stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }

        Ok(Some(sequence.consumed))
    }
}
