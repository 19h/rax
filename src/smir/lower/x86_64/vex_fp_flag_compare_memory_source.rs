//! Helper-backed VEX floating-point flag-compare memory-source lowering.

use std::collections::HashMap;

use super::{VecEncoding, VecEncodingKind, X86_64Lowerer};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::ops::{OpKind, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{VReg, VecElementType, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_JIT_VECTOR_SCRATCH_INDEX};

impl X86_64Lowerer {
    /// Fuse one exact VEX scalar floating-point flag-comparison memory-source
    /// decomposition.
    ///
    /// The MMU helper commits only the nonarchitectural vector transfer slot.
    /// The native register-source comparison consumes that value from a
    /// borrowed low XMM register, which is restored without modifying the
    /// comparison result flags. Guest MXCSR remains active for the comparison,
    /// and the enclosing vector trampoline captures accrued IE/DE status.
    pub(crate) fn try_lower_jit_vex_fp_flag_compare_memory_source(
        &mut self,
        block: &SmirBlock,
        index: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) =
            crate::smir::lower::runtime::x86_jit_vex_fp_flag_compare_memory_sequence(
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
        let address = match &block.ops[index].kind {
            OpKind::Load { addr, .. } => addr,
            _ => unreachable!("validated VEX floating-point flag compare starts with a load"),
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
            .find(|candidate| *candidate != sequence.source1)
            .expect("one VEX source leaves at least fifteen scratch registers");
        let scratch = PhysReg::Xmm(scratch_index);
        let source1 = PhysReg::Xmm(sequence.source1);
        let prefix = match sequence.elem {
            VecElementType::F32 => X86SsePrefix::None,
            VecElementType::F64 => X86SsePrefix::OpSize,
            _ => unreachable!("validated floating-point flag-comparison element"),
        };
        let opcode = if sequence.signaling { 0x2F } else { 0x2E };

        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.emit_jit_vector_scratch_load(scratch, VecWidth::V128);
        self.emit_vec_rr(
            VecEncoding {
                kind: VecEncodingKind::Vex,
                map: X86VecMap::Map0F,
                pp: prefix,
                opcode,
                width: VecWidth::V128,
                w: sequence.w,
            },
            source1,
            scratch,
            0,
        );
        self.emit_jit_vector_scratch_restore(scratch_index);
        self.code.emit_u8(0x58); // pop guest RAX without modifying compare flags

        Ok(Some(sequence.consumed))
    }
}
