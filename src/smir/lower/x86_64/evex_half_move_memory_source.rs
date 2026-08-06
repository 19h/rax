//! Helper-backed EVEX.128 high/low 64-bit lane memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{MemWidth, VReg};
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Fuse one exact `VMOVLPS`/`VMOVLPD`/`VMOVHPS`/`VMOVHPD` EVEX memory
    /// decomposition.
    ///
    /// The checked 8-byte guest load completes before the byte-validated EVEX
    /// stack replay can update its destination. A helper fault therefore exits
    /// at the original guest PC with no architectural vector-state commit.
    pub(crate) fn try_lower_jit_evex_half_move_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_half_move_memory_sequence(
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
                op: "EVEX high/low half-move memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX XMM0-XMM31 state".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load {
                addr,
                width: MemWidth::B8,
                ..
            } => addr,
            _ => unreachable!("validated EVEX half-move sequence owns its 8-byte load"),
        };
        self.emit_evex_scalar_memory_stack_replay(
            block.ops[index].guest_pc,
            address,
            MemWidth::B8,
            None,
            sequence.encoding.stack_instruction,
        )?;
        Ok(Some(sequence.consumed))
    }

    /// Fuse one exact `VMOVLPS`/`VMOVLPD`/`VMOVHPS`/`VMOVHPD` EVEX memory
    /// destination decomposition.
    ///
    /// The byte-validated EVEX instruction first transfers the selected live
    /// source qword into a nonarchitectural stack slot. The precise 8-byte MMU
    /// helper then commits that slot to guest memory or exits at the original
    /// guest PC. Neither success nor failure modifies architectural vector
    /// state, RFLAGS, or MXCSR.
    pub(crate) fn try_lower_jit_evex_half_move_memory_store(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_half_move_store_sequence(
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
                op: "EVEX high/low half-move memory store".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX XMM0-XMM31 state".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Store {
                addr,
                width: MemWidth::B8,
                ..
            } => addr,
            _ => unreachable!("validated EVEX half-move sequence owns its 8-byte store"),
        };
        self.emit_evex_scalar_memory_stack_store(
            block.ops[index].guest_pc,
            address,
            MemWidth::B8,
            None,
            sequence.encoding.stack_instruction,
        )?;
        Ok(Some(sequence.consumed))
    }
}
