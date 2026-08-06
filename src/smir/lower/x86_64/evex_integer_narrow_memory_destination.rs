//! Helper-backed EVEX integer-narrowing memory-destination lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{MemWidth, SignExtend, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

impl X86_64Lowerer {
    #[allow(clippy::too_many_arguments)]
    fn emit_evex_integer_narrow_unmasked_lane_helper(
        &mut self,
        guest_pc: u64,
        address: &crate::smir::ir::types::Address,
        lane: usize,
        memory_width: MemWidth,
        lane_bytes: i32,
    ) -> Result<(), LowerError> {
        let lane_offset =
            i32::try_from(lane).expect("at most 32 integer-narrow lanes") * lane_bytes;
        // The helper itself pushes guest RAX and flags before reading the
        // stack source, so the private result starts 16 bytes above its live
        // RSP at helper argument construction.
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
        )
    }

    /// Fuse one exact Type-E6 VPMOV*/VPMOVS*/VPMOVUS* memory decomposition.
    ///
    /// An unmasked byte-validated native replay writes every narrowed result
    /// lane to private stack storage. Scalar guest-memory helpers then commit
    /// fixed destination positions in ascending lane order under the original
    /// writemask. A later fault retains every earlier active-lane write and
    /// exits at the source PC without changing architectural registers,
    /// flags, opmasks, or MXCSR. An empty live mask issues no guest-memory
    /// helper call.
    pub(crate) fn try_lower_jit_evex_integer_narrow_memory_destination(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_evex_integer_narrow_memory_sequence(
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
                op: "EVEX integer-narrow memory destination".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 narrowing state".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated integer-narrow sequence starts with LEA"),
        };
        let lanes = sequence.encoding.width.lanes(sequence.encoding.src_elem) as usize;
        let (memory_width, _, lane_bytes) =
            Self::evex_e4_memory_element_widths(sequence.encoding.dst_elem)?;

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(
                PhysReg::Rsp,
                PhysReg::Rsp,
                -EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
            );
        }
        self.code
            .emit_bytes(sequence.encoding.stack_instruction.as_slice());

        for lane in 0..lanes {
            if let Some(mask) = sequence.encoding.writemask {
                self.emit_evex_packed_move_store_lane_helper(
                    block.ops[index].guest_pc,
                    address,
                    mask,
                    lane,
                    memory_width,
                    lane_bytes,
                )?;
            } else {
                self.emit_evex_integer_narrow_unmasked_lane_helper(
                    block.ops[index].guest_pc,
                    address,
                    lane,
                    memory_width,
                    lane_bytes,
                )?;
            }
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, EVEX_E4_MASKED_VECTOR_FRAME_SIZE);
        }
        Ok(Some(sequence.consumed))
    }
}
