//! Helper-backed VEX binary/FMA3 memory-source lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn vex_binary_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            _ => unreachable!("validated VEX binary width"),
        }
    }

    /// Fuse one exact VEX packed/scalar binary or packed FMA3 memory-source
    /// sequence. The MMU helper commits only a nonarchitectural transfer slot.
    /// One low vector register not named by the guest instruction carries that
    /// value for the native operation and is restored in full before
    /// continuation.
    pub(crate) fn try_lower_jit_vex_binary_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_binary_memory_sequence(
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
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VEX binary sequence starts with a memory load"),
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
            .find(|candidate| *candidate != sequence.destination && *candidate != sequence.source1)
            .expect("two VEX operands leave at least fourteen scratch registers");
        let scratch = Self::vex_binary_phys_reg(scratch_index, sequence.width);
        let destination = Self::vex_binary_phys_reg(sequence.destination, sequence.width);
        let source1 = Self::vex_binary_phys_reg(sequence.source1, sequence.width);

        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, sequence.width);
        self.emit_vec_rrr(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map: sequence.map,
                pp: sequence.prefix,
                opcode: sequence.opcode,
                width: sequence.width,
                w: sequence.w,
            },
            destination,
            source1,
            scratch,
        );
        self.emit_jit_vector_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop rax

        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(sequence.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
