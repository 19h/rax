//! Helper-backed writemasked EVEX packed-logical memory lowering.

use std::collections::HashMap;

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{Address, OpWidth, SignExtend, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

// Scalar load helpers stage a complete 8-byte return. A final 4-byte lane
// begins at byte 60, so bytes 64..67 must remain caller-owned. Preserve the
// trampoline's 16-byte stack alignment with one 80-byte frame.
const FRAME_SIZE: i32 = 80;

impl X86_64Lowerer {
    fn emit_evex_masked_logic_lane_helper(
        &mut self,
        guest_pc: u64,
        address: &Address,
        mask: u8,
        lane: usize,
        element_bytes: i32,
        memory_width: crate::smir::ir::types::MemWidth,
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

        let lane_offset = lane as i32 * element_bytes;
        self.emit_jit_mem_op_linear_offset(
            guest_pc,
            true,
            None,
            Some(16 + lane_offset),
            None,
            None,
            None,
            address,
            memory_width,
            SignExtend::Zero,
            FRAME_SIZE,
            lane_offset,
        )?;
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        self.patch_rel32_to_current(inactive)?;
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags
        self.patch_rel32_to_current(done)?;
        Ok(())
    }

    /// Fuse one exact writemasked EVEX packed-logical full-vector memory
    /// decomposition.
    ///
    /// Active lanes are loaded in ascending order through scalar MMU helpers;
    /// inactive lanes invoke no helper. All values remain in nonarchitectural
    /// stack state until every active lane succeeds. A byte-validated rewrite
    /// then performs the original merge/zero mask operation from `[rsp]`, so a
    /// helper fault exits at the source guest PC without any destination
    /// commit.
    pub(crate) fn try_lower_jit_evex_masked_logic_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_masked_logic_memory_sequence(
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
                op: "masked EVEX logical memory source".to_string(),
                operand: "AVX-only vector bridge cannot carry EVEX state".to_string(),
            });
        }

        let address_index = index + sequence.address_offset;
        let address = match &block.ops[address_index].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated masked EVEX logical sequence owns its LEA"),
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -FRAME_SIZE);
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

        let element_bytes =
            i32::try_from(sequence.encoding.elem.bytes()).expect("validated 4- or 8-byte element");
        let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as usize;
        for lane in 0..lanes {
            self.emit_evex_masked_logic_lane_helper(
                block.ops[index].guest_pc,
                address,
                sequence.encoding.writemask,
                lane,
                element_bytes,
                sequence.encoding.memory_width,
            )?;
        }

        self.code
            .emit_bytes(sequence.encoding.stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, FRAME_SIZE);
        }
        Ok(Some(sequence.consumed))
    }
}
