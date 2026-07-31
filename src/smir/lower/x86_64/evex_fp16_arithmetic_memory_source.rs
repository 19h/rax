//! Helper-backed EVEX packed binary16 arithmetic memory-source lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexPackedFp16ArithmeticMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{Address, MemWidth, OpWidth, SignExtend, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

// Full-vector writemasking stages 64 payload bytes plus one 8-byte scalar
// helper result. An 80-byte frame preserves the trampoline's 16-byte call
// alignment while keeping the two regions disjoint.
const MASKED_VECTOR_FRAME_SIZE: i32 = 80;
const MASKED_VECTOR_STAGING_OFFSET: i32 = 64;

impl X86_64Lowerer {
    fn evex_fp16_arithmetic_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated packed binary16 arithmetic width"),
        }
    }

    fn emit_evex_masked_fp16_arithmetic_lane_helper(
        &mut self,
        guest_pc: u64,
        address: &Address,
        mask: u8,
        lane: usize,
    ) -> Result<(), LowerError> {
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_opmask_mask_to_rax64(mask);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_ri(PhysReg::Rax, 1i64 << lane, OpWidth::W32);
        }
        let inactive = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags

        let lane_offset = i32::try_from(lane).expect("at most 32 FP16 lanes") * 2;
        self.emit_jit_mem_op_linear_offset(
            guest_pc,
            true,
            None,
            Some(16 + MASKED_VECTOR_STAGING_OFFSET),
            None,
            None,
            None,
            address,
            MemWidth::B2,
            SignExtend::Zero,
            MASKED_VECTOR_FRAME_SIZE,
            lane_offset,
        )?;

        // The scalar helper ABI stages a complete 8-byte return. Copy only
        // the architectural 2-byte lane into the vector payload so one active
        // lane cannot overwrite its neighbors. MOV preserves guest flags.
        self.code.emit_u8(0x50); // push guest RAX
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rax,
                PhysReg::Rsp,
                8 + MASKED_VECTOR_STAGING_OFFSET,
                OpWidth::W16,
            );
            emitter.emit_mov_mr(PhysReg::Rsp, 8 + lane_offset, PhysReg::Rax, OpWidth::W16);
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

    fn emit_evex_fp16_arithmetic_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexPackedFp16ArithmeticMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let address_index = index + sequence.address_offset;
        let address = match &block.ops[address_index].kind {
            OpKind::Load {
                addr,
                width: MemWidth::B2,
                sign: SignExtend::Zero,
                ..
            }
            | OpKind::PredLoad {
                addr,
                width: MemWidth::B2,
                signed: SignExtend::Zero,
                ..
            } => addr,
            _ => unreachable!("validated FP16 broadcast sequence owns its scalar memory op"),
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }

        let inactive = if let Some(mask) = sequence.encoding.writemask {
            let lanes = sequence
                .encoding
                .width
                .lanes(crate::smir::ir::types::VecElementType::F16);
            let lane_mask = (1u64 << lanes) - 1;
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax64(mask);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                // W32 covers all 32 ZMM binary16 lanes without observing
                // architecturally ignored high opmask bits.
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
            MemWidth::B2,
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

    /// Fuse one exact packed AVX-512-FP16 arithmetic memory decomposition.
    ///
    /// Unmasked vectors use the reserved nonarchitectural vector transfer
    /// slot and a byte-validated register rewrite. Broadcasts use at most one
    /// scalar helper access and a byte-validated `[rsp]{1toN}` rewrite.
    /// Writemasked vectors issue ascending 2-byte helper loads only for active
    /// lanes, accumulate them outside architectural state, and commit the
    /// destination once every active load succeeds. Any helper fault exits at
    /// the source guest PC before arithmetic or destination modification.
    pub(crate) fn try_lower_jit_evex_packed_fp16_arithmetic_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_packed_fp16_arithmetic_memory_sequence(
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
                op: "EVEX packed FP16 arithmetic memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512-FP16".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexPackedFp16ArithmeticMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated FP16 vector sequence starts with VLoad"),
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
                    Self::evex_fp16_arithmetic_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexPackedFp16ArithmeticMemoryReplay::Broadcast { stack_instruction } => {
                self.emit_evex_fp16_arithmetic_broadcast_replay(
                    block,
                    index,
                    sequence,
                    stack_instruction,
                )?;
            }
            X86EvexPackedFp16ArithmeticMemoryReplay::MaskedVector { stack_instruction } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Lea { addr, .. } => addr,
                    _ => unreachable!("validated masked FP16 vector sequence owns its LEA"),
                };
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -MASKED_VECTOR_FRAME_SIZE);
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
                let lanes = sequence
                    .encoding
                    .width
                    .lanes(crate::smir::ir::types::VecElementType::F16)
                    as usize;
                let mask = sequence
                    .encoding
                    .writemask
                    .expect("validated masked-vector replay");
                for lane in 0..lanes {
                    self.emit_evex_masked_fp16_arithmetic_lane_helper(
                        block.ops[index].guest_pc,
                        address,
                        mask,
                        lane,
                    )?;
                }
                self.code.emit_bytes(stack_instruction.as_slice());
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, MASKED_VECTOR_FRAME_SIZE);
                }
            }
        }
        Ok(Some(sequence.consumed))
    }
}
