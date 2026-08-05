//! Helper-backed EVEX unary packed-integer memory lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::{
    EVEX_E4_MASKED_VECTOR_FRAME_SIZE, EVEX_E4_MASKED_VECTOR_STAGING_OFFSET,
};
use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{Address, OpWidth, SignExtend, VReg};
use crate::smir::ir::{SmirBlock, X86EvexIntegerUnaryMemoryKind, X86EvexIntegerUnaryMemoryReplay};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn emit_evex_integer_unary_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexIntegerUnaryMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let (memory_width, _, _) = Self::evex_e4_memory_element_widths(sequence.encoding.elem)?;
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } if *width == memory_width => addr,
            _ => unreachable!("validated integer unary broadcast owns its scalar load"),
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
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
            memory_width,
            SignExtend::Zero,
            16,
        )?;
        self.code.emit_bytes(stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_evex_integer_unary_masked_lane_helper(
        &mut self,
        guest_pc: u64,
        address: &Address,
        mask: u8,
        lane: usize,
        lanes: usize,
        source_offset: i32,
        memory_width: crate::smir::ir::types::MemWidth,
        copy_width: OpWidth,
        lane_bytes: i32,
        conflict: bool,
    ) -> Result<(), LowerError> {
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push guest RAX
        if lanes <= 16 {
            self.emit_opmask_mask_to_rax16(mask);
        } else {
            self.emit_opmask_mask_to_rax64(mask);
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            if conflict {
                let valid_mask = (1u64 << lanes) - 1;
                emitter.emit_and_ri(PhysReg::Rax, valid_mask as i64, OpWidth::W64);
                if lane != 0 {
                    emitter.emit_shr_ri(
                        PhysReg::Rax,
                        u8::try_from(lane).expect("at most 16 conflict lanes"),
                        OpWidth::W64,
                    );
                }
                emitter.emit_test_rr(PhysReg::Rax, PhysReg::Rax, OpWidth::W64);
            } else if lane < 32 {
                emitter.emit_test_ri(PhysReg::Rax, 1i64 << lane, OpWidth::W32);
            } else {
                emitter.emit_shr_ri(
                    PhysReg::Rax,
                    u8::try_from(lane).expect("at most 64 integer lanes"),
                    OpWidth::W64,
                );
                emitter.emit_test_ri(PhysReg::Rax, 1, OpWidth::W64);
            }
        }
        let inactive = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags

        self.emit_jit_mem_op_linear_offset(
            guest_pc,
            true,
            None,
            Some(16 + EVEX_E4_MASKED_VECTOR_STAGING_OFFSET),
            None,
            None,
            None,
            address,
            memory_width,
            SignExtend::Zero,
            EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
            source_offset,
        )?;

        self.code.emit_u8(0x50); // push guest RAX
        let destination_offset =
            i32::try_from(lane).expect("at most 64 integer lanes") * lane_bytes;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rax,
                PhysReg::Rsp,
                8 + EVEX_E4_MASKED_VECTOR_STAGING_OFFSET,
                copy_width,
            );
            emitter.emit_mov_mr(
                PhysReg::Rsp,
                8 + destination_offset,
                PhysReg::Rax,
                copy_width,
            );
        }
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(inactive)?;
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags
        self.patch_rel32_to_current(done)?;
        Ok(())
    }

    /// Fuse one exact `VPCONFLICTD/Q`, `VPLZCNTD/Q`, or `VPOPCNTB/W/D/Q`
    /// memory decomposition.
    ///
    /// Unmasked vectors use the reserved vector-transfer slot. Unmasked
    /// broadcasts issue one scalar access. Writemasked forms preserve every
    /// canonical SMIR helper access in ascending source-lane order, including
    /// conflict prefix dependencies and repeated broadcast reads, and commit
    /// the architectural destination only after all required accesses succeed.
    pub(crate) fn try_lower_jit_evex_integer_unary_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_integer_unary_memory_sequence(
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
                op: "EVEX unary packed-integer memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 integer state".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexIntegerUnaryMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated integer unary vector sequence starts with VLoad"),
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
            X86EvexIntegerUnaryMemoryReplay::Broadcast { stack_instruction } => {
                self.emit_evex_integer_unary_broadcast_replay(
                    block,
                    index,
                    sequence,
                    stack_instruction,
                )?;
            }
            X86EvexIntegerUnaryMemoryReplay::MaskedVector { stack_instruction } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Lea { addr, .. } => addr,
                    _ => unreachable!("validated masked integer unary sequence owns its LEA"),
                };
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(
                        PhysReg::Rsp,
                        PhysReg::Rsp,
                        -EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
                    );
                }
                let mask = sequence
                    .encoding
                    .writemask
                    .expect("masked integer unary replay has an opmask");
                let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as usize;
                let (memory_width, copy_width, lane_bytes) =
                    Self::evex_e4_memory_element_widths(sequence.encoding.elem)?;
                let conflict = sequence.encoding.kind == X86EvexIntegerUnaryMemoryKind::Conflict;
                for lane in 0..lanes {
                    let source_offset = if sequence.encoding.broadcast {
                        0
                    } else {
                        i32::try_from(lane).expect("at most 64 integer lanes") * lane_bytes
                    };
                    self.emit_evex_integer_unary_masked_lane_helper(
                        block.ops[index].guest_pc,
                        address,
                        mask,
                        lane,
                        lanes,
                        source_offset,
                        memory_width,
                        copy_width,
                        lane_bytes,
                        conflict,
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
