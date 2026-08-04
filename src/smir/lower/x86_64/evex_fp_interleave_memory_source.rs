//! Helper-backed EVEX VUNPCKL/HPS/PD memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexFpInterleaveMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{SignExtend, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_fp_interleave_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX floating interleave width"),
        }
    }

    fn emit_evex_fp_interleave_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexFpInterleaveMemorySequence,
        memory_width: crate::smir::ir::types::MemWidth,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } if *width == memory_width => addr,
            _ => unreachable!("validated EVEX floating interleave owns its scalar load"),
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

    /// Fuse one exact EVEX VUNPCKLPS/LPD/HPS/HPD memory decomposition.
    ///
    /// Type E4NF requires one complete 16/32/64-byte vector or 4/8-byte
    /// broadcast helper access before any destination update, including when
    /// every writemask lane is inactive. Full tuples replay from an otherwise
    /// unused low vector register; scalar tuples replay from `[rsp]{1toN}`.
    pub(crate) fn try_lower_jit_evex_fp_interleave_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_fp_interleave_memory_sequence(
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
                op: "EVEX floating-point interleave memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 state".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexFpInterleaveMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!(
                        "validated EVEX floating interleave sequence starts with VLoad"
                    ),
                };
                self.emit_jit_vector_mem_helper(
                    block.ops[index].guest_pc,
                    true,
                    X86_JIT_VECTOR_SCRATCH_INDEX as u8,
                    address,
                    sequence.encoding.memory_size,
                    true,
                    true,
                )?;
                let scratch_reg =
                    Self::evex_fp_interleave_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexFpInterleaveMemoryReplay::Broadcast {
                memory_width,
                stack_instruction,
            } => self.emit_evex_fp_interleave_broadcast_replay(
                block,
                index,
                sequence,
                memory_width,
                stack_instruction,
            )?,
        }

        Ok(Some(sequence.consumed))
    }
}
