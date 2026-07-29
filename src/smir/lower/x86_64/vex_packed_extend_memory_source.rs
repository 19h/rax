//! Helper-backed VEX packed sign/zero-extension memory-source lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn vex_packed_extend_destination(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            _ => unreachable!("validated VEX packed-extension width"),
        }
    }

    /// Fuse one exact VEX `VPMOVSX*`/`VPMOVZX*` memory decomposition. The MMU
    /// helper reads exactly the architectural 2-, 4-, 8-, or 16-byte source
    /// into the nonarchitectural vector transfer slot before any destination
    /// mutation. A borrowed XMM register carries that zero-padded value into
    /// the byte-validated register form and is then restored completely.
    pub(crate) fn try_lower_jit_vex_packed_extend_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_packed_extend_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let address = match &block.ops[index + 2].kind {
            OpKind::Lea { addr, .. } => addr,
            _ => unreachable!("validated VEX packed-extension sequence contains a leading Lea"),
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

        let scratch_index = (0..16u8)
            .find(|candidate| *candidate != sequence.destination)
            .expect("one VEX destination leaves fifteen scratch registers");
        let scratch = PhysReg::Xmm(scratch_index);
        let destination = Self::vex_packed_extend_destination(sequence.destination, sequence.width);

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        // Every VEX packed-extension source is at most 128 bits. The helper
        // zero-pads the complete nonarchitectural slot beyond memory_size.
        self.emit_jit_vector_scratch_load(scratch, VecWidth::V128);
        self.emit_vec_rr(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: sequence.opcode,
                width: sequence.width,
                // Intel defines VEX.W as ignored; preserve source provenance.
                w: sequence.w,
            },
            destination,
            scratch,
            0,
        );
        self.emit_jit_vector_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop guest RAX

        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(sequence.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
