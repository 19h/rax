//! Helper-backed EVEX opmask-selector blend memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexMaskBlendMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, MemWidth, OpWidth, SignExtend, VReg, VecElementType, VecWidth,
};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

// Full-vector selector masking stages 64 payload bytes plus one 8-byte scalar
// helper result. An 80-byte frame preserves the trampoline's 16-byte call
// alignment while keeping the two regions disjoint.
const MASKED_VECTOR_FRAME_SIZE: i32 = 80;
const MASKED_VECTOR_STAGING_OFFSET: i32 = 64;

impl X86_64Lowerer {
    fn evex_mask_blend_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX mask-blend width"),
        }
    }

    fn evex_mask_blend_element_widths(
        elem: VecElementType,
    ) -> Result<(MemWidth, OpWidth, i32), LowerError> {
        match elem {
            VecElementType::I8 => Ok((MemWidth::B1, OpWidth::W8, 1)),
            VecElementType::I16 => Ok((MemWidth::B2, OpWidth::W16, 2)),
            VecElementType::I32 => Ok((MemWidth::B4, OpWidth::W32, 4)),
            VecElementType::I64 => Ok((MemWidth::B8, OpWidth::W64, 8)),
            _ => Err(LowerError::InvalidOperand {
                op: "EVEX mask-blend memory source".to_string(),
                operand: format!("unsupported element type {elem:?}"),
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_evex_mask_blend_lane_helper(
        &mut self,
        guest_pc: u64,
        address: &Address,
        selector: u8,
        lane: usize,
        memory_width: MemWidth,
        copy_width: OpWidth,
        lane_bytes: i32,
    ) -> Result<(), LowerError> {
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_opmask_mask_to_rax64(selector);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            if lane != 0 {
                emitter.emit_shr_ri(
                    PhysReg::Rax,
                    u8::try_from(lane).expect("at most 64 selector lanes"),
                    OpWidth::W64,
                );
            }
            emitter.emit_test_ri(PhysReg::Rax, 1, OpWidth::W64);
        }
        let inactive = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags

        let lane_offset = i32::try_from(lane).expect("at most 64 selector lanes") * lane_bytes;
        self.emit_jit_mem_op_linear_offset(
            guest_pc,
            true,
            None,
            Some(16 + MASKED_VECTOR_STAGING_OFFSET),
            None,
            None,
            None,
            address,
            memory_width,
            SignExtend::Zero,
            MASKED_VECTOR_FRAME_SIZE,
            lane_offset,
        )?;

        // The scalar helper ABI stages a complete 8-byte return. Copy exactly
        // one 1/2/4/8-byte architectural lane into the vector payload.
        self.code.emit_u8(0x50); // push guest RAX
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(
                PhysReg::Rax,
                PhysReg::Rsp,
                8 + MASKED_VECTOR_STAGING_OFFSET,
                copy_width,
            );
            emitter.emit_mov_mr(PhysReg::Rsp, 8 + lane_offset, PhysReg::Rax, copy_width);
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

    fn emit_evex_mask_blend_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexMaskBlendMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let (memory_width, _, _) = Self::evex_mask_blend_element_widths(sequence.encoding.elem)?;
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
            } if *width == memory_width => addr,
            _ => unreachable!("validated mask-blend broadcast owns its scalar memory op"),
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }

        let inactive = if let Some(selector) = sequence.encoding.selector {
            let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as u8;
            let lane_mask = if lanes == 64 {
                u64::MAX
            } else {
                (1u64 << lanes) - 1
            };
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax64(selector);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_ri(PhysReg::Rax, lane_mask as i64, OpWidth::W64);
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
            memory_width,
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

    /// Fuse one exact EVEX V[P]BLENDM* memory-source decomposition.
    ///
    /// Unselected Type E4 lanes perform no guest-memory helper access.
    /// Full unmasked vectors use the reserved vector transfer slot; masked
    /// vectors stage active scalar lanes in ascending order; broadcasts issue
    /// at most one scalar read. All paths defer the architectural destination
    /// commit until every required helper access succeeds.
    pub(crate) fn try_lower_jit_evex_mask_blend_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_mask_blend_memory_sequence(
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
                op: "EVEX mask-blend memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX mask blends".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexMaskBlendMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated mask-blend vector sequence starts with VLoad"),
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
                let scratch_reg = Self::evex_mask_blend_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexMaskBlendMemoryReplay::Broadcast { stack_instruction } => {
                self.emit_evex_mask_blend_broadcast_replay(
                    block,
                    index,
                    sequence,
                    stack_instruction,
                )?;
            }
            X86EvexMaskBlendMemoryReplay::MaskedVector { stack_instruction } => {
                let (memory_width, copy_width, lane_bytes) =
                    Self::evex_mask_blend_element_widths(sequence.encoding.elem)?;
                let selector = sequence
                    .encoding
                    .selector
                    .expect("masked-vector replay has a selector");
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Lea { addr, .. } => addr,
                    _ => unreachable!("validated masked mask-blend sequence owns its address"),
                };
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -MASKED_VECTOR_FRAME_SIZE);
                }
                let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as usize;
                for lane in 0..lanes {
                    self.emit_evex_mask_blend_lane_helper(
                        block.ops[index].guest_pc,
                        address,
                        selector,
                        lane,
                        memory_width,
                        copy_width,
                        lane_bytes,
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
