//! Helper-backed EVEX memory-broadcast lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, VReg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact EVEX `VBROADCAST*`/`VPBROADCAST*` memory decomposition.
    ///
    /// A precise vector helper reads the complete 1-32-byte source tuple into
    /// nonarchitectural state before any destination mutation. A live-host-K
    /// guard bypasses that helper exactly when all applicable writemask bits
    /// are zero. The helper result is copied to a 32-byte stack slot and the
    /// byte-validated original instruction is replayed against `[rsp]`, which
    /// retains native merge/zero and upper-destination clearing semantics.
    pub(crate) fn try_lower_jit_evex_broadcast_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_broadcast_memory_sequence(
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
                op: "EVEX memory broadcast".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 broadcast state".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated EVEX broadcast sequence owns its LEA"),
        };

        let inactive = if let Some(mask) = sequence.encoding.writemask {
            let lanes = sequence.encoding.width.lanes(sequence.encoding.elem);
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            if lanes <= 16 {
                self.emit_opmask_mask_to_rax16(mask);
            } else {
                self.emit_opmask_mask_to_rax64(mask);
            }
            let (applicable, test_width) = match lanes {
                64 => (-1, OpWidth::W64),
                32 => (i64::from(u32::MAX), OpWidth::W32),
                lanes => ((1i64 << lanes) - 1, OpWidth::W64),
            };
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_ri(PhysReg::Rax, applicable, test_width);
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
            sequence.encoding.memory_size,
            true,
            true,
        )?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
        }
        self.emit_jit_vector_scratch_stack_store_256();

        if let Some(inactive) = inactive {
            self.code.emit_u8(0xE9);
            let execute = self.code.position();
            self.code.emit_u32(0);
            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -32);
            }
            self.patch_rel32_to_current(execute)?;
        }

        self.code
            .emit_bytes(sequence.encoding.stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 32);
        }
        Ok(Some(sequence.consumed))
    }
}
