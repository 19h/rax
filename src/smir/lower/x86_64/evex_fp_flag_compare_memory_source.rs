//! Helper-backed EVEX floating-point flag-compare memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{SignExtend, VReg};
use crate::smir::lower::LowerError;

impl X86_64Lowerer {
    /// Fuse one exact EVEX scalar floating-point flag-comparison memory-source
    /// decomposition.
    ///
    /// The scalar MMU helper stages the unconditional 2/4/8-byte Type-E3NF
    /// source in a 16-byte nonarchitectural host-stack slot. A byte-validated
    /// EVEX `[rsp]` replay then sets RFLAGS and accrues guest MXCSR status using
    /// the original COMI/UCOMI policy and LLIG image. Helper faults occur
    /// before native comparison and therefore commit neither flags nor MXCSR.
    pub(crate) fn try_lower_jit_evex_fp_flag_compare_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_fp_flag_compare_memory_sequence(
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
                op: "EVEX floating-point flag compare memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX XMM0-XMM31 state".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } if *width == sequence.encoding.memory_width => addr,
            _ => unreachable!("validated EVEX flag compare sequence owns its scalar load"),
        };
        self.emit_evex_scalar_memory_stack_replay(
            block.ops[index].guest_pc,
            address,
            sequence.encoding.memory_width,
            None,
            sequence.encoding.stack_instruction,
        )?;
        Ok(Some(sequence.consumed))
    }
}
