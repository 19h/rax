//! Helper-backed VEX scalar-insert memory-source lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer, X86Emitter};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{OpWidth, VReg, VecWidth};
use crate::smir::ir::{X86VexScalarInsertMemoryFields, X86VexScalarInsertMemoryKind};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn vex_scalar_insert_encoding(fields: X86VexScalarInsertMemoryFields) -> (X86VecMap, u8) {
        match fields.kind {
            X86VexScalarInsertMemoryKind::Vpinsrw => (X86VecMap::Map0F, 0xC4),
            X86VexScalarInsertMemoryKind::Vpinsrb => (X86VecMap::Map0F3A, 0x20),
            X86VexScalarInsertMemoryKind::Vinsertps => (X86VecMap::Map0F3A, 0x21),
            X86VexScalarInsertMemoryKind::Vpinsrd | X86VexScalarInsertMemoryKind::Vpinsrq => {
                (X86VecMap::Map0F3A, 0x22)
            }
        }
    }

    /// Fuse one complete canonical VEX scalar-insert memory decomposition.
    ///
    /// The precise MMU helper commits only the nonarchitectural vector
    /// transfer slot. VPINSR* moves its low scalar bits through preserved host
    /// RAX; VINSERTPS uses scratch XMM lane zero and clears imm8[7:6] in the
    /// rewritten register form because a memory source always selects its only
    /// scalar lane. The borrowed vector register is restored in full before
    /// native execution continues.
    pub(crate) fn try_lower_jit_vex_scalar_insert_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_scalar_insert_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index].kind {
            OpKind::Load { addr, .. } => addr,
            _ => unreachable!("validated VEX scalar-insert sequence starts with Load"),
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

        let fields = sequence.encoding;
        let scratch_index = (0..16u8)
            .find(|candidate| *candidate != fields.destination && *candidate != fields.source1)
            .expect("two VEX operands leave at least fourteen scratch registers");
        let scratch = PhysReg::Xmm(scratch_index);
        let destination = PhysReg::Xmm(fields.destination);
        let source1 = PhysReg::Xmm(fields.source1);
        let (map, opcode) = Self::vex_scalar_insert_encoding(fields);

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, VecWidth::V128);

        let (source2, immediate) = if fields.kind == X86VexScalarInsertMemoryKind::Vinsertps {
            // Memory INSERTPS has no source-lane selector. Scratch lane zero
            // contains the helper-loaded dword, so clear the register-form
            // Count_S field while retaining Count_D and the zero mask.
            (scratch, fields.immediate & 0x3F)
        } else {
            let transfer_width = if fields.kind == X86VexScalarInsertMemoryKind::Vpinsrq {
                OpWidth::W64
            } else {
                OpWidth::W32
            };
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_movd_q_rr(0x7E, scratch, PhysReg::Rax, transfer_width);
            }
            (PhysReg::Rax, fields.immediate)
        };
        self.emit_vec_rrr_imm(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map,
                pp: X86SsePrefix::OpSize,
                opcode,
                width: VecWidth::V128,
                w: fields.w,
            },
            destination,
            source1,
            source2,
            immediate,
        );

        // VPINSR* reused host RAX for the helper-loaded scalar. Re-establish
        // the GuestRegs base required by scratch restoration; guest RAX
        // remains preserved on the native stack.
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop guest RAX
        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(fields.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
