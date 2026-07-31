//! Helper-backed EVEX VALIGND/Q memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexVectorAlignMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{MemWidth, SignExtend, VReg, VecElementType, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_vector_align_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated VALIGN vector width"),
        }
    }

    fn evex_vector_align_memory_width(elem: VecElementType) -> Result<MemWidth, LowerError> {
        match elem {
            VecElementType::I32 => Ok(MemWidth::B4),
            VecElementType::I64 => Ok(MemWidth::B8),
            _ => Err(LowerError::InvalidOperand {
                op: "EVEX VALIGN memory source".to_string(),
                operand: format!("unsupported element type {elem:?}"),
            }),
        }
    }

    fn emit_evex_vector_align_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexVectorAlignMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let memory_width = Self::evex_vector_align_memory_width(sequence.encoding.elem)?;
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } if *width == memory_width => addr,
            _ => unreachable!("validated VALIGN broadcast owns its scalar memory op"),
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        // VALIGN memory is E4NF: even an all-zero writemask performs this
        // helper read. The destination remains untouched until replay.
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

    /// Fuse one exact VALIGND/Q memory-source decomposition.
    ///
    /// Full vectors use the reserved nonarchitectural vector transfer slot and
    /// a byte-validated register rewrite. Broadcasts issue one unconditional
    /// scalar helper read and replay an exact `[rsp]{1toN}` form. In both
    /// cases, helper failure exits before the architectural destination commit.
    pub(crate) fn try_lower_jit_evex_vector_align_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_vector_align_memory_sequence(
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
                op: "EVEX VALIGN memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 VALIGN".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexVectorAlignMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated VALIGN vector sequence starts with VLoad"),
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
                    Self::evex_vector_align_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexVectorAlignMemoryReplay::Broadcast { stack_instruction } => {
                self.emit_evex_vector_align_broadcast_replay(
                    block,
                    index,
                    sequence,
                    stack_instruction,
                )?;
            }
        }
        Ok(Some(sequence.consumed))
    }
}
