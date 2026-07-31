//! Helper-backed EVEX saturating integer-pack memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexIntegerArithmeticMemoryReplay;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{MemWidth, SignExtend, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn evex_integer_pack_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            VecWidth::V512 => PhysReg::Zmm(index),
            _ => unreachable!("validated EVEX saturating-pack width"),
        }
    }

    fn emit_evex_integer_pack_broadcast_replay(
        &mut self,
        block: &SmirBlock,
        index: usize,
        sequence: crate::smir::lower::runtime::X86JitEvexIntegerPackMemorySequence,
        stack_instruction: crate::smir::ir::X86InstructionBytes,
    ) -> Result<(), LowerError> {
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Load {
                addr,
                width: MemWidth::B4,
                sign: SignExtend::Zero,
                ..
            } => addr,
            _ => unreachable!("validated saturating pack owns its scalar memory op"),
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
            MemWidth::B4,
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

    /// Fuse one exact EVEX signed/unsigned saturating-pack memory
    /// decomposition.
    ///
    /// Full-vector sources use the reserved vector transfer slot and always
    /// issue one complete 16/32/64-byte helper access. Broadcast forms issue
    /// one unconditional 4-byte scalar access. The original writemask is
    /// applied only by the rewritten native register/stack operation after
    /// the E4NF/E4NF.nb memory frontier succeeds.
    pub(crate) fn try_lower_jit_evex_integer_pack_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_integer_pack_memory_sequence(
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
                op: "EVEX saturating integer-pack memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 packs".to_string(),
            });
        }

        match sequence.encoding.replay {
            X86EvexIntegerArithmeticMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                let address = match &block.ops[index + sequence.address_offset].kind {
                    OpKind::VLoad { addr, .. } => addr,
                    _ => unreachable!("validated saturating-pack sequence starts with VLoad"),
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
                    Self::evex_integer_pack_phys_reg(scratch, sequence.encoding.width);
                self.code.emit_u8(0x50); // push guest RAX
                self.emit_load_state_ptr_rax();
                self.emit_jit_vector_scratch_load(scratch_reg, sequence.encoding.width);
                self.code.emit_bytes(register_instruction.as_slice());
                self.emit_jit_vector_scratch_restore(scratch);
                self.code.emit_u8(0x58); // pop guest RAX
            }
            X86EvexIntegerArithmeticMemoryReplay::Broadcast { stack_instruction } => {
                self.emit_evex_integer_pack_broadcast_replay(
                    block,
                    index,
                    sequence,
                    stack_instruction,
                )?;
            }
            X86EvexIntegerArithmeticMemoryReplay::MaskedVector { .. } => {
                unreachable!("E4NF saturating packs never use per-lane masked replay")
            }
        }
        Ok(Some(sequence.consumed))
    }
}
