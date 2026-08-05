//! Helper-backed EVEX scalar floating-point-to-integer memory lowering.

use std::collections::HashMap;

use super::X86_64Lowerer;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::ir::{SmirBlock, X86NativeReplaySpan};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact EVEX `VCVT{T}{SS,SD,SH}2{SI,USI}` scalar memory source.
    ///
    /// The precise vector MMU helper commits only the nonarchitectural
    /// transfer slot. A borrowed XMM0 imports the 2-, 4-, or 8-byte source and
    /// feeds a byte-validated register rewrite, retaining native MXCSR
    /// rounding/status and exact 32-/64-bit GPR writes. CPU-level JIT admission
    /// requires every guest SIMD exception mask, so replay cannot escape as a
    /// host #XM while XMM0 or the host stack contains transient state.
    pub(crate) fn try_lower_jit_evex_scalar_fp_to_int_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_scalar_fp_to_int_memory_sequence(
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
                op: "EVEX scalar FP-to-integer memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX state".to_string(),
            });
        }
        let address = match &block.ops[index].kind {
            OpKind::Load { addr, .. } => addr,
            _ => unreachable!("validated EVEX scalar FP-to-integer sequence starts with a load"),
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

        // Import the precise helper result without exposing guest RAX.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(PhysReg::Xmm(0), VecWidth::V128);
        self.code.emit_u8(0x58); // pop guest RAX

        self.emit_native_replay_span(&X86NativeReplaySpan {
            end: index + sequence.consumed,
            instruction: sequence.encoding.register_instruction,
            needs_avx512vl: false,
            needs_avx512dq: false,
            needs_avx512fp16: sequence.encoding.needs_avx512fp16,
            preserve_mxcsr_de: false,
        });

        // RAX may itself hold the conversion result. Preserve its live value
        // while borrowing it as the state base for the XMM0 restoration.
        self.code.emit_u8(0x50); // push post-conversion guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_restore(0);
        self.code.emit_u8(0x58); // pop post-conversion guest RAX
        Ok(Some(sequence.consumed))
    }
}
