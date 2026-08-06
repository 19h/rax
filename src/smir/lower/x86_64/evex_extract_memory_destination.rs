//! Helper-backed EVEX scalar and vector-chunk extraction to memory.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{DispSize, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::runtime::X86JitEvexExtractMemorySequence;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX, X86_STATE_PTR_AT_RBP};

impl X86_64Lowerer {
    /// Copy one private 128- or 256-bit stack result into the
    /// nonarchitectural vector-helper slot while preserving guest RAX,
    /// RFLAGS, and the complete architectural ZMM0.
    fn emit_evex_extract_stack_to_vector_scratch(&mut self, width: VecWidth) {
        let scratch = match width {
            VecWidth::V128 => PhysReg::Xmm(0),
            VecWidth::V256 => PhysReg::Ymm(0),
            _ => unreachable!("validated EVEX extraction chunk width"),
        };
        self.code.emit_u8(0x50); // push guest RAX
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_vex_prefix(
                X86VecMap::Map0F,
                X86SsePrefix::Rep,
                width,
                false,
                scratch.vec_ext(),
                0,
                PhysReg::Rsp.vec_ext(),
                0,
            );
            emitter.code.emit_u8(0x6F); // VMOVDQU xmm/ymm,[rsp+8]
            emitter.emit_modrm_mem_disp(scratch, PhysReg::Rsp, 8, DispSize::Disp8);
        }
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_store(scratch, width);
        self.emit_jit_vector_scratch_restore(0);
        self.code.emit_u8(0x58); // pop guest RAX
    }

    /// Fuse one exact EVEX scalar-lane or vector-chunk extraction whose
    /// destination is guest memory.
    ///
    /// Scalar E9NF forms replay to preserved RAX and perform one precise
    /// 1-/2-/4-/8-byte helper write. Chunk E6NF forms replay against private
    /// stack storage. A masked chunk first performs the mandatory complete
    /// guest read and seeds that storage; every chunk then performs the
    /// mandatory complete 16- or 32-byte guest write. Helper failure exits at
    /// the instruction PC without committing registers, flags, opmasks, or
    /// MXCSR.
    pub(crate) fn try_lower_jit_evex_extract_memory_destination(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_extract_memory_sequence(
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
                op: "EVEX extraction memory destination".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 extraction state".to_string(),
            });
        }

        match sequence {
            X86JitEvexExtractMemorySequence::Scalar(sequence) => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Store { addr, .. } => addr,
                    _ => unreachable!("validated EVEX scalar extraction ends with Store"),
                };
                let encoding = sequence.encoding;
                self.code.emit_u8(0x50); // push guest RAX
                self.code
                    .emit_bytes(encoding.register_instruction.as_slice());
                self.code.emit_u8(0x51); // push guest RCX
                self.code.emit_bytes(&[0x48, 0x8B, 0x4D]);
                self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rcx,[rbp+state]
                self.emit_jit_vector_scratch_gpr_store(encoding.memory_width);
                self.code.emit_u8(0x59); // pop guest RCX
                self.code.emit_u8(0x58); // pop guest RAX
                self.emit_jit_vector_mem_helper(
                    block.ops[index].guest_pc,
                    false,
                    X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                    address,
                    encoding.memory_width.bytes(),
                    false,
                    true,
                )?;
            }
            X86JitEvexExtractMemorySequence::Chunk(sequence) => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } | OpKind::VStore { addr, .. } => addr,
                    _ => unreachable!("validated EVEX chunk extraction owns vector memory"),
                };
                let encoding = sequence.encoding;
                let size = encoding.chunk_width.bytes();

                if encoding.writemask.is_some() {
                    // E6NF requires this full read even when every live mask
                    // bit is clear. It occurs before private-frame allocation
                    // so a helper fault needs no additional stack cleanup.
                    self.emit_jit_vector_mem_helper(
                        block.ops[index].guest_pc,
                        true,
                        X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                        address,
                        size,
                        true,
                        true,
                    )?;
                }

                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(
                        PhysReg::Rsp,
                        PhysReg::Rsp,
                        -EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
                    );
                }
                if encoding.writemask.is_some() {
                    match encoding.chunk_width {
                        VecWidth::V128 => self.emit_jit_vector_scratch_stack_store_128(),
                        VecWidth::V256 => self.emit_jit_vector_scratch_stack_store_256(),
                        _ => unreachable!("validated EVEX extraction chunk width"),
                    }
                }
                self.code.emit_bytes(encoding.stack_instruction.as_slice());
                self.emit_evex_extract_stack_to_vector_scratch(encoding.chunk_width);
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, EVEX_E4_MASKED_VECTOR_FRAME_SIZE);
                }
                self.emit_jit_vector_mem_helper(
                    block.ops[index].guest_pc,
                    false,
                    X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                    address,
                    size,
                    false,
                    true,
                )?;
            }
        }
        Ok(Some(sequence.consumed()))
    }
}
