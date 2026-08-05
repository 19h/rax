//! Helper-backed AVX-512 4FMAPS whole-tuple memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact `V4FMADDPS`/`V4FNMADDPS`/`V4FMADDSS`/`V4FNMADDSS`
    /// Tuple1_4X decomposition.
    ///
    /// One 16-byte vector helper stages the complete tuple before the native
    /// operation can update its destination or MXCSR. A live-host-K guard
    /// bypasses the helper when every applicable mask bit is zero. The helper
    /// runs before the 16-byte stack reservation, so its fault exit observes
    /// the standard native frame and commits no architectural state.
    pub(crate) fn try_lower_jit_evex_four_fma_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_four_fma_memory_sequence(
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
                op: "AVX-512 4FMAPS memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry 4FMAPS state".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::VLoad {
                addr,
                width: VecWidth::V128,
                ..
            }
            | OpKind::PredVLoad {
                addr,
                width: VecWidth::V128,
                ..
            } => addr,
            _ => unreachable!("validated 4FMAPS sequence owns its whole-tuple load"),
        };

        let inactive = if let Some(mask) = sequence.encoding.writemask {
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax16(mask);
            let applicable = if sequence.encoding.scalar { 1 } else { 0xFFFF };
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_ri(PhysReg::Rax, applicable, OpWidth::W64);
            }
            Some(self.emit_jcc_placeholder(X86Cond::E))
        } else {
            None
        };

        if inactive.is_some() {
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
        }
        self.emit_jit_vector_mem_helper(
            block.ops[index].guest_pc,
            true,
            X86_JIT_VECTOR_SCRATCH_INDEX as u8,
            address,
            16,
            true,
            true,
        )?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        self.emit_jit_vector_scratch_stack_store_128();

        if let Some(inactive) = inactive {
            self.code.emit_u8(0xE9);
            let execute = self.code.position();
            self.code.emit_u32(0);
            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
            }
            self.patch_rel32_to_current(execute)?;
        }

        self.code
            .emit_bytes(sequence.encoding.stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(Some(sequence.consumed))
    }
}
