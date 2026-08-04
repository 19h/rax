//! Helper-backed EVEX `VGF2P8MULB` memory-source lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexGfniMultiplyMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{DispSize, OpWidth, VReg, VecElementType, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_gfni_multiply_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX VGF2P8MULB width"),
        }
    }

    /// Fuse one exact EVEX `VGF2P8MULB` Full Mem decomposition.
    ///
    /// An unmasked tuple is loaded completely through the reserved vector
    /// transfer slot. A writemasked tuple issues ascending 1-byte helper loads
    /// only for active lanes and stages them in a private stack vector. Every
    /// required helper access completes before the byte-validated native
    /// instruction executes, so a source fault exits at the guest instruction
    /// frontier without changing the destination.
    pub(crate) fn try_lower_jit_evex_gfni_multiply_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_gfni_multiply_memory_sequence(
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
                op: "EVEX VGF2P8MULB memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 GFNI".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexGfniMultiplyMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated VGF2P8MULB vector sequence starts with VLoad"),
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
                let scratch_reg =
                    Self::evex_gfni_multiply_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexGfniMultiplyMemoryReplay::MaskedVector { stack_instruction } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Lea { addr, .. } => addr,
                    _ => unreachable!("validated masked VGF2P8MULB sequence owns its LEA"),
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
                            DispSize::Auto,
                            0,
                            OpWidth::W64,
                        );
                    }
                }
                let mask = sequence
                    .encoding
                    .writemask
                    .expect("masked VGF2P8MULB replay has an opmask");
                let lanes = sequence.encoding.width.bytes() as usize;
                for lane in 0..lanes {
                    self.emit_evex_masked_e4_memory_lane_helper(
                        block.ops[index].guest_pc,
                        address,
                        mask,
                        lane,
                        crate::smir::ir::types::MemWidth::B1,
                        OpWidth::W8,
                        VecElementType::I8.bytes() as i32,
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
