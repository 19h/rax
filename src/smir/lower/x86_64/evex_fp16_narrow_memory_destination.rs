//! Helper-backed EVEX `VCVTPS2PH` memory-destination lowering.

use super::X86_64Lowerer;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VecWidth;
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_MXCSR_OFFSET, X86_GUEST_VECTOR_SCRATCH_OFFSET,
    X86_JIT_VECTOR_MASKED_WORD_SCRATCH_BASE,
};

/// The low 32 bytes of `vector_scratch` carry the largest FP16 store payload.
/// Its last dword is disjoint temporary space for the post-conversion MXCSR.
const POST_CONVERSION_MXCSR_OFFSET: i32 = X86_GUEST_VECTOR_SCRATCH_OFFSET + 60;

impl X86_64Lowerer {
    /// Fuse one exact EVEX `VCVTPS2PH` memory destination.
    ///
    /// The byte-rewritten native conversion targets a borrowed low vector
    /// register while retaining its architectural writemask. The complete
    /// borrowed ZMM carrier and original MXCSR are synchronized before replay;
    /// the active conversion results and post-conversion MXCSR are then staged
    /// in disjoint nonarchitectural scratch. Native MXCSR and the borrowed ZMM
    /// are restored before the helper performs the sole memory-commit phase.
    ///
    /// The helper proves every active E11 two-byte lane is writable ordinary
    /// RAM, performs no access for inactive lanes, and commits only after every
    /// active access is valid.
    /// A helper failure therefore exposes neither memory, MXCSR, nor vector
    /// changes and exits at the instruction PC. CPU-level admission requires
    /// every guest SIMD exception mask, preventing a host #XM/SIGFPE during
    /// native conversion.
    pub(crate) fn try_lower_jit_evex_fp16_narrow_memory_destination(
        &mut self,
        block: &SmirBlock,
        index: usize,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_fp16_narrow_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
        ) else {
            return Ok(None);
        };
        if self.avx_ymm16_vector_state {
            return Err(LowerError::InvalidOperand {
                op: "EVEX VCVTPS2PH memory destination".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 source/opmask state"
                    .to_string(),
            });
        }
        let address = match &block.ops[index].kind {
            OpKind::X86PackedFpConvertStore { addr, .. } => addr,
            _ => unreachable!("validated EVEX VCVTPS2PH memory sequence"),
        };
        let encoding = sequence.encoding;

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_state_backed_xmm_sync(encoding.scratch, true);
        self.emit_fp16_narrow_mxcsr_transfer(X86_GUEST_MXCSR_OFFSET, true);
        self.code
            .emit_bytes(encoding.register_instruction.as_slice());
        let result_register = match encoding.result_width {
            VecWidth::V128 => PhysReg::Xmm(encoding.scratch),
            VecWidth::V256 => PhysReg::Ymm(encoding.scratch),
            _ => unreachable!("validated EVEX VCVTPS2PH result width"),
        };
        self.emit_jit_vector_scratch_store(result_register, encoding.result_width);
        self.emit_fp16_narrow_mxcsr_transfer(POST_CONVERSION_MXCSR_OFFSET, true);
        self.emit_jit_vector_scratch_restore(encoding.scratch);
        self.emit_fp16_narrow_mxcsr_transfer(X86_GUEST_MXCSR_OFFSET, false);
        self.code.emit_u8(0x58); // pop guest RAX

        let store_tag =
            X86_JIT_VECTOR_MASKED_WORD_SCRATCH_BASE + u32::from(encoding.writemask.unwrap_or(0));
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            false,
            store_tag as u8,
            address,
            encoding.memory_size,
            false,
            true,
        )?;

        // The helper returned only after committing every active lane.
        // Re-establish the FP status image produced by the conversion.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_fp16_narrow_mxcsr_transfer(POST_CONVERSION_MXCSR_OFFSET, false);
        self.code.emit_u8(0x58); // pop guest RAX

        Ok(Some(sequence.consumed))
    }
}
