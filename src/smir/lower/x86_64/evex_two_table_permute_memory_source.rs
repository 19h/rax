//! Helper-backed EVEX VPERMI2*/VPERMT2* memory-source lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexTwoTablePermuteMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{SignExtend, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_two_table_permute_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX two-table-permute width"),
        }
    }

    /// Fuse one exact EVEX VPERMI2*/VPERMT2* memory decomposition.
    ///
    /// Full-vector tuples use the reserved nonarchitectural vector-transfer
    /// slot and a byte-validated register rewrite. Broadcast tuples issue one
    /// unconditional scalar helper read into a 16-byte stack slot and replay
    /// an exact `[rsp]{1toN}` rewrite. Both Type E4NF/E4NF.nb paths exit on
    /// helper failure before changing the architectural destination.
    pub(crate) fn try_lower_jit_evex_two_table_permute_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_two_table_permute_memory_sequence(
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
                op: "EVEX two-table permute memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 permutes".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexTwoTablePermuteMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated full-vector tuple starts with VLoad"),
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
                    Self::evex_two_table_permute_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexTwoTablePermuteMemoryReplay::Broadcast {
                memory_width,
                stack_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::Load {
                        addr,
                        width,
                        sign: SignExtend::Zero,
                        ..
                    } if *width == memory_width => addr,
                    _ => unreachable!("validated broadcast tuple starts with scalar Load"),
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
            }
        }
        Ok(Some(sequence.consumed))
    }
}
