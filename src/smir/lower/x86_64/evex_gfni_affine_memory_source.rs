//! Helper-backed EVEX affine GFNI memory-source lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexGfniAffineMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{MemWidth, SignExtend, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_gfni_affine_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX affine GFNI width"),
        }
    }

    fn emit_evex_gfni_affine_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexGfniAffineMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load {
                addr,
                width: MemWidth::B8,
                sign: SignExtend::Zero,
                ..
            } => addr,
            _ => unreachable!("validated affine GFNI broadcast owns one 8-byte load"),
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        // Type E4NF explicitly does not suppress the memory access. Do not
        // predicate this helper on the architectural writemask.
        self.emit_jit_mem_op(
            block.ops[index].guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            address,
            MemWidth::B8,
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

    /// Fuse one exact EVEX VGF2P8AFFINE[INV]QB memory decomposition.
    ///
    /// Full vectors use the reserved nonarchitectural vector-transfer slot and
    /// a byte-validated register rewrite. Broadcasts use one 8-byte scalar
    /// helper access and a byte-validated `[rsp]{1toN}` rewrite. Both accesses
    /// are unconditional under Type E4NF, so any helper fault exits at the
    /// source guest PC before GFNI execution or destination modification.
    pub(crate) fn try_lower_jit_evex_gfni_affine_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_gfni_affine_memory_sequence(
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
                op: "EVEX affine GFNI memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 GFNI".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexGfniAffineMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated affine GFNI vector sequence starts with VLoad"),
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
                let scratch_reg = Self::evex_gfni_affine_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexGfniAffineMemoryReplay::Broadcast { stack_instruction } => {
                self.emit_evex_gfni_affine_broadcast_replay(
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
