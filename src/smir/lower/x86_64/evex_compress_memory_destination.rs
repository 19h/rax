//! Helper-backed EVEX packed compress memory-destination lowering.

use std::collections::HashMap;

use super::evex_packed_rotate_memory_source::EVEX_E4_MASKED_VECTOR_FRAME_SIZE;
use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{Address, MemWidth, OpWidth, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

/// Bytes 0..64 hold the dense native compress result, bytes 64..72 remain
/// available to the scalar helper ABI, and the final qword holds
/// popcount(KL).
const COMPRESS_ACTIVE_COUNT_OFFSET: i32 = 72;

impl X86_64Lowerer {
    #[allow(clippy::too_many_arguments)]
    fn emit_evex_compress_dense_slot_store_helper(
        &mut self,
        guest_pc: u64,
        address: &Address,
        slot: usize,
        memory_width: MemWidth,
        lane_bytes: i32,
        guarded: bool,
    ) -> Result<(), LowerError> {
        let inactive = if guarded {
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(
                    PhysReg::Rax,
                    PhysReg::Rsp,
                    16 + COMPRESS_ACTIVE_COUNT_OFFSET,
                    OpWidth::W64,
                );
                emitter.emit_cmp_ri(
                    PhysReg::Rax,
                    i64::try_from(slot).expect("at most 64 packed compress lanes"),
                    OpWidth::W64,
                );
            }
            // Dense slot n is written iff popcount(KL) > n.
            let inactive = self.emit_jcc_placeholder(X86Cond::Be);
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
            Some(inactive)
        } else {
            None
        };

        let slot_offset =
            i32::try_from(slot).expect("at most 64 packed compress lanes") * lane_bytes;
        self.emit_jit_mem_op_linear_offset_packed_stack_store(
            guest_pc,
            16 + slot_offset,
            address,
            memory_width,
            EVEX_E4_MASKED_VECTOR_FRAME_SIZE,
            slot_offset,
        )?;

        if let Some(inactive) = inactive {
            self.code.emit_u8(0xE9);
            let done = self.code.position();
            self.code.emit_u32(0);

            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
            self.patch_rel32_to_current(done)?;
        }
        Ok(())
    }

    /// Fuse one exact VCOMPRESSPS/PD or VPCOMPRESSB/W/D/Q memory
    /// decomposition.
    ///
    /// A byte-validated native replay first writes the dense selected vector
    /// to private stack storage. Scalar guest-memory helpers then commit its
    /// elements in ascending order. A later helper fault therefore retains
    /// every preceding dense write, exits at the source PC, and leaves all
    /// architectural registers, flags, and MXCSR unchanged. A zero live mask
    /// issues no guest-memory helper call.
    pub(crate) fn try_lower_jit_evex_compress_memory_destination(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_evex_compress_memory_sequence(
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
                op: "EVEX packed compress memory destination".to_string(),
                operand: "AVX-only vector bridge cannot carry AVX-512 compress state".to_string(),
            });
        }
        let address = match &block.ops[index + sequence.address_offset].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated packed compress sequence starts with LEA"),
        };
        let lanes = sequence.encoding.width.lanes(sequence.encoding.elem) as usize;
        let (memory_width, _, lane_bytes) =
            Self::evex_e4_memory_element_widths(sequence.encoding.elem)?;

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

        let guarded = if let Some(mask) = sequence.encoding.writemask {
            // Compute the dense output length once. The repository's
            // x86-64-v3 baseline includes POPCNT; architectural RAX and flags
            // are restored before the first guest-memory helper executes.
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax64(mask);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                if lanes < 64 {
                    emitter.emit_and_ri(PhysReg::Rax, ((1u64 << lanes) - 1) as i64, OpWidth::W64);
                }
                emitter.emit_popcnt(PhysReg::Rax, PhysReg::Rax, OpWidth::W64);
                emitter.emit_mov_mr(
                    PhysReg::Rsp,
                    16 + COMPRESS_ACTIVE_COUNT_OFFSET,
                    PhysReg::Rax,
                    OpWidth::W64,
                );
            }
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-count flags
            true
        } else {
            false
        };

        for slot in 0..lanes {
            self.emit_evex_compress_dense_slot_store_helper(
                block.ops[index].guest_pc,
                address,
                slot,
                memory_width,
                lane_bytes,
                guarded,
            )?;
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, EVEX_E4_MASKED_VECTOR_FRAME_SIZE);
        }
        Ok(Some(sequence.consumed))
    }
}
