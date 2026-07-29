//! Helper-backed EVEX VXORPS/VXORPD scalar-broadcast memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{OpWidth, SignExtend, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

impl X86_64Lowerer {
    /// Fuse one exact EVEX VXORPS/VXORPD scalar-broadcast memory
    /// decomposition. The scalar MMU helper stages 4/8 bytes in a 16-byte
    /// nonarchitectural stack slot, and a byte-validated rewrite consumes
    /// `[rsp]{1toN}`. For writemasking, a live applicable-lane test bypasses
    /// the helper when the memory access is architecturally suppressed; the
    /// native instruction still performs merge/zero masking and upper clearing.
    pub(crate) fn try_lower_jit_evex_broadcast_xor_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_broadcast_xor_memory_sequence(
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
                op: "EVEX broadcast VXOR memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX state".to_string(),
            });
        }

        let memory_index = index + sequence.memory_offset;
        let address = match &block.ops[memory_index].kind {
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
            } if *width == sequence.encoding.memory_width => addr,
            _ => unreachable!("validated EVEX broadcast XOR sequence owns its scalar memory op"),
        };

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        let inactive = if let Some(mask) = sequence.encoding.writemask {
            let lanes = sequence.encoding.width.lanes(sequence.encoding.elem);
            debug_assert!(lanes <= 16, "EVEX broadcast XOR applicable opmask width");
            let lane_mask = (1u64 << lanes) - 1;
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax64(mask);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
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
            sequence.encoding.memory_width,
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
        self.code
            .emit_bytes(sequence.encoding.stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }

        Ok(Some(sequence.consumed))
    }
}
