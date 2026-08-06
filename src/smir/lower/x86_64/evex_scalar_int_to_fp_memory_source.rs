//! Helper-backed EVEX scalar integer-to-floating-point memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, VReg};
use crate::smir::ir::{SmirBlock, X86NativeReplaySpan};
use crate::smir::lower::{
    LowerError, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_JIT_VECTOR_SCRATCH_INDEX,
};

impl X86_64Lowerer {
    /// Fuse one exact EVEX `VCVT{,U}SI2{SS,SD,SH}` scalar memory source.
    ///
    /// The precise vector MMU helper commits only the nonarchitectural
    /// transfer slot. Push/pop-preserved RAX imports the 4- or 8-byte value and
    /// feeds a byte-validated register rewrite, retaining native MXCSR
    /// rounding/status, merge-source behavior, and upper-vector clearing. A
    /// failed helper exits before any destination or MXCSR state is modified.
    pub(crate) fn try_lower_jit_evex_scalar_int_to_fp_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_scalar_int_to_fp_memory_sequence(
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
                op: "EVEX scalar integer-to-FP memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX state".to_string(),
            });
        }
        let address = match &block.ops[index].kind {
            OpKind::Load { addr, .. } => addr,
            _ => unreachable!("validated EVEX scalar integer-to-FP sequence starts with a load"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            sequence.encoding.memory_width.bytes(),
            true,
            true,
        )?;

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        if sequence.encoding.int_width == OpWidth::W64 {
            self.code.emit_u8(0x48);
        }
        self.code.emit_bytes(&[0x8B, 0x80]); // mov eax/rax,[rax+scratch]
        self.code.emit_u32(X86_GUEST_VECTOR_SCRATCH_OFFSET as u32);
        self.emit_native_replay_span(&X86NativeReplaySpan {
            end: index + sequence.consumed,
            instruction: sequence.encoding.register_instruction,
            needs_avx512vl: false,
            needs_avx512dq: false,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
            preserve_mxcsr_de: false,
        })?;
        self.code.emit_u8(0x58); // pop guest RAX
        Ok(Some(sequence.consumed))
    }
}
