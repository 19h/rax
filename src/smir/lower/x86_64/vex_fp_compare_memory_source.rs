//! Helper-backed VEX floating-point comparison memory-source lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{VReg, VecElementType, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    fn vex_fp_compare_phys_reg(index: u8, width: VecWidth) -> PhysReg {
        match width {
            VecWidth::V128 => PhysReg::Xmm(index),
            VecWidth::V256 => PhysReg::Ymm(index),
            _ => unreachable!("validated VEX floating-point comparison width"),
        }
    }

    /// Fuse one exact packed or scalar VEX floating-point comparison
    /// memory-source decomposition. The MMU helper commits only the
    /// nonarchitectural vector transfer slot. A register-source comparison
    /// consumes that value from a borrowed low vector register, which is
    /// restored completely before native execution continues.
    ///
    /// Guest MXCSR is restored after the helper and remains active for the
    /// native comparison. The enclosing vector trampoline captures any IE/DE
    /// status accrued by the comparison. CPU-level native admission separately
    /// requires all six exception masks, preventing host #XM/SIGFPE from
    /// escaping the precise guest frontier.
    pub(crate) fn try_lower_jit_vex_fp_compare_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_vex_fp_compare_memory_sequence(
            block,
            index,
            true,
            &self.x86_instruction_bytes,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let memory_index = index
            + usize::from(sequence.scalar && matches!(&block.ops[index].kind, OpKind::Mov { .. }));
        let address = match &block.ops[memory_index].kind {
            OpKind::Load { addr, .. } | OpKind::VLoad { addr, .. } => addr,
            _ => unreachable!("validated VEX floating-point comparison starts with a load"),
        };
        self.emit_jit_vector_mem_helper(
            block.ops[memory_index].guest_pc,
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
        let scratch = Self::vex_fp_compare_phys_reg(scratch_index, sequence.width);
        let destination = Self::vex_fp_compare_phys_reg(sequence.destination, sequence.width);
        let source1 = Self::vex_fp_compare_phys_reg(sequence.source1, sequence.width);
        let prefix = match (sequence.scalar, sequence.elem) {
            (false, VecElementType::F32) => X86SsePrefix::None,
            (false, VecElementType::F64) => X86SsePrefix::OpSize,
            (true, VecElementType::F32) => X86SsePrefix::Rep,
            (true, VecElementType::F64) => X86SsePrefix::Repne,
            _ => unreachable!("validated floating-point comparison element"),
        };

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, sequence.width);
        self.emit_vec_rrr_imm(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map: X86VecMap::Map0F,
                pp: prefix,
                opcode: 0xC2,
                width: sequence.width,
                w: sequence.w,
            },
            destination,
            source1,
            scratch,
            sequence.predicate,
        );
        self.emit_jit_vector_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop guest RAX

        if self.avx_ymm16_vector_state {
            self.emit_avx_ymm16_state_backed_upper_clear(sequence.destination);
        }
        Ok(Some(sequence.consumed))
    }
}
