//! Helper-backed F16C `VCVTPS2PH` memory-destination lowering.

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{DispSize, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_MXCSR_OFFSET, X86_GUEST_VECTOR_SCRATCH_OFFSET,
    X86_JIT_VECTOR_SCRATCH_INDEX,
};

/// The low 16 bytes of `vector_scratch` carry the FP16 store payload. Its last
/// dword is disjoint temporary space for the post-conversion MXCSR image.
const POST_CONVERSION_MXCSR_OFFSET: i32 = X86_GUEST_VECTOR_SCRATCH_OFFSET + 60;

impl X86_64Lowerer {
    pub(crate) fn emit_fp16_narrow_mxcsr_transfer(&mut self, offset: i32, store: bool) {
        self.code.emit_bytes(&[0x0F, 0xAE]);
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_modrm_mem_disp(
            if store { PhysReg::Rbx } else { PhysReg::Rdx },
            PhysReg::Rax,
            offset,
            DispSize::Disp32,
        );
    }

    /// Fuse one exact F16C VEX `VCVTPS2PH` memory destination.
    ///
    /// The native conversion targets a borrowed XMM register. Before it runs,
    /// the complete borrowed YMM/ZMM carrier and original live guest MXCSR are
    /// synchronized to `GuestRegs`. The converted low 8/16 bytes and the
    /// post-conversion MXCSR image are staged in disjoint nonarchitectural
    /// scratch slots. Native MXCSR is then restored to its original image
    /// before the MMU helper performs the sole memory commit.
    ///
    /// Consequently a helper fault exposes neither FP status nor memory or
    /// vector-register changes and restarts at the instruction PC. Only a
    /// successful helper return reloads the post-conversion MXCSR. CPU-level
    /// admission requires every guest SIMD exception mask, so the native
    /// conversion itself cannot escape as host #XM/SIGFPE.
    pub(crate) fn try_lower_jit_vex_fp16_narrow_memory_destination(
        &mut self,
        block: &SmirBlock,
        index: usize,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_fp16_narrow_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index].kind {
            OpKind::X86PackedFpConvertStore { addr, .. } => addr,
            _ => unreachable!("validated VCVTPS2PH memory sequence"),
        };
        let encoding = sequence.encoding;

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_state_backed_xmm_sync(encoding.scratch, true);
        self.emit_fp16_narrow_mxcsr_transfer(X86_GUEST_MXCSR_OFFSET, true);
        self.code
            .emit_bytes(encoding.register_instruction.as_slice());
        self.emit_jit_vector_scratch_store(PhysReg::Xmm(encoding.scratch), VecWidth::V128);
        self.emit_fp16_narrow_mxcsr_transfer(POST_CONVERSION_MXCSR_OFFSET, true);
        self.emit_jit_vector_scratch_restore(encoding.scratch);
        self.emit_fp16_narrow_mxcsr_transfer(X86_GUEST_MXCSR_OFFSET, false);
        self.code.emit_u8(0x58); // pop guest RAX

        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            false,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            encoding.memory_size,
            false,
            true,
        )?;

        // The helper returned only after committing the complete 8-/16-byte
        // store. Re-establish the status image produced by the conversion.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_fp16_narrow_mxcsr_transfer(POST_CONVERSION_MXCSR_OFFSET, false);
        self.code.emit_u8(0x58); // pop guest RAX

        Ok(Some(sequence.consumed))
    }
}
