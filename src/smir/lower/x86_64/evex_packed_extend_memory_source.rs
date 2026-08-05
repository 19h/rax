//! Helper-backed EVEX packed sign/zero-extension memory lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexPackedExtendMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::VReg;
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact EVEX `VPMOVSX*`/`VPMOVZX*` memory source.
    ///
    /// Unmasked tuples perform one exact 2-/4-/8-/16-/32-byte helper read into
    /// the reserved vector transfer slot and execute a byte-validated register
    /// rewrite. Writemasked tuples issue ascending helper reads only for active
    /// destination lanes and execute after the complete source is staged, so
    /// a helper fault cannot commit any destination state.
    pub(crate) fn try_lower_jit_evex_packed_extend_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_packed_extend_memory_sequence(
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
                op: "EVEX packed-extension memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 state".to_string(),
            });
        }

        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated EVEX packed-extension owns its LEA"),
        };
        match sequence.encoding.replay {
            X86EvexPackedExtendMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                self.emit_jit_vector_mem_helper(
                    block.ops[index].guest_pc,
                    true,
                    X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                    address,
                    sequence.memory_size,
                    true,
                    true,
                )?;
                let transfer_width = sequence.encoding.transfer_width();
                let scratch_reg = Self::evex_e4_memory_phys_reg(scratch, transfer_width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, transfer_width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexPackedExtendMemoryReplay::MaskedVector { stack_instruction } => {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(
                        PhysReg::Rsp,
                        PhysReg::Rsp,
                        -EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
                    );
                    for offset in (0..64).step_by(8) {
                        emitter.emit_mov_mi_disp(
                            PhysReg::Rsp,
                            offset,
                            crate::smir::ir::types::DispSize::Auto,
                            0,
                            crate::smir::ir::types::OpWidth::W64,
                        );
                    }
                }
                let mask = sequence
                    .encoding
                    .writemask
                    .expect("validated masked packed-extension replay");
                let (memory_width, copy_width, lane_bytes) =
                    Self::evex_e4_memory_element_widths(sequence.encoding.source_elem)?;
                for lane in 0..usize::from(sequence.encoding.lanes) {
                    self.emit_evex_masked_e4_memory_lane_helper(
                        block.ops[index].guest_pc,
                        address,
                        mask,
                        lane,
                        memory_width,
                        copy_width,
                        lane_bytes,
                    )?;
                }
                self.code.emit_bytes(stack_instruction.as_slice());
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, EVEX_E4_MASKED_VECTOR_FRAME_SIZE);
                }
            }
        }
        Ok(Some(sequence.consumed))
    }
}
