//! Helper-backed writemasked EVEX packed-move memory lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86EvexPackedMoveMemoryKind;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{Address, MemWidth, OpWidth, SignExtend, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

impl X86_64Lowerer {
    #[allow(clippy::too_many_arguments)]
    fn emit_evex_packed_move_store_lane_helper(
        &mut self,
        guest_pc: u64,
        address: &Address,
        mask: u8,
        lane: usize,
        memory_width: MemWidth,
        lane_bytes: i32,
    ) -> Result<(), LowerError> {
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_opmask_mask_to_rax64(mask);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            if lane < 32 {
                emitter.emit_test_ri(PhysReg::Rax, 1i64 << lane, OpWidth::W32);
            } else {
                emitter.emit_shr_ri(
                    PhysReg::Rax,
                    u8::try_from(lane).expect("at most 64 packed-move lanes"),
                    OpWidth::W64,
                );
                emitter.emit_test_ri(PhysReg::Rax, 1, OpWidth::W64);
            }
        }
        let inactive = self.emit_jcc_placeholder(X86Cond::E);
        self.code.emit_u8(0x58); // pop guest RAX
        self.code.emit_u8(0x9D); // restore exact pre-guard flags

        let lane_offset = i32::try_from(lane).expect("at most 64 packed-move lanes") * lane_bytes;
        self.emit_jit_mem_op_linear_offset(
            guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(16 + lane_offset),
            address,
            memory_width,
            SignExtend::Zero,
            EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
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

    /// Fuse one exact writemasked EVEX packed move with a memory operand.
    ///
    /// Loads issue ascending 1/2/4/8-byte helper reads only for active lanes,
    /// accumulate them in nonarchitectural stack state, and commit the vector
    /// once with the byte-validated masked replay. Stores snapshot the complete
    /// source vector before the first helper and issue ascending active-lane
    /// writes, preserving the interpreter's partial-completion boundary. Type
    /// E1 aligned forms first execute their unconditional guest-address guard;
    /// their private stack replay is deliberately unaligned.
    pub(crate) fn try_lower_jit_evex_packed_move_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_packed_move_memory_sequence(
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
                op: "EVEX packed move memory".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 packed moves".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated packed-move sequence owns its address LEA"),
        };
        if let Some(alignment) = sequence.encoding.alignment {
            self.emit_x86_check_alignment(block.ops[index].guest_pc, address, alignment)?;
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(
                PhysReg::Rsp,
                PhysReg::Rsp,
                -EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
            );
            if sequence.encoding.kind == X86EvexPackedMoveMemoryKind::Load {
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
        }

        let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as usize;
        let (memory_width, copy_width, lane_bytes) =
            Self::evex_e4_memory_element_widths(sequence.encoding.elem)?;
        match sequence.encoding.kind {
            X86EvexPackedMoveMemoryKind::Load => {
                for lane in 0..lanes {
                    self.emit_evex_masked_e4_memory_lane_helper(
                        block.ops[index].guest_pc,
                        address,
                        sequence.encoding.writemask,
                        lane,
                        memory_width,
                        copy_width,
                        lane_bytes,
                    )?;
                }
                self.code
                    .emit_bytes(sequence.encoding.stack_instruction.as_slice());
            }
            X86EvexPackedMoveMemoryKind::Store => {
                // The classifier cleared aaa, so this snapshots every source
                // lane irrespective of the architectural store mask.
                self.code
                    .emit_bytes(sequence.encoding.stack_instruction.as_slice());
                for lane in 0..lanes {
                    self.emit_evex_packed_move_store_lane_helper(
                        block.ops[index].guest_pc,
                        address,
                        sequence.encoding.writemask,
                        lane,
                        memory_width,
                        lane_bytes,
                    )?;
                }
            }
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, EVEX_E4_MASKED_VECTOR_FRAME_SIZE);
        }
        Ok(Some(sequence.consumed))
    }
}
