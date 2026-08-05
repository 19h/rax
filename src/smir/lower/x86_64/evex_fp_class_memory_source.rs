//! Helper-backed EVEX `VFPCLASS*` memory lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexFpClassMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, SignExtend, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn emit_evex_fp_class_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexFpClassMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let address_index = index + sequence.address_offset;
        let address = match &block.ops[address_index].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            }
            | OpKind::PredLoad {
                addr,
                width,
                signed: SignExtend::Zero,
                ..
            } if *width == sequence.encoding.memory_width => addr,
            // `append_evex_masked_vector_source` reconstructs masked
            // broadcasts lane by lane. Its common exact matcher points at the
            // original helper address carried by this LEA.
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated VFPCLASS broadcast owns its helper address"),
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }

        let inactive = if let Some(mask) = sequence.encoding.writemask {
            let lanes = sequence.encoding.width.lanes(sequence.encoding.elem);
            debug_assert!(lanes < 64, "VFPCLASS broadcasts use at most 32 mask bits");
            let lane_mask = (1u64 << lanes) - 1;
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax64(mask);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_ri(PhysReg::Rax, lane_mask as i64, OpWidth::W32);
            }
            Some(self.emit_jcc_placeholder(X86Cond::E))
        } else {
            None
        };

        if inactive.is_some() {
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
        }
        self.emit_jit_mem_op(
            block.ops[index].guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            address,
            sequence.encoding.memory_width,
            SignExtend::Zero,
            16,
        )?;
        if let Some(inactive) = inactive {
            self.code.emit_u8(0xE9);
            let execute = self.code.position();
            self.code.emit_u32(0);
            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
            self.patch_rel32_to_current(execute)?;
        }
        self.code.emit_bytes(stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(())
    }

    /// Fuse one exact packed or scalar `VFPCLASS*` memory decomposition.
    ///
    /// Unmasked vectors use the reserved vector-transfer slot. Broadcasts
    /// issue one scalar helper access only when any applicable K bit is set.
    /// Writemasked full vectors issue ascending per-active-lane helper loads
    /// into a zeroed stack vector. Scalar forms use the shared K[0]-guarded
    /// stack replay. Native classification executes only after every required
    /// helper succeeds, so a fault cannot commit the K destination.
    pub(crate) fn try_lower_jit_evex_fp_class_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_fp_class_memory_sequence(
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
                op: "EVEX VFPCLASS memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 FPCLASS".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexFpClassMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated VFPCLASS vector sequence starts with VLoad"),
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
            X86EvexFpClassMemoryReplay::Broadcast { stack_instruction } => {
                self.emit_evex_fp_class_broadcast_replay(
                    block,
                    index,
                    sequence,
                    stack_instruction,
                )?;
            }
            X86EvexFpClassMemoryReplay::MaskedVector { stack_instruction } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Lea { addr, .. } => addr,
                    _ => unreachable!("validated masked VFPCLASS owns its LEA"),
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
                            OpWidth::W64,
                        );
                    }
                }
                let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as usize;
                let mask = sequence
                    .encoding
                    .writemask
                    .expect("validated masked-vector VFPCLASS replay");
                let (memory_width, copy_width, lane_bytes) =
                    Self::evex_e4_memory_element_widths(sequence.encoding.elem)?;
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
            X86EvexFpClassMemoryReplay::Scalar { stack_instruction } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Load {
                        addr,
                        width,
                        sign: SignExtend::Zero,
                        ..
                    }
                    | OpKind::PredLoad {
                        addr,
                        width,
                        signed: SignExtend::Zero,
                        ..
                    } if *width == sequence.encoding.memory_width => addr,
                    _ => unreachable!("validated scalar VFPCLASS owns its scalar memory op"),
                };
                self.emit_evex_scalar_memory_stack_replay(
                    block.ops[index].guest_pc,
                    address,
                    sequence.encoding.memory_width,
                    sequence.encoding.writemask,
                    stack_instruction,
                )?;
            }
        }
        Ok(Some(sequence.consumed))
    }
}
