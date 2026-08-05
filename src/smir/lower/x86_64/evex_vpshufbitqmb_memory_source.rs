//! Helper-backed EVEX `VPSHUFBITQMB` memory-source lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexVpshufbitqmbMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecElementType};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact Type-E4 `VPSHUFBITQMB` memory decomposition.
    ///
    /// Unmasked vectors use the reserved nonarchitectural vector-transfer slot
    /// and a byte-validated register rewrite. Writemasked vectors issue
    /// ascending one-byte helper loads only for active output bits and execute
    /// the native operation only after every load succeeds. A helper fault
    /// therefore leaves the K destination unchanged, including when the
    /// destination aliases its writemask.
    pub(crate) fn try_lower_jit_evex_vpshufbitqmb_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_vpshufbitqmb_memory_sequence(
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
                op: "EVEX VPSHUFBITQMB memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 BITALG state".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexVpshufbitqmbMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated VPSHUFBITQMB vector tuple starts with VLoad"),
                };
                self.emit_jit_vector_mem_helper(
                    block.ops[index].guest_pc,
                    true,
                    X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                    address,
                    sequence.memory_size,
                    true,
                    true,
                )?;
                let scratch_reg = Self::evex_e4_memory_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexVpshufbitqmbMemoryReplay::MaskedVector { stack_instruction } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Lea { addr, .. } => addr,
                    _ => unreachable!("validated masked VPSHUFBITQMB owns its LEA"),
                };
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
                let lanes = sequence.encoding.width.lanes(VecElementType::I8) as usize;
                let mask = sequence
                    .encoding
                    .writemask
                    .expect("validated masked VPSHUFBITQMB replay");
                let (memory_width, copy_width, lane_bytes) =
                    Self::evex_e4_memory_element_widths(VecElementType::I8)?;
                for lane in 0..lanes {
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
